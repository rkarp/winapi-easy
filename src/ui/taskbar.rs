//! Taskbar functionality.

use std::io;

use num_enum::IntoPrimitive;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    ITaskbarList3,
    TBPF_ERROR,
    TBPF_INDETERMINATE,
    TBPF_NOPROGRESS,
    TBPF_NORMAL,
    TBPF_PAUSED,
    TBPFLAG,
    TaskbarList,
};
use windows::core::GUID;

use crate::com::ComInterfaceExt;
use crate::internal::custom_err_with_code;
use crate::ui::window::WindowHandle;

/// Taskbar progress state animation type.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(i32)]
pub enum ProgressState {
    /// Stops displaying progress and returns the button to its normal state.
    #[default]
    NoProgress = TBPF_NOPROGRESS.0,
    /// Shows a "working" animation without indicating a completion percentage.
    Indeterminate = TBPF_INDETERMINATE.0,
    /// Shows a progress indicator displaying the amount of work being completed.
    Normal = TBPF_NORMAL.0,
    /// The progress indicator turns red to show that an error has occurred. This is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a red determinate display
    /// of a generic percentage not indicative of actual progress.
    Error = TBPF_ERROR.0,
    /// The progress indicator turns yellow to show that progress is currently stopped. This is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a yellow determinate display
    /// of a generic percentage not indicative of actual progress.
    Paused = TBPF_PAUSED.0,
}

impl From<ProgressState> for TBPFLAG {
    fn from(value: ProgressState) -> Self {
        TBPFLAG(value.into())
    }
}

/// Taskbar functionality.
pub struct Taskbar {
    taskbar_list_3: ITaskbarList3,
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
    ///
    /// ```no_run
    /// use winapi_easy::ui::taskbar::{
    ///     ProgressState,
    ///     Taskbar,
    /// };
    /// use winapi_easy::ui::window::WindowHandle;
    ///
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// let window = WindowHandle::get_console_window().expect("Cannot get console window");
    /// let taskbar = Taskbar::new()?;
    ///
    /// taskbar.set_progress_state(window, ProgressState::Indeterminate)?;
    /// thread::sleep(Duration::from_millis(3000));
    /// taskbar.set_progress_state(window, ProgressState::NoProgress)?;
    ///
    /// # Result::<(), std::io::Error>::Ok(())
    /// ```
    pub fn set_progress_state(&self, window: WindowHandle, state: ProgressState) -> io::Result<()> {
        let ret_val = unsafe {
            self.taskbar_list_3
                .SetProgressState(HWND::from(window), state.into())
        };
        ret_val.map_err(|err| custom_err_with_code("Error setting progress state", err.code()))
    }

    /// Sets the completion amount of the taskbar progress state animation.
    pub fn set_progress_value(
        &self,
        window: WindowHandle,
        completed: u64,
        total: u64,
    ) -> io::Result<()> {
        let ret_val = unsafe {
            self.taskbar_list_3
                .SetProgressValue(HWND::from(window), completed, total)
        };
        ret_val.map_err(|err| custom_err_with_code("Error setting progress value", err.code()))
    }
}

impl ComInterfaceExt for ITaskbarList3 {
    const CLASS_GUID: GUID = TaskbarList;
}
