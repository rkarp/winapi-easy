//! Various hooking functionality.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{
    Mutex,
    OnceLock,
};
use std::{
    io,
    ptr,
};

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use windows::Win32::Foundation::{
    HWND,
    LPARAM,
    LRESULT,
    POINT,
    WPARAM,
};
use windows::Win32::UI::Accessibility::{
    HWINEVENTHOOK,
    SetWinEventHook,
    UnhookWinEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx,
    EVENT_MIN,
    EVENT_SYSTEM_END,
    EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND,
    EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MOVESIZESTART,
    HHOOK,
    KBDLLHOOKSTRUCT,
    MSLLHOOKSTRUCT,
    SetWindowsHookExW,
    UnhookWindowsHookEx,
    WH_KEYBOARD_LL,
    WH_MOUSE_LL,
    WINDOWS_HOOK_ID,
    WINEVENT_OUTOFCONTEXT,
    WM_KEYDOWN,
    WM_KEYUP,
    WM_LBUTTONDOWN,
    WM_LBUTTONUP,
    WM_MBUTTONDOWN,
    WM_MBUTTONUP,
    WM_MOUSEMOVE,
    WM_MOUSEWHEEL,
    WM_RBUTTONDOWN,
    WM_RBUTTONUP,
    WM_SYSKEYDOWN,
    WM_SYSKEYUP,
    WM_XBUTTONDOWN,
    WM_XBUTTONUP,
};

#[expect(clippy::wildcard_imports)]
use self::private::*;
use crate::input::{
    KeyboardKey,
    MouseButton,
    MouseScrollEvent,
};
use crate::internal::windows_missing::HIWORD;
use crate::internal::{
    RawBox,
    ReturnValue,
    catch_unwind_and_abort,
};
use crate::messaging::ThreadMessageLoop;
use crate::ui::window::WindowHandle;

/// Deployed low level input hook.
///
/// The hook will be removed when this struct is dropped.
#[must_use]
pub struct LowLevelInputHook<HT: HookType, F> {
    #[expect(dead_code)]
    handle: HookHandle<HT::ClosureStore, F, HHOOK>,
}

impl<HT: HookType, F> LowLevelInputHook<HT, F> {
    fn new<const ID: IdType>(user_callback: F) -> io::Result<Self>
    where
        F: FnMut(HT::Message) -> HookReturnValue,
    {
        let handle = HT::add_hook_internal::<ID, _>(user_callback)?;
        Ok(Self { handle })
    }
}

/// A global mouse or keyboard hook.
///
/// This hook can be used to listen to mouse (with [`LowLevelMouseHook`]) or keyboard (with [`LowLevelKeyboardHook`]) events,
/// no matter which application or window they occur in.
pub trait LowLevelInputHookType: HookType + Copy {
    fn run_hook<F>(user_callback: F) -> io::Result<()>
    where
        F: FnMut(Self::Message) -> HookReturnValue,
    {
        // Always using ID 0 only works with ThreadLocalRawClosureStore
        let _handle = Self::add_hook::<0, _>(user_callback)?;
        ThreadMessageLoop::new().run()?;
        Ok(())
    }

    /// Adds a new hook with the given ID.
    ///
    /// A [`ThreadMessageLoop`] must be run separately for `user_callback` to receive events.
    ///
    /// # Panics
    ///
    /// Will panic if a Hook with the given ID already exists for this thread.
    fn add_hook<const ID: IdType, F>(user_callback: F) -> io::Result<LowLevelInputHook<Self, F>>
    where
        F: FnMut(Self::Message) -> HookReturnValue,
    {
        LowLevelInputHook::new::<ID>(user_callback)
    }
}

/// The mouse variant of [`LowLevelInputHook`].
#[derive(Copy, Clone, Debug)]
pub enum LowLevelMouseHook {}

impl HookType for LowLevelMouseHook {
    const TYPE_ID: WINDOWS_HOOK_ID = WH_MOUSE_LL;
    type Message = LowLevelMouseMessage;
    type ClosureStore = ThreadLocalRawClosureStore;
}

impl LowLevelInputHookType for LowLevelMouseHook {}

/// The keyboard variant of [`LowLevelInputHook`].
#[derive(Copy, Clone, Debug)]
pub enum LowLevelKeyboardHook {}

impl HookType for LowLevelKeyboardHook {
    const TYPE_ID: WINDOWS_HOOK_ID = WH_KEYBOARD_LL;
    type Message = LowLevelKeyboardMessage;
    type ClosureStore = ThreadLocalRawClosureStore;
}

impl LowLevelInputHookType for LowLevelKeyboardHook {}

/// Decoded mouse message.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct LowLevelMouseMessage {
    pub action: LowLevelMouseAction,
    pub coords: POINT,
    pub timestamp_ms: u32,
}

impl FromRawLowLevelMessage for LowLevelMouseMessage {
    unsafe fn from_raw_message(value: RawLowLevelMessage) -> Self {
        let w_param = u32::try_from(value.w_param).unwrap();
        let message_data = unsafe {
            &*ptr::with_exposed_provenance::<MSLLHOOKSTRUCT>(value.l_param.cast_unsigned())
        };
        let action = match (w_param, HIWORD(message_data.mouseData)) {
            (WM_MOUSEMOVE, _) => LowLevelMouseAction::Move,
            (WM_LBUTTONDOWN, _) => LowLevelMouseAction::ButtonDown(MouseButton::Left),
            (WM_RBUTTONDOWN, _) => LowLevelMouseAction::ButtonDown(MouseButton::Right),
            (WM_MBUTTONDOWN, _) => LowLevelMouseAction::ButtonDown(MouseButton::Middle),
            (WM_XBUTTONDOWN, 1) => LowLevelMouseAction::ButtonDown(MouseButton::X1),
            (WM_XBUTTONDOWN, 2) => LowLevelMouseAction::ButtonDown(MouseButton::X2),
            (WM_LBUTTONUP, _) => LowLevelMouseAction::ButtonUp(MouseButton::Left),
            (WM_RBUTTONUP, _) => LowLevelMouseAction::ButtonUp(MouseButton::Right),
            (WM_MBUTTONUP, _) => LowLevelMouseAction::ButtonUp(MouseButton::Middle),
            (WM_XBUTTONUP, 1) => LowLevelMouseAction::ButtonUp(MouseButton::X1),
            (WM_XBUTTONUP, 2) => LowLevelMouseAction::ButtonUp(MouseButton::X2),
            (WM_MOUSEWHEEL, raw_movement) => LowLevelMouseAction::WheelScroll(
                MouseScrollEvent::from_raw_movement(raw_movement.cast_signed()),
            ),
            (_, _) => LowLevelMouseAction::Other(w_param),
        };
        LowLevelMouseMessage {
            action,
            coords: message_data.pt,
            timestamp_ms: message_data.time,
        }
    }
}

/// Decoded keyboard message.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct LowLevelKeyboardMessage {
    pub action: LowLevelKeyboardAction,
    pub key: KeyboardKey,
    pub scan_code: u32,
    pub timestamp_ms: u32,
}

impl FromRawLowLevelMessage for LowLevelKeyboardMessage {
    unsafe fn from_raw_message(value: RawLowLevelMessage) -> Self {
        let w_param = u32::try_from(value.w_param).unwrap();
        let message_data = unsafe {
            &*ptr::with_exposed_provenance::<KBDLLHOOKSTRUCT>(value.l_param.cast_unsigned())
        };
        let key = KeyboardKey::from(u16::try_from(message_data.vkCode).expect("Key code too big"));
        let action = LowLevelKeyboardAction::from(w_param);
        LowLevelKeyboardMessage {
            action,
            key,
            scan_code: message_data.scanCode,
            timestamp_ms: message_data.time,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LowLevelMouseAction {
    Move,
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
    WheelScroll(MouseScrollEvent),
    Other(u32),
}

#[derive(FromPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum LowLevelKeyboardAction {
    /// A key press event, possibly auto-repeated by the keyboard.
    KeyDown = WM_KEYDOWN,
    KeyUp = WM_KEYUP,
    SysKeyDown = WM_SYSKEYDOWN,
    SysKeyUp = WM_SYSKEYUP,
    #[num_enum(catch_all)]
    Other(u32),
}

/// A value indicating what action should be taken after returning from the user callback
/// in [`LowLevelInputHook::run_hook`].
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum HookReturnValue {
    /// Returns the result of calling [`CallNextHookEx`] with the original raw message,
    /// allowing further processing by other hooks.
    #[default]
    CallNextHook,
    /// Prevents the event from being passed on to the target window procedure or the rest of the hook chain.
    BlockMessage,
    /// Passes the event to the target window procedure but not the rest of the hook chain.
    PassToWindowProcOnly,
    ExplicitValue(LRESULT),
}

mod private {
    #[expect(clippy::wildcard_imports)]
    use super::*;

    #[derive(Clone, Copy, Debug)]
    #[repr(transparent)]
    struct StoredClosurePtr(*mut c_void);

    unsafe impl Send for StoredClosurePtr {}

    impl StoredClosurePtr {
        fn from_closure<F, I, O>(value: *mut F) -> Self
        where
            F: FnMut(I) -> O,
        {
            StoredClosurePtr(value.cast::<c_void>())
        }

        /// Transforms the pointer to an arbitrary closure.
        ///
        /// # Safety
        ///
        /// Unsafe both because any type is supported and because an arbitrary lifetime can be generated.
        unsafe fn to_closure<'a, F, I, O>(self) -> &'a mut F
        where
            F: FnMut(I) -> O,
        {
            unsafe { &mut *(self.0.cast::<F>()) }
        }
    }

    pub type IdType = u32;

    pub trait RawClosureStore {
        unsafe fn get_raw_closure<'a, F, I, O>(id: IdType) -> Option<&'a mut F>
        where
            F: FnMut(I) -> O + Send;

        fn set_raw_closure<F, I, O>(id: IdType, user_callback: Option<*mut F>)
        where
            F: FnMut(I) -> O + Send;
    }

    pub trait RawThreadClosureStore: RawClosureStore {
        unsafe fn get_thread_raw_closure<'a, F, I, O>(id: IdType) -> Option<&'a mut F>
        where
            F: FnMut(I) -> O;

        fn set_thread_raw_closure<F, I, O>(id: IdType, user_callback: Option<*mut F>)
        where
            F: FnMut(I) -> O;
    }

    pub enum ThreadLocalRawClosureStore {}

    impl ThreadLocalRawClosureStore {
        // This should be safe since for the low level mouse and keyboard hooks windows will only use
        // the same thread as the one registering the hook to send messages to the internal callback.
        thread_local! {
            static RAW_CLOSURE: RefCell<HashMap<IdType, StoredClosurePtr>> = RefCell::new(HashMap::new());
        }

        pub(crate) unsafe fn get_thread_raw_closure<'a, F, I, O>(id: IdType) -> Option<&'a mut F>
        where
            F: FnMut(I) -> O,
        {
            let unwrapped_closure: Option<StoredClosurePtr> =
                Self::RAW_CLOSURE.with(|cell| cell.borrow_mut().get(&id).copied());
            let closure: Option<&mut F> = unwrapped_closure.map(|ptr| unsafe { ptr.to_closure() });
            closure
        }

        pub(crate) fn set_thread_raw_closure<F, I, O>(
            id: IdType,
            maybe_user_callback: Option<*mut F>,
        ) where
            F: FnMut(I) -> O,
        {
            Self::RAW_CLOSURE.with(|cell| {
                let mut map_ref = cell.borrow_mut();
                assert_ne!(maybe_user_callback.is_some(), map_ref.contains_key(&id));
                if let Some(user_callback) = maybe_user_callback {
                    map_ref.insert(id, StoredClosurePtr::from_closure(user_callback));
                } else {
                    map_ref.remove(&id);
                }
            });
        }
    }

    impl RawClosureStore for ThreadLocalRawClosureStore {
        unsafe fn get_raw_closure<'a, F, I, O>(id: IdType) -> Option<&'a mut F>
        where
            F: FnMut(I) -> O,
        {
            unsafe { Self::get_thread_raw_closure(id) }
        }

        fn set_raw_closure<F, I, O>(id: IdType, maybe_user_callback: Option<*mut F>)
        where
            F: FnMut(I) -> O,
        {
            Self::set_thread_raw_closure(id, maybe_user_callback);
        }
    }

    impl RawThreadClosureStore for ThreadLocalRawClosureStore {
        unsafe fn get_thread_raw_closure<'a, F, I, O>(id: IdType) -> Option<&'a mut F>
        where
            F: FnMut(I) -> O,
        {
            unsafe { Self::get_thread_raw_closure(id) }
        }

        fn set_thread_raw_closure<F, I, O>(id: IdType, maybe_user_callback: Option<*mut F>)
        where
            F: FnMut(I) -> O,
        {
            Self::set_thread_raw_closure(id, maybe_user_callback);
        }
    }

    pub enum GlobalRawClosureStore {}

    impl GlobalRawClosureStore {
        fn closures() -> &'static Mutex<HashMap<IdType, StoredClosurePtr>> {
            static CLOSURES: OnceLock<Mutex<HashMap<IdType, StoredClosurePtr>>> = OnceLock::new();
            CLOSURES.get_or_init(|| Mutex::new(HashMap::new()))
        }

        unsafe fn get_raw_closure_with_id<'a, F, I, O>(id: IdType) -> Option<&'a mut F>
        where
            F: FnMut(I) -> O + Send,
        {
            let raw_hooks = Self::closures().lock().unwrap();
            let maybe_stored_fn: Option<StoredClosurePtr> = raw_hooks.get(&id).copied();
            let closure: Option<&mut F> = maybe_stored_fn.map(|ptr| unsafe { ptr.to_closure() });
            closure
        }

        fn set_raw_closure_with_id<F, I, O>(id: IdType, user_callback: Option<*mut F>)
        where
            F: FnMut(I) -> O + Send,
        {
            let mut hooks = Self::closures().lock().unwrap();
            assert_ne!(user_callback.is_some(), hooks.contains_key(&id));
            match user_callback {
                Some(user_callback) => {
                    let value = StoredClosurePtr::from_closure(user_callback);
                    hooks.insert(id, value);
                }
                None => {
                    hooks.remove(&id);
                }
            }
        }
    }

    impl RawClosureStore for GlobalRawClosureStore {
        unsafe fn get_raw_closure<'a, F, I, O>(id: IdType) -> Option<&'a mut F>
        where
            F: FnMut(I) -> O + Send,
        {
            unsafe { Self::get_raw_closure_with_id(id) }
        }

        fn set_raw_closure<F, I, O>(id: IdType, user_callback: Option<*mut F>)
        where
            F: FnMut(I) -> O + Send,
        {
            Self::set_raw_closure_with_id(id, user_callback);
        }
    }

    #[derive(Copy, Clone, Debug)]
    pub struct RawLowLevelMessage {
        #[expect(dead_code)]
        pub n_code: u32,
        pub w_param: usize,
        pub l_param: isize,
    }

    pub trait FromRawLowLevelMessage {
        unsafe fn from_raw_message(value: RawLowLevelMessage) -> Self;
    }

    pub trait HookType: Sized {
        const TYPE_ID: WINDOWS_HOOK_ID;
        type Message: FromRawLowLevelMessage;
        type ClosureStore: RawThreadClosureStore;

        /// Registers a hook and returns a handle for auto-drop.
        fn add_hook_internal<const ID: IdType, F>(
            user_callback: F,
        ) -> io::Result<HookHandle<Self::ClosureStore, F, HHOOK>>
        where
            F: FnMut(Self::Message) -> HookReturnValue,
        {
            unsafe extern "system" fn internal_callback<const ID: IdType, HT, F>(
                n_code: i32,
                w_param: WPARAM,
                l_param: LPARAM,
            ) -> LRESULT
            where
                HT: HookType,
                F: FnMut(HT::Message) -> HookReturnValue,
            {
                if n_code < 0 {
                    unsafe { return CallNextHookEx(None, n_code, w_param, l_param) }
                }
                let call = move || {
                    let raw_message = RawLowLevelMessage {
                        n_code: n_code.cast_unsigned(),
                        w_param: w_param.0,
                        l_param: l_param.0,
                    };
                    let message = unsafe { HT::Message::from_raw_message(raw_message) };
                    let maybe_closure: Option<&mut F> =
                        unsafe { HT::ClosureStore::get_thread_raw_closure(ID) };
                    if let Some(closure) = maybe_closure {
                        closure(message)
                    } else {
                        panic!("Callback called without installed hook")
                    }
                };
                let result = catch_unwind_and_abort(call);
                match result {
                    HookReturnValue::CallNextHook => unsafe {
                        CallNextHookEx(None, n_code, w_param, l_param)
                    },
                    HookReturnValue::BlockMessage => LRESULT(1),
                    HookReturnValue::PassToWindowProcOnly => LRESULT(0),
                    HookReturnValue::ExplicitValue(l_result) => l_result,
                }
            }
            let mut user_callback = RawBox::new(user_callback);
            Self::ClosureStore::set_thread_raw_closure(ID, Some(user_callback.as_mut_ptr()));
            let handle = unsafe {
                SetWindowsHookExW(
                    Self::TYPE_ID,
                    Some(internal_callback::<ID, Self, F>),
                    None,
                    0,
                )?
            };
            Ok(HookHandle::new(ID, handle, user_callback))
        }
    }

    pub trait RemovableHookHandle {
        unsafe fn unhook(&mut self) -> io::Result<()>;
    }

    #[derive(Debug)]
    pub struct HookHandle<RCS: RawClosureStore, B, H>
    where
        Self: RemovableHookHandle,
    {
        id: IdType,
        handle: H,
        hook_dependency: RawBox<B>,
        remove_initiated: bool,
        phantom: PhantomData<RCS>,
    }

    #[cfg(test)]
    static_assertions::assert_not_impl_any!(HookHandle<ThreadLocalRawClosureStore, (), HHOOK>: Send, Sync);

    impl<RCS: RawClosureStore, B, H> HookHandle<RCS, B, H>
    where
        Self: RemovableHookHandle,
    {
        pub(crate) fn new(id: IdType, handle: H, hook_dependency: RawBox<B>) -> Self {
            Self {
                id,
                handle,
                hook_dependency,
                remove_initiated: false,
                phantom: PhantomData,
            }
        }

        fn remove(&mut self) -> io::Result<()> {
            if !self.remove_initiated {
                self.remove_initiated = true;
                unsafe { self.unhook()? };
                RCS::set_raw_closure::<fn(_) -> _, (), ()>(self.id, None);
            }
            Ok(())
        }
    }

    impl<RCS: RawClosureStore, B, H> Drop for HookHandle<RCS, B, H>
    where
        Self: RemovableHookHandle,
    {
        fn drop(&mut self) {
            self.remove().unwrap();
            // Manually drop for clarity
            let _ = self.hook_dependency;
        }
    }

    impl<RCS: RawClosureStore, B> RemovableHookHandle for HookHandle<RCS, B, HHOOK> {
        unsafe fn unhook(&mut self) -> io::Result<()> {
            unsafe { UnhookWindowsHookEx(self.handle)? };
            Ok(())
        }
    }

    impl<RCS: RawClosureStore, B> RemovableHookHandle for HookHandle<RCS, B, HWINEVENTHOOK> {
        unsafe fn unhook(&mut self) -> io::Result<()> {
            let _ = unsafe { UnhookWinEvent(self.handle) }
                .if_null_to_error(|| io::ErrorKind::Other.into())?;
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const EXPECTED_MESSAGE: LowLevelMouseMessage = LowLevelMouseMessage {
            action: LowLevelMouseAction::Move,
            coords: POINT { x: 0, y: 0 },
            timestamp_ms: 42,
        };
        const EXPECTED_HOOK_RET_VAL: HookReturnValue = HookReturnValue::BlockMessage;

        #[test]
        fn curr_thread_set_and_retrieve_closure_thread_local() {
            curr_thread_set_and_retrieve_closure::<ThreadLocalRawClosureStore>();
        }

        #[test]
        fn curr_thread_set_and_retrieve_closure_global() {
            curr_thread_set_and_retrieve_closure::<GlobalRawClosureStore>();
        }

        fn curr_thread_set_and_retrieve_closure<CS>()
        where
            CS: RawClosureStore,
        {
            let mut closure = generate_closure();
            check_retrieved_closure::<CS, LowLevelMouseHook, _>(0, &mut closure, EXPECTED_MESSAGE);
        }

        #[test]
        fn new_thread_set_and_retrieve_closure() {
            let mut closure = generate_closure();
            check_retrieved_closure_new_thread::<GlobalRawClosureStore, LowLevelMouseHook, _>(
                1,
                &mut closure,
                EXPECTED_MESSAGE,
            );
        }

        const fn generate_closure()
        -> impl Fn(<LowLevelMouseHook as HookType>::Message) -> HookReturnValue {
            |message| {
                assert_eq!(message, EXPECTED_MESSAGE);
                EXPECTED_HOOK_RET_VAL
            }
        }

        fn check_retrieved_closure<CS, HT, F>(
            id: IdType,
            closure: &mut F,
            expected_message: HT::Message,
        ) where
            CS: RawClosureStore,
            HT: HookType,
            F: FnMut(HT::Message) -> HookReturnValue + Send,
        {
            CS::set_raw_closure(id, Some(closure));
            let retrieved_closure: &mut F = unsafe { CS::get_raw_closure(id) }.unwrap();
            assert_eq!(retrieved_closure(expected_message), EXPECTED_HOOK_RET_VAL)
        }

        fn check_retrieved_closure_new_thread<CS, HT, F>(
            id: IdType,
            closure: &mut F,
            expected_message: HT::Message,
        ) where
            CS: RawClosureStore,
            HT: HookType,
            F: FnMut(HT::Message) -> HookReturnValue + Send,
            <HT as HookType>::Message: Send + 'static,
        {
            CS::set_raw_closure(id, Some(closure));
            std::thread::spawn(move || {
                let retrieved_closure: &mut F = unsafe { CS::get_raw_closure(id) }.unwrap();
                assert_eq!(retrieved_closure(expected_message), EXPECTED_HOOK_RET_VAL)
            })
            .join()
            .unwrap();
        }
    }
}

impl ReturnValue for HWINEVENTHOOK {
    const NULL_VALUE: HWINEVENTHOOK = HWINEVENTHOOK(ptr::null_mut());
}

/// A hook for various UI events.
///
/// The hook will be removed when this struct is dropped.
#[must_use]
pub struct WinEventHook<F> {
    #[expect(dead_code)]
    handle: HookHandle<ThreadLocalRawClosureStore, F, HWINEVENTHOOK>,
}

impl<F> WinEventHook<F>
where
    F: FnMut(WinEventMessage),
{
    /// Adds a new hook with the given ID.
    ///
    /// A [`ThreadMessageLoop`] must be run separately for `user_callback` to receive events.
    ///
    /// # Panics
    ///
    /// Will panic if a Hook with the given ID already exists for this thread.
    pub fn new<const ID: IdType>(user_callback: F) -> io::Result<Self> {
        let handle = Self::add_hook_internal::<ID>(user_callback)?;
        Ok(Self { handle })
    }

    /// Runs a new hook with ID `0` on a new thread message loop ([`ThreadMessageLoop`]).
    ///
    /// This will block the current thread to process messages.
    ///
    /// # Panics
    ///
    /// Will panic if a Hook with ID `0` already exists for this thread
    /// or if the thread message loop lock is already acquired.
    pub fn run_hook_loop(user_callback: F) -> io::Result<()> {
        // Always using ID 0 only works with ThreadLocalRawClosureStore
        let _handle = Self::new::<0>(user_callback)?;
        ThreadMessageLoop::new().run()?;
        Ok(())
    }

    fn add_hook_internal<const ID: IdType>(
        user_callback: F,
    ) -> io::Result<HookHandle<ThreadLocalRawClosureStore, F, HWINEVENTHOOK>> {
        unsafe extern "system" fn internal_callback<const ID: IdType, F>(
            _h_win_event_hook: HWINEVENTHOOK,
            event_id: u32,
            hwnd: HWND,
            id_object: i32,
            id_child: i32,
            _id_event_thread: u32,
            _dwms_event_time: u32,
        ) where
            F: FnMut(WinEventMessage),
        {
            let call = move || {
                let message =
                    unsafe { WinEventMessage::from_raw_event(event_id, hwnd, id_object, id_child) };
                let maybe_closure: Option<&mut F> =
                    unsafe { ThreadLocalRawClosureStore::get_thread_raw_closure(ID) };
                if let Some(closure) = maybe_closure {
                    closure(message);
                } else {
                    panic!("Callback called without installed hook")
                }
            };
            catch_unwind_and_abort(call);
        }
        let mut user_callback = RawBox::new(user_callback);
        ThreadLocalRawClosureStore::set_thread_raw_closure(ID, Some(user_callback.as_mut_ptr()));
        let handle = unsafe {
            SetWinEventHook(
                EVENT_MIN,
                EVENT_SYSTEM_END,
                None,
                Some(internal_callback::<ID, F>),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
            .if_null_to_error(|| io::ErrorKind::Other.into())?
        };
        Ok(HookHandle::new(ID, handle, user_callback))
    }
}

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
#[repr(u32)]
pub enum WinEventKind {
    /// The foreground window changed.
    ///
    /// **Note**: This event is not always sent when a window is unminimized ([`WinEventKind::WindowUnminimized`]).
    ForegroundWindowChanged = EVENT_SYSTEM_FOREGROUND,
    WindowMinimized = EVENT_SYSTEM_MINIMIZESTART,
    /// A window has been unminimized.
    WindowUnminimized = EVENT_SYSTEM_MINIMIZEEND,
    WindowMoveStart = EVENT_SYSTEM_MOVESIZESTART,
    WindowMoveEnd = EVENT_SYSTEM_MOVESIZEEND,
    #[num_enum(catch_all)]
    Other(u32),
}

/// Decoded UI events.
#[derive(Debug)]
pub struct WinEventMessage {
    pub event_kind: WinEventKind,
    pub window_handle: Option<WindowHandle>,
    #[expect(dead_code)]
    object_id: i32,
    #[expect(dead_code)]
    child_id: i32,
}

impl WinEventMessage {
    unsafe fn from_raw_event(event_id: u32, hwnd: HWND, id_object: i32, id_child: i32) -> Self {
        let window_handle = WindowHandle::from_maybe_null(hwnd);
        Self {
            event_kind: WinEventKind::from(event_id),
            window_handle,
            object_id: id_object,
            child_id: id_child,
        }
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        PostThreadMessageW,
        WM_QUIT,
    };

    use super::*;

    #[test]
    fn ll_hook_and_unhook() -> windows::core::Result<()> {
        ll_hook_and_unhook_with_ids::<0, 1>()
    }

    #[test]
    #[should_panic]
    fn ll_hook_and_unhook_duplicate() {
        let _ = ll_hook_and_unhook_with_ids::<0, 0>();
    }

    fn ll_hook_and_unhook_with_ids<const ID1: IdType, const ID2: IdType>()
    -> windows::core::Result<()> {
        let mouse_callback =
            |_message: LowLevelMouseMessage| -> HookReturnValue { HookReturnValue::CallNextHook };
        let mut keyboard_counter = 0;
        let keyboard_callback = |_message: LowLevelKeyboardMessage| -> HookReturnValue {
            keyboard_counter += 1;
            HookReturnValue::CallNextHook
        };
        unsafe {
            PostThreadMessageW(
                GetCurrentThreadId(),
                WM_QUIT,
                WPARAM::default(),
                LPARAM::default(),
            )?
        };
        let _mouse_handle = LowLevelMouseHook::add_hook_internal::<ID1, _>(mouse_callback)?;
        let _keyboard_handle =
            LowLevelKeyboardHook::add_hook_internal::<ID2, _>(keyboard_callback)?;
        ThreadMessageLoop::new().run()?;
        Ok(())
    }

    #[cfg(feature = "process")]
    #[test]
    fn win_event_hook_and_unhook() -> windows::core::Result<()> {
        use crate::process::ThreadId;
        let mut counter = 0;
        let callback = |_message: WinEventMessage| {
            counter += 1;
        };
        ThreadId::current().post_quit_message()?;
        let _hook_handle = WinEventHook::new::<0>(callback)?;
        ThreadMessageLoop::new().run()?;
        Ok(())
    }
}
