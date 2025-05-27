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
    WM_COMMAND,
    WM_CONTEXTMENU,
    WM_DESTROY,
    WM_MENUCOMMAND,
    WM_SIZE,
    WM_TIMER,
};

use crate::internal::catch_unwind_and_abort;
use crate::internal::windows_missing::{
    GET_X_LPARAM,
    GET_Y_LPARAM,
    HIWORD,
    LOWORD,
    NIN_KEYSELECT,
};
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
    fn from_known_raw_message(
        raw_message: RawMessage,
        window_handle: WindowHandle,
    ) -> Option<Self> {
        let variant = match raw_message.message {
            value if value >= WM_APP && value <= WM_APP + (u32::from(u8::MAX)) => {
                ListenerMessageVariant::CustomUserMessage(CustomUserMessage {
                    message_id: (raw_message.message - WM_APP)
                        .try_into()
                        .expect("Message ID should be in u8 range"),
                    w_param: raw_message.w_param.0,
                    l_param: raw_message.l_param.0,
                })
                .into()
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
                        ListenerMessageVariant::NotificationIconSelect { icon_id, xy_coords }.into()
                    }
                    // Works both with mouse right click and the context menu key.
                    WM_CONTEXTMENU => {
                        ListenerMessageVariant::NotificationIconContextSelect { icon_id, xy_coords }
                            .into()
                    }
                    _ => None,
                }
            }
            WM_COMMAND if HIWORD(u32::try_from(raw_message.w_param.0).unwrap()) == 0 => {
                // Not preferable since unly u16 IDs are supported
                ListenerMessageVariant::MenuCommand {
                    selected_item_id: u32::from(LOWORD(
                        u32::try_from(raw_message.w_param.0).unwrap(),
                    )),
                }
                .into()
            }
            WM_MENUCOMMAND => {
                let menu_handle = MenuHandle::from_maybe_null(HMENU(
                    ptr::with_exposed_provenance_mut(raw_message.l_param.0.cast_unsigned()),
                ))
                .expect("Menu handle should not be null here");
                let selected_item_id = menu_handle
                    .get_item_id(raw_message.w_param.0.try_into().unwrap())
                    .unwrap();
                ListenerMessageVariant::MenuCommand { selected_item_id }.into()
            }
            WM_SIZE => {
                if raw_message.w_param.0 == SIZE_MINIMIZED.try_into().unwrap() {
                    ListenerMessageVariant::WindowMinimized.into()
                } else {
                    None
                }
            }
            WM_TIMER => ListenerMessageVariant::Timer {
                timer_id: raw_message.w_param.0,
            }
            .into(),
            WM_CLOSE => ListenerMessageVariant::WindowClose.into(),
            WM_DESTROY => ListenerMessageVariant::WindowDestroy.into(),
            _ => None,
        };
        variant.map(|variant| ListenerMessage {
            window_handle,
            variant,
        })
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
    Timer {
        timer_id: usize,
    },
    /// Message generated from raw message ID values between `WM_APP` and `WM_APP + u8::MAX` exclusive.
    ///
    /// Message ID `0` represents the raw value `WM_APP`.
    CustomUserMessage(CustomUserMessage),
}

/// Indicates what should be done after the [`WindowMessageListener`] is done processing the message.
#[derive(Copy, Clone, Default, Debug)]
pub enum ListenerAnswer {
    /// Call the default windows handler after the current message processing code.
    #[default]
    CallDefaultHandler,
    /// Message processing is finished, skip calling the windows handler.
    StopMessageProcessing,
}

impl ListenerAnswer {
    fn to_raw_lresult(self) -> Option<LRESULT> {
        match self {
            ListenerAnswer::CallDefaultHandler => None,
            ListenerAnswer::StopMessageProcessing => Some(LRESULT(0)),
        }
    }
}

pub(crate) type WmlOpaqueClosure<'a> = Box<dyn FnMut(&ListenerMessage) -> ListenerAnswer + 'a>;

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

    pub(crate) const ID_WINDOW_PROC_MSG: u32 = Self::STR_MSG_RANGE_START - 1;
    pub(crate) const ID_APP_WAKEUP_MSG: u32 = Self::STR_MSG_RANGE_START - 2;
    pub(crate) const ID_NOTIFICATION_ICON_MSG: u32 = Self::STR_MSG_RANGE_START - 3;

    /// Posts a message to the thread message queue and returns immediately.
    ///
    /// If no window is given, the window procedure won't be called by `DispatchMessageW`.
    pub(crate) fn post_to_queue(&self, window: Option<WindowHandle>) -> io::Result<()> {
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

    fn post_window_proc_message(listener_message: ListenerMessage) -> io::Result<()> {
        let ptr_usize = Box::into_raw(Box::new(listener_message)).expose_provenance();
        let window_proc_message = RawMessage {
            message: Self::ID_WINDOW_PROC_MSG,
            w_param: WPARAM(ptr_usize),
            l_param: LPARAM(0),
        };
        window_proc_message.post_to_queue(None)
    }

    #[expect(dead_code)]
    fn post_loop_wakeup_message() -> io::Result<()> {
        let wakeup_message = RawMessage {
            message: Self::ID_APP_WAKEUP_MSG,
            w_param: WPARAM(0),
            l_param: LPARAM(0),
        };
        wakeup_message.post_to_queue(None)
    }
}

impl From<CustomUserMessage> for RawMessage {
    fn from(custom_message: CustomUserMessage) -> Self {
        RawMessage {
            message: WM_APP + u32::from(custom_message.message_id),
            w_param: WPARAM(custom_message.w_param),
            l_param: LPARAM(custom_message.l_param),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct CustomUserMessage {
    pub message_id: u8,
    pub w_param: usize,
    pub l_param: isize,
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

        let listener_message = ListenerMessage::from_known_raw_message(raw_message, window);
        // When creating a window, the custom data for the loop is not set yet
        // before the first call to this function
        let listener_result = unsafe { window.get_user_data_ptr::<WmlOpaqueClosure>() }.and_then(
            |mut listener_ptr| {
                if let Some(known_listener_message) = &listener_message {
                    (unsafe { listener_ptr.as_mut().as_mut() })(known_listener_message)
                        .to_raw_lresult()
                } else {
                    ListenerAnswer::default().to_raw_lresult()
                }
            },
        );
        if let Some(known_listener_message) = listener_message {
            // Many messages won't go through the thread message loop at all, so we need to notify it.
            // No chance of an infinite loop here since the window procedure won't be called for messages with no associated windows.
            // Also note that the window procedure may be called multiple times while the thread message loop is blocked (waiting).
            RawMessage::post_window_proc_message(known_listener_message)
                .expect("Cannot send internal window procedure message");
        }

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
