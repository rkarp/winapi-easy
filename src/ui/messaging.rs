//! Window and thread message handling.

use std::{
    io,
    ptr,
};

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

use crate::internal::windows_missing::{
    GET_X_LPARAM,
    GET_Y_LPARAM,
    HIWORD,
    LOWORD,
    NIN_KEYSELECT,
};
use crate::internal::{
    OpaqueClosure,
    catch_unwind_and_abort,
};
use crate::messaging::ThreadMessageLoop;
use crate::ui::menu::MenuHandle;
use crate::ui::{
    Point,
    WindowHandle,
};

#[derive(Clone, PartialEq, Debug)]
pub struct ListenerMessage {
    pub window_handle: WindowHandle,
    pub variant: ListenerMessageVariant,
}

impl ListenerMessage {
    fn from_raw_message(raw_message: RawMessage, window_handle: WindowHandle) -> Self {
        let variant = match raw_message.message {
            value if value >= WM_APP && value <= WM_APP + (u32::from(u8::MAX)) => {
                ListenerMessageVariant::CustomUserMessage {
                    message_id: (raw_message.message - WM_APP).try_into().unwrap(),
                    w_param: raw_message.w_param,
                    l_param: raw_message.l_param,
                }
            }
            RawMessage::ID_NOTIFICATION_ICON_MSG => {
                let icon_id = HIWORD(
                    u32::try_from(raw_message.l_param.0).expect("Icon ID conversion failed"),
                );
                let event_code: u32 = LOWORD(
                    u32::try_from(raw_message.l_param.0).expect("Event code conversion failed"),
                )
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
                        ListenerMessageVariant::NotificationIconSelect { icon_id, xy_coords }
                    }
                    // Works both with mouse right click and the context menu key.
                    WM_CONTEXTMENU => {
                        ListenerMessageVariant::NotificationIconContextSelect { icon_id, xy_coords }
                    }
                    _ => ListenerMessageVariant::Other,
                }
            }
            WM_MENUCOMMAND => {
                let menu_handle = MenuHandle::from_maybe_null(HMENU(
                    ptr::with_exposed_provenance_mut(raw_message.l_param.0.cast_unsigned()),
                ))
                .expect("Menu handle should not be null here");
                let selected_item_id = menu_handle
                    .get_item_id(raw_message.w_param.0.try_into().unwrap())
                    .unwrap();
                ListenerMessageVariant::MenuCommand { selected_item_id }
            }
            WM_SIZE => {
                if raw_message.w_param.0 == SIZE_MINIMIZED.try_into().unwrap() {
                    ListenerMessageVariant::WindowMinimized
                } else {
                    ListenerMessageVariant::Other
                }
            }
            WM_CLOSE => ListenerMessageVariant::WindowClose,
            WM_DESTROY => ListenerMessageVariant::WindowDestroy,
            _ => ListenerMessageVariant::Other,
        };
        ListenerMessage {
            window_handle,
            variant,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ListenerMessageVariant {
    MenuCommand {
        selected_item_id: u32,
    },
    WindowMinimized,
    WindowClose,
    WindowDestroy,
    NotificationIconSelect {
        icon_id: u16,
        xy_coords: Point,
    },
    NotificationIconContextSelect {
        icon_id: u16,
        xy_coords: Point,
    },
    CustomUserMessage {
        message_id: u8,
        w_param: WPARAM,
        l_param: LPARAM,
    },
    Other,
}

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

pub(crate) type WmlOpaqueClosure<'a> = OpaqueClosure<'a, ListenerMessage, ListenerAnswer>;

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

pub(crate) unsafe extern "system" fn generic_window_proc(
    h_wnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
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
        let listener_result = unsafe { window.get_user_data_ptr::<WmlOpaqueClosure>() }.and_then(
            |mut listener_ptr| {
                let listener_message = ListenerMessage::from_raw_message(raw_message, window);
                if !matches!(listener_message.variant, ListenerMessageVariant::Other) {
                    ThreadMessageLoop::ENABLE_CALLBACK_ONCE.with(|x| x.set(true));
                }
                // Many messages won't go through the thread message loop, so we need to notify it.
                // No chance of an infinite loop here since the window procedure won't be called for messages with no associated windows.
                RawMessage::post_loop_wakeup_message().unwrap();
                unsafe { listener_ptr.as_mut() }.to_closure()(listener_message).to_raw_lresult()
            },
        );

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
