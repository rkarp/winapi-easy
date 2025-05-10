//! Messaging and message loops.

use std::cell::Cell;
use std::io;

use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW,
    GetMessageW,
    MSG,
    PostQuitMessage,
    TranslateMessage,
    WM_QUIT,
};
use windows::core::BOOL;

use crate::internal::{
    CustomAutoDrop,
    ReturnValue,
};

/// Windows thread message loop functions.
///
/// This type is not meant to be instantiated.
pub enum ThreadMessageLoop {}

impl ThreadMessageLoop {
    thread_local! {
        static RUNNING: Cell<bool> = const { Cell::new(false) };
    }

    /// Runs the Windows thread message loop.
    ///
    /// The user defined callback that will only be called after every user handled message.
    /// This allows using local variables and `Result` propagation.
    ///
    /// Only a single message loop may be running per thread.
    ///
    /// # Panics
    ///
    /// Will panic if the message loop is already running.
    pub fn run_thread_message_loop<F>(mut loop_callback: F) -> io::Result<()>
    where
        F: FnMut() -> io::Result<()>,
    {
        Self::run_thread_message_loop_internal(|_msg| loop_callback(), true, None)
    }

    pub(crate) fn run_thread_message_loop_internal<F>(
        mut loop_msg_callback: F,
        dispatch_to_wnd_proc: bool,
        filter_message_id: Option<u32>,
    ) -> io::Result<()>
    where
        F: FnMut(&MSG) -> io::Result<()>,
    {
        let _auto_unset_running = CustomAutoDrop {
            value: (),
            drop_fn: |()| Self::RUNNING.set(false),
        };
        Self::RUNNING.with(|running| {
            assert!(
                !running.get(),
                "Cannot run two thread message loops on the same thread"
            );
            running.set(true);
        });
        loop {
            match Self::process_single_thread_message(dispatch_to_wnd_proc, filter_message_id)? {
                ThreadMessageProcessingResult::Success(msg) => {
                    loop_msg_callback(&msg)?;
                }
                ThreadMessageProcessingResult::Quit => {
                    break Ok(());
                }
            }
        }
    }

    pub(crate) fn process_single_thread_message(
        dispatch_to_wnd_proc: bool,
        filter_message_id: Option<u32>,
    ) -> io::Result<ThreadMessageProcessingResult> {
        let filter_message_id = filter_message_id.unwrap_or(0);
        let mut msg: MSG = Default::default();
        unsafe {
            GetMessageW(&mut msg, None, filter_message_id, filter_message_id)
                .if_eq_to_error(BOOL(-1), io::Error::last_os_error)?;
        }
        if msg.message == WM_QUIT {
            return Ok(ThreadMessageProcessingResult::Quit);
        }
        if dispatch_to_wnd_proc {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(ThreadMessageProcessingResult::Success(msg))
    }

    /// Posts a 'quit' message in the thread message loop.
    ///
    /// This will cause [`Self::run_thread_message_loop`] to return.
    pub fn post_quit_message() {
        unsafe {
            PostQuitMessage(0);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_loop_running() -> bool {
        Self::RUNNING.with(Cell::get)
    }
}

#[must_use]
pub(crate) enum ThreadMessageProcessingResult {
    Success(MSG),
    Quit,
}
