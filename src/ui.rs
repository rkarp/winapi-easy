/*!
UI components: Windows, taskbar.
*/

use std::io;
use std::io::ErrorKind;
use std::ptr::NonNull;

use winapi::shared::minwindef::{
    DWORD,
    LPARAM,
    WPARAM,
};
use winapi::shared::windef::HWND__;
use winapi::shared::winerror::S_OK;
use winapi::um::shobjidl_core::{
    ITaskbarList3,
    TBPFLAG,
    TBPF_ERROR,
    TBPF_INDETERMINATE,
    TBPF_NOPROGRESS,
    TBPF_NORMAL,
    TBPF_PAUSED,
};
use winapi::um::wincon::GetConsoleWindow;
use winapi::um::winuser::{
    GetDesktopWindow,
    GetForegroundWindow,
    GetWindowThreadProcessId,
    IsWindow,
    IsWindowVisible,
    LockWorkStation,
    SendMessageW,
    SC_CLOSE,
    SC_MAXIMIZE,
    SC_MINIMIZE,
    SC_MONITORPOWER,
    SC_RESTORE,
    WM_SYSCOMMAND,
};
use wio::com::ComPtr;

use crate::com::ComInterface;
use crate::internal::{
    custom_err_with_code,
    ManagedHandle,
    RawHandle,
    ReturnValue,
};
use crate::process::{
    ProcessId,
    ThreadId,
};

/// A (non-null) handle to a window.
///
/// **Note**: If the window was not created by this thread, then it is not guaranteed that
/// the handle continues pointing to the same window because the underlying handles
/// can get invalid or even recycled.
///
/// Implements neither `Copy` nor `Clone` to avoid concurrent mutable access to the same handle.
pub struct Window {
    handle: NonNull<HWND__>,
}

impl Window {
    /// Returns the console window associated with the current process, if there is one.
    pub fn get_console_window() -> Option<Self> {
        let handle = unsafe { GetConsoleWindow() };
        handle.to_non_null().map(Self::from_non_null)
    }

    pub fn get_foreground_window() -> Option<Self> {
        let handle = unsafe { GetForegroundWindow() };
        handle.to_non_null().map(Self::from_non_null)
    }

    pub fn get_desktop_window() -> io::Result<Self> {
        let handle = unsafe { GetDesktopWindow() };
        let handle = handle.to_non_null_else_error(|| ErrorKind::Other.into())?;
        Ok(Self::from_non_null(handle))
    }

    /// Checks if the handle points to an existing window.
    pub fn is_window(&self) -> bool {
        let result = unsafe { IsWindow(self.as_immutable_ptr()) };
        !result.is_null()
    }

    pub fn is_window_visible(&self) -> bool {
        let result = unsafe { IsWindowVisible(self.as_immutable_ptr()) };
        !result.is_null()
    }

    pub fn action(&mut self, action: WindowAction) -> io::Result<()> {
        let result =
            unsafe { SendMessageW(self.as_mutable_ptr(), WM_SYSCOMMAND, action as WPARAM, 0) };
        result.if_non_null_to_error(|| custom_err_with_code("Cannot perform window action", result))
    }

    #[inline(always)]
    pub fn get_creator_thread_id(&self) -> ThreadId {
        self.get_creator_thread_process_ids().0
    }

    #[inline(always)]
    pub fn get_creator_process_id(&self) -> ProcessId {
        self.get_creator_thread_process_ids().1
    }

    fn get_creator_thread_process_ids(&self) -> (ThreadId, ProcessId) {
        let mut process_id: DWORD = 0;
        let thread_id =
            unsafe { GetWindowThreadProcessId(self.as_immutable_ptr(), &mut process_id) };
        (ThreadId(thread_id), ProcessId(process_id))
    }

    pub fn set_monitor_power(&mut self, level: MonitorPower) -> io::Result<()> {
        let result = unsafe {
            SendMessageW(
                self.as_mutable_ptr(),
                WM_SYSCOMMAND,
                SC_MONITORPOWER,
                level as LPARAM,
            )
        };
        result.if_non_null_to_error(|| {
            custom_err_with_code("Cannot set monitor power using window", result)
        })
    }

    pub(crate) fn from_non_null(handle: NonNull<HWND__>) -> Self {
        Self { handle }
    }
}

impl ManagedHandle for Window {
    type Target = HWND__;

    #[inline(always)]
    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.handle.as_immutable_ptr()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(usize)]
pub enum WindowAction {
    Close = SC_CLOSE,
    Maximize = SC_MAXIMIZE,
    Minimize = SC_MINIMIZE,
    Restore = SC_RESTORE,
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(isize)]
pub enum MonitorPower {
    On = -1,
    Low = 1,
    Off = 2,
}

/// Taskbar progress state animation type.
#[derive(Copy, Clone, Eq, PartialEq)]
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
                .SetProgressState(window.as_mutable_ptr(), state as TBPFLAG)
            {
                S_OK => Ok(()),
                err_code => Err(custom_err_with_code(
                    "Error setting progress state",
                    err_code,
                )),
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
                .SetProgressValue(window.as_mutable_ptr(), completed, total)
            {
                S_OK => Ok(()),
                err_code => Err(custom_err_with_code(
                    "Error setting progress value",
                    err_code,
                )),
            }
        }
    }
}

pub fn lock_workstation() -> io::Result<()> {
    // Because the function executes asynchronously, a nonzero return value indicates that the operation has been initiated.
    // It does not indicate whether the workstation has been successfully locked.
    let _ = unsafe { LockWorkStation().if_null_get_last_error()? };
    Ok(())
}
