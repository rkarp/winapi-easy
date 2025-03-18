//! Window and thread message handling.

use std::io;

use windows::Win32::Foundation::{
    HWND,
    LPARAM,
    LRESULT,
    WPARAM,
};
use windows::Win32::UI::Shell::NIN_SELECT;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW,
    GetMessagePos,
    HMENU,
    PostMessageW,
    SIZE_MINIMIZED,
    WM_APP,
    WM_CLOSE,
    WM_CONTEXTMENU,
    WM_DESTROY,
    WM_MENUCOMMAND,
    WM_SIZE,
};

use crate::internal::catch_unwind_and_abort;
use crate::internal::windows_missing::{
    GET_X_LPARAM,
    GET_Y_LPARAM,
    HIWORD,
    LOWORD,
    NIN_KEYSELECT,
};
use crate::messaging::ThreadMessageLoop;
use crate::ui::{
    Point,
    WindowHandle,
};
use crate::ui::menu::MenuHandle;

/// Indicates what should be done after the [`WindowMessageListener`] is done processing the message.
#[derive(Copy, Clone, Default, Debug)]
pub enum ListenerAnswer {
    /// Call the default windows handler after the current message processing code.
    #[default]
    CallDefaultHandler,
    /// Message processing is finished, skip calling the windows handler.
    MessageProcessed,
}

impl ListenerAnswer {
    fn to_raw_lresult(self) -> Option<LRESULT> {
        match self {
            ListenerAnswer::CallDefaultHandler => None,
            ListenerAnswer::MessageProcessed => Some(LRESULT(0)),
        }
    }
}

/// A user-defined implementation for various windows message handlers.
///
/// The trait already defines a default for all methods, making it easier to just implement specific ones.
///
/// # Design rationale
///
/// The way the Windows API is structured, it doesn't seem to be possible to use closures here
/// due to [`crate::ui::Window`] and [`crate::ui::WindowClass`] needing type parameters for the [`WindowMessageListener`],
/// making it hard to swap out the listener since every `Fn` has its own type in Rust.
///
/// `Box` with dynamic dispatch `Fn` is also not practical due to allowing only `'static` lifetimes.
pub trait WindowMessageListener {
    /// An item from a window's menu was selected by the user.
    #[allow(unused_variables)]
    #[inline]
    fn handle_menu_command(&self, window: &WindowHandle, selected_item_id: u32) {}
    /// A 'minimize window' action was performed.
    #[allow(unused_variables)]
    #[inline]
    fn handle_window_minimized(&self, window: &WindowHandle) {}
    /// A 'close window' action was performed.
    #[allow(unused_variables)]
    #[inline]
    fn handle_window_close(&self, window: &WindowHandle) -> ListenerAnswer {
        Default::default()
    }
    /// A window was destroyed and removed from the screen.
    #[allow(unused_variables)]
    #[inline]
    fn handle_window_destroy(&self, window: &WindowHandle) {}
    /// A notification icon was selected (triggered).
    #[allow(unused_variables)]
    #[inline]
    fn handle_notification_icon_select(&self, icon_id: u16, xy_coords: Point) {}
    /// A notification icon was context-selected (e.g. right-clicked).
    #[allow(unused_variables)]
    #[inline]
    fn handle_notification_icon_context_select(&self, icon_id: u16, xy_coords: Point) {}
    /// A custom user message was sent.
    #[allow(unused_variables)]
    #[inline]
    fn handle_custom_user_message(
        &self,
        window: &WindowHandle,
        message_id: u8,
        w_param: WPARAM,
        l_param: LPARAM,
    ) {
    }
}

/// A [`WindowMessageListener`] that leaves all handlers to their default empty impls.
#[derive(Copy, Clone, Default, Debug)]
pub struct EmptyWindowMessageListener;

impl WindowMessageListener for EmptyWindowMessageListener {}

#[derive(Copy, Clone, Debug)]
pub(crate) struct RawMessage {
    pub(crate) message: u32,
    pub(crate) w_param: WPARAM,
    pub(crate) l_param: LPARAM,
}

impl RawMessage {
    /// Start of the message range for string message registered by `RegisterWindowMessage`.
    ///
    /// Values between `WM_APP` and this value (exclusive) can be used for private message IDs
    /// that won't conflict with messages from predefined Windows control classes.
    const STR_MSG_RANGE_START: u32 = 0xC000;

    pub(crate) const ID_APP_WAKEUP_MSG: u32 = Self::STR_MSG_RANGE_START - 1;
    pub(crate) const ID_NOTIFICATION_ICON_MSG: u32 = Self::STR_MSG_RANGE_START - 2;

    pub(crate) fn dispatch_to_message_listener<WML: WindowMessageListener>(
        self,
        window: &WindowHandle,
        listener: &WML,
    ) -> Option<LRESULT> {
        // Many messages won't go through the thread message loop, so we need to notify it.
        // No chance of an infinite loop here since the window procedure won't be called for messages with no associated windows.
        Self::post_loop_wakeup_message().unwrap();
        let mut call_message_loop_callback = true;
        let result = match self.message {
            value if value >= WM_APP && value <= WM_APP + (u32::from(u8::MAX)) => {
                listener.handle_custom_user_message(
                    window,
                    (self.message - WM_APP).try_into().unwrap(),
                    self.w_param,
                    self.l_param,
                );
                None
            }
            Self::ID_NOTIFICATION_ICON_MSG => {
                let icon_id =
                    HIWORD(u32::try_from(self.l_param.0).expect("Icon ID conversion failed"));
                let event_code: u32 =
                    LOWORD(u32::try_from(self.l_param.0).expect("Event code conversion failed"))
                        .into();
                let xy_coords = {
                    // `w_param` does contain the coordinates of the click event, but they are not adjusted for DPI scaling, so we can't use them.
                    // Instead we have to call `GetMessagePos`, which will however return mouse coordinates even if the keyboard was used.
                    // An alternative would be to use `NOTIFYICON_VERSION_4`, but that would not allow exposing an API for rich pop-up UIs
                    // when the user hovers over the tray icon since the necessary notifications would not be sent.
                    // See also: https://stackoverflow.com/a/41649787
                    let raw_position = unsafe { GetMessagePos() };
                    get_param_xy_coords(raw_position)
                };
                match event_code {
                    // NIN_SELECT only happens with left clicks. Space will produce 1x NIN_KEYSELECT, Enter 2x NIN_KEYSELECT.
                    NIN_SELECT | NIN_KEYSELECT => {
                        listener.handle_notification_icon_select(icon_id, xy_coords);
                    }
                    // Works both with mouse right click and the context menu key.
                    WM_CONTEXTMENU => {
                        listener.handle_notification_icon_context_select(icon_id, xy_coords);
                    }
                    _ => (),
                }
                None
            }
            WM_MENUCOMMAND => {
                let menu_handle =
                    MenuHandle::from_maybe_null(HMENU(self.l_param.0 as *mut std::ffi::c_void))
                        .expect("Menu handle should not be null here");
                let item_id = menu_handle
                    .get_item_id(self.w_param.0.try_into().unwrap())
                    .unwrap();
                listener.handle_menu_command(window, item_id);
                None
            }
            WM_SIZE => {
                if self.w_param.0 == SIZE_MINIMIZED.try_into().unwrap() {
                    listener.handle_window_minimized(window);
                }
                None
            }
            WM_CLOSE => listener.handle_window_close(window).to_raw_lresult(),
            WM_DESTROY => {
                listener.handle_window_destroy(window);
                None
            }
            _ => {
                call_message_loop_callback = false;
                None
            }
        };
        if call_message_loop_callback {
            ThreadMessageLoop::ENABLE_CALLBACK_ONCE.with(|x| x.set(true));
        }
        result
    }

    /// Posts a message to the thread message queue and returns immediately.
    ///
    /// If no window is given, the window procedure won't be called by `DispatchMessageW`.
    fn post_to_queue(&self, window: Option<&WindowHandle>) -> io::Result<()> {
        unsafe {
            PostMessageW(
                window.map(Into::into),
                self.message,
                self.w_param,
                self.l_param,
            )?;
        }
        Ok(())
    }

    fn post_loop_wakeup_message() -> io::Result<()> {
        let wakeup_message = RawMessage {
            message: Self::ID_APP_WAKEUP_MSG,
            w_param: WPARAM(0),
            l_param: LPARAM(0),
        };
        wakeup_message.post_to_queue(None)
    }
}

pub(crate) unsafe extern "system" fn generic_window_proc<WML>(
    h_wnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT
where
    WML: WindowMessageListener,
{
    let call = move || {
        let window = WindowHandle::from_maybe_null(h_wnd)
            .expect("Window handle given to window procedure should never be NULL");

        let raw_message = RawMessage {
            message,
            w_param,
            l_param,
        };

        // When creating a window, the custom data for the loop is not set yet
        // before the first call to this function
        let listener_result =
            unsafe { window.get_user_data_ptr::<WML>() }.and_then(|listener_ptr| {
                raw_message.dispatch_to_message_listener(&window, unsafe { listener_ptr.as_ref() })
            });

        if let Some(l_result) = listener_result {
            l_result
        } else {
            unsafe { DefWindowProcW(h_wnd, message, w_param, l_param) }
        }
    };
    catch_unwind_and_abort(call)
}

fn get_param_xy_coords(param: u32) -> Point {
    let param = LPARAM(param.try_into().unwrap());
    Point {
        x: GET_X_LPARAM(param),
        y: GET_Y_LPARAM(param),
    }
}
