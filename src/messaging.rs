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

#[cfg(feature = "input")]
pub use crate::input::hotkey::HotkeyId;
use crate::internal::ReturnValue;
#[cfg(feature = "ui")]
pub use crate::ui::messaging::ListenerMessage;

pub type RawThreadMessage = MSG;

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ThreadMessage {
    #[cfg(feature = "ui")]
    WindowProc(ListenerMessage),
    #[cfg(feature = "input")]
    Hotkey(u8),
    Other(RawThreadMessage),
}

impl From<RawThreadMessage> for ThreadMessage {
    fn from(raw_message: RawThreadMessage) -> Self {
        match raw_message.message {
            #[cfg(feature = "ui")]
            crate::ui::messaging::RawMessage::ID_WINDOW_PROC_MSG => {
                let listener_message = unsafe {
                    Box::from_raw(std::ptr::with_exposed_provenance_mut::<ListenerMessage>(
                        raw_message.wParam.0,
                    ))
                };
                Self::WindowProc(*listener_message)
            }
            #[cfg(feature = "input")]
            windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY => Self::Hotkey(
                HotkeyId::try_from(raw_message.wParam.0).expect("Hotkey ID outside of valid range"),
            ),
            _ => Self::Other(raw_message),
        }
    }
}

/// Windows thread message loop context.
pub struct ThreadMessageLoop(());

impl ThreadMessageLoop {
    thread_local! {
        static RUNNING: Cell<bool> = const { Cell::new(false) };
    }

    /// Creates a new thread message context.
    ///
    /// # Panics
    ///
    /// Will panic if a thread message context already exists for the current thread.
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        assert!(
            !Self::RUNNING.get(),
            "Multiple message loop contexts per thread are not allowed"
        );
        Self::RUNNING.set(true);
        Self(())
    }

    pub fn run(&mut self) -> io::Result<()> {
        self.run_with(|_| Ok(()))
    }

    /// Runs the Windows thread message loop.
    ///
    /// The user defined callback will be called on every message except `WM_QUIT`.
    pub fn run_with<F>(&mut self, loop_callback: F) -> io::Result<()>
    where
        F: FnMut(ThreadMessage) -> io::Result<()>,
    {
        self.run_thread_message_loop_internal(loop_callback, true, None)
    }

    pub(crate) fn run_thread_message_loop_internal<F>(
        &mut self,
        mut loop_msg_callback: F,
        dispatch_to_wnd_proc: bool,
        filter_message_id: Option<u32>,
    ) -> io::Result<()>
    where
        F: FnMut(ThreadMessage) -> io::Result<()>,
    {
        loop {
            match Self::process_single_thread_message(
                self,
                dispatch_to_wnd_proc,
                filter_message_id,
            )? {
                ThreadMessageProcessingResult::Success(msg) => {
                    loop_msg_callback(ThreadMessage::from(msg))?;
                }
                ThreadMessageProcessingResult::Quit => {
                    break Ok(());
                }
            }
        }
    }

    #[expect(clippy::unused_self)]
    pub(crate) fn process_single_thread_message(
        &mut self,
        dispatch_to_wnd_proc: bool,
        filter_message_id: Option<u32>,
    ) -> io::Result<ThreadMessageProcessingResult> {
        // Warning: Message filtering will also filter out `WM_QUIT` messages if posted via `PostThreadMessageW`.
        let filter_message_id = filter_message_id.unwrap_or(0);
        let mut msg: MSG = Default::default();
        unsafe {
            GetMessageW(&raw mut msg, None, filter_message_id, filter_message_id)
                .if_eq_to_error(BOOL(-1), io::Error::last_os_error)?;
        }
        if msg.message == WM_QUIT {
            return Ok(ThreadMessageProcessingResult::Quit);
        }
        if dispatch_to_wnd_proc {
            unsafe {
                let _ = TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
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

    #[cfg(feature = "process")]
    pub fn post_thread_quit_message(thread_id: crate::process::ThreadId) -> io::Result<()> {
        thread_id.post_quit_message()
    }
}

impl Drop for ThreadMessageLoop {
    fn drop(&mut self) {
        Self::RUNNING.set(false);
    }
}

#[must_use]
pub(crate) enum ThreadMessageProcessingResult {
    Success(MSG),
    Quit,
}
