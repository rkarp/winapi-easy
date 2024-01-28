//! Keyboard and mouse hooking functionality.

use num_enum::FromPrimitive;
use windows::Win32::Foundation::{
    HMODULE,
    LPARAM,
    LRESULT,
    POINT,
    WPARAM,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx,
    SetWindowsHookExW,
    UnhookWindowsHookEx,
    HHOOK,
    KBDLLHOOKSTRUCT,
    MSLLHOOKSTRUCT,
    WH_KEYBOARD_LL,
    WH_MOUSE_LL,
    WINDOWS_HOOK_ID,
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

use std::cell::Cell;
use std::fmt::Debug;
use std::{
    io,
    ptr,
};

use crate::input::{
    Key,
    MouseButton,
    MouseScrollEvent,
};
use crate::internal::windows_missing::HIWORD;
use crate::internal::{
    catch_unwind_and_abort,
    ReturnValue,
};
use crate::messaging::ThreadMessageLoop;

/// A global mouse or keyboard hook.
///
/// This hook can be used to listen to mouse (with [`LowLevelMouseHook`]) or keyboard (with [`LowLevelKeyboardHook`]) events,
/// no matter which application or window they occur in.
pub trait LowLevelInputHook: Copy {
    const TYPE_ID: WINDOWS_HOOK_ID;
    type Message: From<RawLowLevelMessage<Self>>;
    type RawMessageData: Copy + Debug;

    fn run_hook<F>(user_callback: &mut F) -> io::Result<()>
    where
        F: FnMut(Self::Message, RawLowLevelMessage<Self>) -> HookReturnValue,
    {
        // This should be safe since for the low level mouse and keyboard hooks windows will only use
        // the same thread as the one registering the hook to send messages to the internal callback.
        thread_local! {
            static RAW_CLOSURE: Cell<*mut ()> = Cell::new(ptr::null_mut());
        }
        unsafe extern "system" fn internal_callback<HT, F>(
            code: i32,
            w_param: WPARAM,
            l_param: LPARAM,
        ) -> LRESULT
        where
            HT: LowLevelInputHook + ?Sized,
            F: FnMut(HT::Message, RawLowLevelMessage<HT>) -> HookReturnValue,
        {
            if code < 0 {
                unsafe { return CallNextHookEx(HHOOK::default(), code, w_param, l_param) }
            }
            let call = move || {
                let raw_message: RawLowLevelMessage<HT> =
                    RawLowLevelMessage::<HT>::from_params(code, w_param, l_param);
                let message = HT::Message::from(raw_message);
                let unwrapped_closure: *mut () = RAW_CLOSURE.with(|raw_closure| raw_closure.get());
                let closure: &mut F = &mut *(unwrapped_closure as *mut F);
                closure(message, raw_message)
            };
            let result = catch_unwind_and_abort(call);
            match result {
                HookReturnValue::CallNextHook => unsafe {
                    CallNextHookEx(HHOOK::default(), code, w_param, l_param)
                },
                HookReturnValue::BlockMessage => LRESULT(1),
                HookReturnValue::PassToWindowProcOnly => LRESULT(0),
                HookReturnValue::ExplicitValue(l_result) => l_result,
            }
        }
        RAW_CLOSURE.with(|cell| cell.set(user_callback as *mut F as *mut ()));
        let handle = unsafe {
            SetWindowsHookExW(
                Self::TYPE_ID,
                Some(internal_callback::<Self, F>),
                HMODULE::default(),
                0,
            )?
        };
        ThreadMessageLoop::run_thread_message_loop(|| Ok(()))?;
        let _ = unsafe { UnhookWindowsHookEx(handle).if_null_get_last_error()? };
        Ok(())
    }
}

/// The mouse variant of [`LowLevelInputHook`].
#[derive(Copy, Clone, Debug)]
pub enum LowLevelMouseHook {}

impl LowLevelInputHook for LowLevelMouseHook {
    const TYPE_ID: WINDOWS_HOOK_ID = WH_MOUSE_LL;
    type Message = LowLevelMouseMessage;
    type RawMessageData = MSLLHOOKSTRUCT;
}

/// The keyboard variant of [`LowLevelInputHook`].
#[derive(Copy, Clone, Debug)]
pub enum LowLevelKeyboardHook {}

impl LowLevelInputHook for LowLevelKeyboardHook {
    const TYPE_ID: WINDOWS_HOOK_ID = WH_KEYBOARD_LL;
    type Message = LowLevelKeyboardMessage;
    type RawMessageData = KBDLLHOOKSTRUCT;
}

trait FromCallbackParams: Copy {
    fn from_params(code: i32, w_param: WPARAM, l_param: LPARAM) -> Self;
}

#[derive(Copy, Clone, Debug)]
pub struct RawLowLevelMessage<HT: LowLevelInputHook + ?Sized> {
    pub code: i32,
    pub w_param: u32,
    pub message_data: HT::RawMessageData,
}

impl<HT: LowLevelInputHook + ?Sized> FromCallbackParams for RawLowLevelMessage<HT> {
    fn from_params(code: i32, w_param: WPARAM, l_param: LPARAM) -> Self {
        let w_param = u32::try_from(w_param.0).unwrap();
        let message_data = unsafe { *(l_param.0 as *const HT::RawMessageData) };
        RawLowLevelMessage {
            code,
            w_param,
            message_data,
        }
    }
}

/// Mouse message decoded from [`RawLowLevelMessage`].
#[derive(Copy, Clone, Debug)]
pub struct LowLevelMouseMessage {
    pub action: LowLevelMouseAction,
    pub coords: POINT,
    pub timestamp_ms: u32,
}

impl From<RawLowLevelMessage<LowLevelMouseHook>> for LowLevelMouseMessage {
    fn from(value: RawLowLevelMessage<LowLevelMouseHook>) -> Self {
        let action = match (value.w_param, HIWORD(value.message_data.mouseData)) {
            (WM_MOUSEMOVE, _) => LowLevelMouseAction::Move,
            (WM_LBUTTONDOWN, _) => LowLevelMouseAction::ButtonDown(MouseButton::Left),
            (WM_RBUTTONDOWN, _) => LowLevelMouseAction::ButtonDown(MouseButton::Right),
            (WM_MBUTTONDOWN, _) => LowLevelMouseAction::ButtonDown(MouseButton::Middle),
            (WM_XBUTTONDOWN, button_number) => {
                LowLevelMouseAction::ButtonDown(MouseButton::XButton(button_number))
            }
            (WM_LBUTTONUP, _) => LowLevelMouseAction::ButtonUp(MouseButton::Left),
            (WM_RBUTTONUP, _) => LowLevelMouseAction::ButtonUp(MouseButton::Right),
            (WM_MBUTTONUP, _) => LowLevelMouseAction::ButtonUp(MouseButton::Middle),
            (WM_XBUTTONUP, button_number) => {
                LowLevelMouseAction::ButtonUp(MouseButton::XButton(button_number))
            }
            (WM_MOUSEWHEEL, raw_movement) => {
                LowLevelMouseAction::WheelScroll(MouseScrollEvent::from_raw_movement(raw_movement))
            }
            (_, _) => LowLevelMouseAction::Other(value.w_param),
        };
        LowLevelMouseMessage {
            action,
            coords: value.message_data.pt,
            timestamp_ms: value.message_data.time,
        }
    }
}

/// Keyboard message decoded from [`RawLowLevelMessage`].
#[derive(Copy, Clone, Debug)]
pub struct LowLevelKeyboardMessage {
    pub action: LowLevelKeyboardAction,
    pub key: Key,
    pub scan_code: u32,
    pub timestamp_ms: u32,
}

impl From<RawLowLevelMessage<LowLevelKeyboardHook>> for LowLevelKeyboardMessage {
    fn from(value: RawLowLevelMessage<LowLevelKeyboardHook>) -> Self {
        let key = Key::from(value.message_data.vkCode as u16);
        let action = LowLevelKeyboardAction::from(value.w_param);
        LowLevelKeyboardMessage {
            action,
            key,
            scan_code: value.message_data.scanCode,
            timestamp_ms: value.message_data.time,
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

#[cfg(test)]
mod tests {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        PostThreadMessageW,
        WM_QUIT,
    };

    use super::*;

    #[test]
    fn ll_hook_and_unhook() -> io::Result<()> {
        let mut callback = |_message: LowLevelMouseMessage, _| -> HookReturnValue {
            HookReturnValue::CallNextHook
        };
        let _ = unsafe {
            PostThreadMessageW(
                GetCurrentThreadId(),
                WM_QUIT,
                WPARAM::default(),
                LPARAM::default(),
            )
            .if_null_get_last_error()?
        };
        LowLevelMouseHook::run_hook(&mut callback)?;
        Ok(())
    }
}
