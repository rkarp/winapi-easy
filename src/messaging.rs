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

use crate::internal::ReturnValue;

/// Windows thread message loop functions.
///
/// This type is not meant to be instantiated.
pub enum ThreadMessageLoop {}

impl ThreadMessageLoop {
    thread_local! {
        static RUNNING: Cell<bool> = const { Cell::new(false) };
        pub(crate) static ENABLE_CALLBACK_ONCE: Cell<bool> = const { Cell::new(false) };
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
        Self::RUNNING.with(|running| {
            assert!(
                !running.get(),
                "Cannot run two thread message loops on the same thread"
            );
            running.set(true);
        });
        let mut msg: MSG = Default::default();
        loop {
            unsafe {
                GetMessageW(&mut msg, None, 0, 0)
                    .if_eq_to_error(BOOL(-1), io::Error::last_os_error)?;
            }
            if msg.message == WM_QUIT {
                Self::RUNNING.with(|running| running.set(false));
                break;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if Self::ENABLE_CALLBACK_ONCE.with(Cell::take) {
                loop_callback()?;
            }
        }
        Ok(())
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
    fn is_loop_running() -> bool {
        Self::RUNNING.with(Cell::get)
    }
}
