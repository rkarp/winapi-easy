use winapi::shared::minwindef::{
    LPARAM,
    LRESULT,
    UINT,
    WPARAM,
};
use winapi::shared::windef::HWND;
use winapi::um::winuser::{
    DefWindowProcW,
    DispatchMessageW,
    GetMessageW,
    PostQuitMessage,
    TranslateMessage,
    MSG,
    WM_APP,
    WM_DESTROY,
    WM_MENUCOMMAND,
    WM_QUIT,
    WM_USER,
};

use std::ptr::NonNull;
use std::{
    io,
    ptr,
};

#[cfg(any())]
use std::convert::TryInto;

use crate::internal::{
    catch_unwind_or_abort,
    ReturnValue,
};
use crate::ui::WindowHandle;

pub trait WindowMessageListener {
    #[allow(unused_variables)]
    #[inline(always)]
    fn handle_menu_command(
        &mut self,
        window: WindowHandle,
        selected_item_idx: WPARAM,
        menu_handle: LPARAM,
    ) {
    }
    #[allow(unused_variables)]
    #[inline(always)]
    fn handle_window_destroy(&mut self, window: WindowHandle) {}
    #[allow(unused_variables)]
    #[inline(always)]
    fn handle_user_private_message(&mut self, window: WindowHandle, message_id: u32) {}
}

#[derive(Copy, Clone)]
pub(crate) struct RawMessage {
    pub(crate) message: UINT,
    pub(crate) w_param: WPARAM,
    pub(crate) l_param: LPARAM,
}

impl RawMessage {
    #[cfg(any())]
    #[inline]
    pub(crate) fn try_into_message(self) -> Result<Message, ()> {
        self.try_into()
    }

    pub(crate) fn dispatch_to_message_listener<WML: WindowMessageListener>(
        self,
        window: WindowHandle,
        listener: &mut WML,
    ) -> Option<LRESULT> {
        let RawMessage {
            message,
            w_param,
            l_param,
        } = self;
        match message {
            value if value >= WM_USER && value < WM_APP => {
                listener.handle_user_private_message(window, message - WM_USER);
                None
            }
            WM_MENUCOMMAND => {
                listener.handle_menu_command(window, w_param, l_param);
                None
            }
            WM_DESTROY => {
                listener.handle_window_destroy(window);
                None
            }
            _ => None,
        }
    }
}

#[cfg(any())]
impl TryInto<Message> for RawMessage {
    type Error = ();

    fn try_into(self) -> Result<Message, Self::Error> {
        let RawMessage {
            message,
            w_param,
            l_param,
        } = self;
        match message {
            value if value >= WM_USER && value < WM_APP => Ok(Message::User(message - WM_USER)),
            WM_MENUCOMMAND => Ok(Message::MenuCommand {
                selected_item_idx: w_param,
                menu_handle: l_param,
            }),
            WM_DESTROY => Ok(Message::WindowDestroy),
            _ => Err(()),
        }
    }
}

#[cfg(any())]
pub(crate) enum Message {
    MenuCommand {
        selected_item_idx: WPARAM,
        menu_handle: LPARAM,
    },
    User(u32),
    WindowDestroy,
}

pub fn run_thread_message_loop() -> io::Result<()> {
    let mut msg: MSG = Default::default();
    loop {
        unsafe {
            GetMessageW(&mut msg, ptr::null_mut(), 0, 0)
                .if_eq_to_error(-1, || io::Error::last_os_error())?;
        }
        if msg.message == WM_QUIT {
            break;
        }
        unsafe {
            TranslateMessage(&mut msg);
            DispatchMessageW(&mut msg);
        }
    }
    Ok(())
}

pub fn post_quit_message() {
    unsafe {
        PostQuitMessage(0);
    }
}

pub(crate) unsafe extern "system" fn generic_window_proc<WML>(
    h_wnd: HWND,
    message: UINT,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT
where
    WML: WindowMessageListener,
{
    let call = move || {
        let window = WindowHandle::from_non_null(
            NonNull::new(h_wnd)
                .expect("Window handle given to window procedure should never be NULL"),
        );

        let raw_message = RawMessage {
            message,
            w_param,
            l_param,
        };

        // When creating a window, the custom data for the loop is not set yet
        // before the first call to this function
        let listener_result = window
            .get_user_data_ptr::<WML>()
            .and_then(|mut listener_ptr| {
                raw_message.dispatch_to_message_listener(window, listener_ptr.as_mut())
            });

        if let Some(l_result) = listener_result {
            l_result
        } else {
            DefWindowProcW(h_wnd, message, w_param, l_param)
        }
    };
    catch_unwind_or_abort(call)
}

#[cfg(any())]
pub(crate) fn sync_closure_to_window_proc_unsafe<F>(
    closure: &mut F,
) -> unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT
where
    F: FnMut(WindowHandle, RawMessage) -> Option<LRESULT>,
{
    thread_local! {
        static RAW_CLOSURE: Cell<*mut ffi::c_void> = Cell::new(ptr::null_mut());
    }

    unsafe extern "system" fn trampoline<F>(
        h_wnd: HWND,
        message: UINT,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT
    where
        F: FnMut(WindowHandle, RawMessage) -> Option<LRESULT>,
    {
        let call = move || {
            let unwrapped_closure: *mut ffi::c_void =
                RAW_CLOSURE.with(|raw_closure| raw_closure.get());
            let closure: &mut F = &mut *(unwrapped_closure as *mut F);

            let window = WindowHandle::from_non_null(
                NonNull::new(h_wnd)
                    .expect("Window handle given to window procedure should never be NULL"),
            );
            let raw_message = RawMessage {
                message,
                w_param,
                l_param,
            };

            if let Some(l_result) = closure(window, raw_message) {
                l_result
            } else {
                DefWindowProcW(h_wnd, message, w_param, l_param)
            }
        };
        catch_unwind_or_abort(call)
    }
    RAW_CLOSURE.with(|cell| cell.set(closure as *mut F as *mut ffi::c_void));
    trampoline::<F>
}
