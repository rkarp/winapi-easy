//! Messaging and message loops.

use std::cell::{
    Cell,
    RefCell,
};
use std::collections::HashMap;
use std::io;
use std::rc::Rc;

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

pub(crate) type ListenerFn<'a> = Box<dyn FnMut(MSG) -> io::Result<()> + 'a>;

/// Windows thread message loop context.
pub struct ThreadMessageLoop<'a> {
    pub(crate) listeners: Rc<RefCell<HashMap<u32, ListenerFn<'a>>>>,
}

impl ThreadMessageLoop<'_> {
    thread_local! {
        static RUNNING: Cell<bool> = const { Cell::new(false) };
    }

    /// Creates a new thread message context.
    ///
    /// # Panics
    ///
    /// Will panic if a thread message context already exists for the current thread.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        assert!(
            !Self::RUNNING.get(),
            "Multiple message loop contexts per thread are not allowed"
        );
        Self::RUNNING.set(true);
        Self {
            listeners: Rc::new(HashMap::new().into()),
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        self.run_with(|| Ok(()))
    }

    pub fn run_with<F>(&mut self, mut loop_callback: F) -> io::Result<()>
    where
        F: FnMut() -> io::Result<()>,
    {
        self.run_thread_message_loop_internal(|_| loop_callback(), true, None)
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
        Self::new().run_thread_message_loop_internal(|_msg| loop_callback(), true, None)
    }

    pub(crate) fn run_thread_message_loop_internal<F>(
        &mut self,
        mut loop_msg_callback: F,
        dispatch_to_wnd_proc: bool,
        filter_message_id: Option<u32>,
    ) -> io::Result<()>
    where
        F: FnMut(&MSG) -> io::Result<()>,
    {
        loop {
            match Self::process_single_thread_message(
                self,
                dispatch_to_wnd_proc,
                filter_message_id,
            )? {
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
        &mut self,
        dispatch_to_wnd_proc: bool,
        filter_message_id: Option<u32>,
    ) -> io::Result<ThreadMessageProcessingResult> {
        // Warning: Message filtering will also filter out `WM_QUIT` messages if posted via `PostThreadMessageW`.
        let filter_message_id = filter_message_id.unwrap_or(0);
        let mut msg: MSG = Default::default();
        unsafe {
            GetMessageW(&mut msg, None, filter_message_id, filter_message_id)
                .if_eq_to_error(BOOL(-1), io::Error::last_os_error)?;
        }
        if msg.message == WM_QUIT {
            return Ok(ThreadMessageProcessingResult::Quit);
        }
        if let Some(listener) = self.listeners.borrow_mut().get_mut(&msg.message) {
            listener(msg)?;
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

    #[cfg(feature = "process")]
    pub fn post_thread_quit_message(thread_id: crate::process::ThreadId) -> io::Result<()> {
        thread_id.post_quit_message()
    }
}

impl Drop for ThreadMessageLoop<'_> {
    fn drop(&mut self) {
        Self::RUNNING.set(false);
    }
}

#[must_use]
pub(crate) enum ThreadMessageProcessingResult {
    Success(MSG),
    Quit,
}
