/*!
UI components: Windows, taskbar.
*/

use std::{
    io,
    ptr::NonNull,
};

use winapi::{
    shared::{
        windef::HWND__,
        winerror::S_OK,
    },
    um::{
        shobjidl_core::{
            ITaskbarList3,
            TBPF_ERROR,
            TBPF_INDETERMINATE,
            TBPF_NOPROGRESS,
            TBPF_NORMAL,
            TBPF_PAUSED,
            TBPFLAG,
        },
        wincon::GetConsoleWindow,
    },
};
use wio::com::ComPtr;

use crate::{
    com::ComInterface,
    custom_hresult_err,
};

/// A (non-null) handle to a window.
///
/// Implements neither `Copy` nor `Clone` to avoid concurrent mutable access to the same handle.
pub struct Window(NonNull<HWND__>);

impl Window {
    /// Returns the console window associated with the current process, if there is one.
    pub fn get_console_window() -> Option<Self> {
        let handle = unsafe { GetConsoleWindow() };
        NonNull::new(handle).map(Into::into)
    }
}

impl From<NonNull<HWND__>> for Window {
    fn from(handle: NonNull<HWND__>) -> Self {
        Self(handle)
    }
}

impl From<Window> for NonNull<HWND__> {
    fn from(window: Window) -> Self {
        window.0
    }
}

/// Taskbar progress state animation type.
#[derive(Copy, Clone)]
#[repr(u32)]
pub enum ProgressState {
    /// Stops displaying progress and returns the button to its normal state.
    NoProgress = TBPF_NOPROGRESS,
    /// Shows a "working" animation without indicating a completion percentage.
    ///
    /// Needs animations on the taskbar to be enabled in the OS config,
    /// otherwise it will not show anything to the user.
    Indeterminate = TBPF_INDETERMINATE,
    /// Shows a progress indicator displaying the amount of work being completed.
    Normal = TBPF_NORMAL,
    /// The progress indicator turns red to show that an error has occurred. This is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a red determinate display
    /// of a generic percentage not indicative of actual progress.
    Error = TBPF_ERROR,
    /// The progress indicator turns yellow to show that progress is currently stopped. his is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a yellow determinate display
    /// of a generic percentage not indicative of actual progress.
    Paused = TBPF_PAUSED,
}

impl Default for ProgressState {
    fn default() -> Self {
        ProgressState::NoProgress
    }
}

/// Taskbar functionality.
pub struct Taskbar {
    taskbar_list_3: ComPtr<ITaskbarList3>,
}

impl Taskbar {
    pub fn new() -> io::Result<Self> {
        let result = Taskbar {
            taskbar_list_3: ITaskbarList3::new_instance()?,
        };
        Ok(result)
    }

    /// Sets the progress state taskbar animation of a window.
    ///
    /// See also: [Microsoft docs](https://docs.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-itaskbarlist3-setprogressstate)
    ///
    /// # Examples
    /// **Warning:** On Windows 7 (and possibly newer versions as well), when changing the progress state too quickly,
    /// the change may be ignored. As a workaround, you can sleep for a short time:
    ///
    /// ```no_run
    /// use winapi_easy::ui::{
    ///     ProgressState,
    ///     Taskbar,
    ///     Window,
    /// };
    ///
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// let mut window = Window::get_console_window().expect("Cannot get console window");
    /// let mut taskbar = Taskbar::new()?;
    ///
    /// taskbar.set_progress_state(&mut window, ProgressState::Indeterminate)?;
    /// thread::sleep(Duration::from_millis(20));
    /// taskbar.set_progress_state(&mut window, ProgressState::NoProgress)?;
    ///
    /// # std::result::Result::<(), std::io::Error>::Ok(())
    /// ```
    pub fn set_progress_state(
        &mut self,
        window: &mut Window,
        state: ProgressState,
    ) -> io::Result<()> {
        unsafe {
            match self
                .taskbar_list_3
                .SetProgressState(window.0.as_ptr(), state as TBPFLAG)
            {
                S_OK => Ok(()),
                err_code => custom_hresult_err("Error setting progress state", err_code),
            }
        }
    }

    /// Sets the completion amount of the taskbar progress state animation.
    pub fn set_progress_value(
        &mut self,
        window: &mut Window,
        completed: u64,
        total: u64,
    ) -> io::Result<()> {
        unsafe {
            match self
                .taskbar_list_3
                .SetProgressValue(window.0.as_ptr(), completed, total)
            {
                S_OK => Ok(()),
                err_code => custom_hresult_err("Error setting progress value", err_code),
            }
        }
    }
}
