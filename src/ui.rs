/*!
UI components: Windows, taskbar.
*/

use std::convert::TryInto;
use std::io;
use std::io::ErrorKind;
use std::mem;
use std::ptr::NonNull;

use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use winapi::shared::minwindef::{
    BOOL,
    DWORD,
    LPARAM,
    TRUE,
    UINT,
    WPARAM,
};
use winapi::shared::ntdef::WCHAR;
use winapi::shared::windef::{
    HWND,
    HWND__,
};
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
    EnumWindows,
    GetDesktopWindow,
    GetForegroundWindow,
    GetWindowPlacement,
    GetWindowTextLengthW,
    GetWindowTextW,
    GetWindowThreadProcessId,
    IsWindow,
    IsWindowVisible,
    LockWorkStation,
    SendMessageW,
    SetWindowPlacement,
    SC_CLOSE,
    SC_MAXIMIZE,
    SC_MINIMIZE,
    SC_MONITORPOWER,
    SC_RESTORE,
    SW_HIDE,
    SW_MAXIMIZE,
    SW_MINIMIZE,
    SW_RESTORE,
    SW_SHOW,
    SW_SHOWMINIMIZED,
    SW_SHOWMINNOACTIVE,
    SW_SHOWNA,
    SW_SHOWNOACTIVATE,
    SW_SHOWNORMAL,
    WINDOWPLACEMENT,
    WM_SYSCOMMAND,
};
use wio::com::ComPtr;

use crate::com::ComInterface;
use crate::internal::{
    custom_err_with_code,
    sync_closure_to_callback2,
    ManagedHandle,
    RawHandle,
    ReturnValue,
};
use crate::process::{
    ProcessId,
    ThreadId,
};
use crate::string::FromWideString;

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

    /// Returns all top-level windows of desktop apps.
    pub fn get_toplevel_windows() -> io::Result<Vec<Self>> {
        let mut result: Vec<Window> = Vec::new();
        let mut callback = |handle: HWND, _app_value: LPARAM| -> BOOL {
            let window_handle = handle
                .to_non_null()
                .expect("Window handle should not be null");
            result.push(Window::from_non_null(window_handle));
            TRUE
        };
        let ret_val = unsafe { EnumWindows(Some(sync_closure_to_callback2(&mut callback)), 0) };
        ret_val.if_null_get_last_error()?;
        Ok(result)
    }

    /// Checks if the handle points to an existing window.
    pub fn is_window(&self) -> bool {
        let result = unsafe { IsWindow(self.as_immutable_ptr()) };
        !result.is_null()
    }

    pub fn is_visible(&self) -> bool {
        let result = unsafe { IsWindowVisible(self.as_immutable_ptr()) };
        !result.is_null()
    }

    pub fn get_caption_text(&self) -> String {
        let required_length = unsafe { GetWindowTextLengthW(self.as_immutable_ptr()) };
        let required_length = if required_length <= 0 {
            return String::new();
        } else {
            1 + required_length
        };

        let mut buffer: Vec<WCHAR> = Vec::with_capacity(required_length as usize);
        let copied_chars = unsafe {
            GetWindowTextW(
                self.as_immutable_ptr(),
                buffer.as_mut_ptr(),
                required_length,
            )
        };
        if copied_chars <= 0 {
            return String::new();
        }
        unsafe {
            buffer.set_len(copied_chars as usize);
        }
        buffer.to_string_lossy()
    }

    pub fn get_placement(&self) -> io::Result<WindowPlacement> {
        let mut raw_placement: WINDOWPLACEMENT = WINDOWPLACEMENT {
            length: mem::size_of::<WINDOWPLACEMENT>() as UINT,
            ..Default::default()
        };
        unsafe {
            GetWindowPlacement(self.as_immutable_ptr(), &mut raw_placement)
                .if_null_get_last_error()?
        };
        Ok(WindowPlacement { raw_placement })
    }

    pub fn set_placement(&mut self, placement: &WindowPlacement) -> io::Result<()> {
        unsafe {
            SetWindowPlacement(self.as_mutable_ptr(), &placement.raw_placement)
                .if_null_get_last_error()?
        };
        Ok(())
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

#[derive(IntoPrimitive, TryFromPrimitive, Copy, Clone, Eq, PartialEq)]
#[repr(i32)]
pub enum WindowShowState {
    Hide = SW_HIDE,
    Maximize = SW_MAXIMIZE,
    Minimize = SW_MINIMIZE,
    Restore = SW_RESTORE,
    Show = SW_SHOW,
    ShowMinimized = SW_SHOWMINIMIZED,
    ShowMinNoActivate = SW_SHOWMINNOACTIVE,
    ShowNoActivate = SW_SHOWNA,
    ShowNormalNoActivate = SW_SHOWNOACTIVATE,
    ShowNormal = SW_SHOWNORMAL,
}

#[derive(Copy, Clone)]
pub struct WindowPlacement {
    raw_placement: WINDOWPLACEMENT,
}

impl WindowPlacement {
    pub fn get_show_state(&self) -> Option<WindowShowState> {
        (self.raw_placement.showCmd as i32).try_into().ok()
    }

    pub fn set_show_state(&mut self, state: WindowShowState) {
        let state_i32: i32 = state.into();
        self.raw_placement.showCmd = state_i32 as u32;
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

#[cfg(test)]
mod tests {
    use more_asserts::*;

    use super::*;

    #[test]
    fn get_toplevel_windows() -> io::Result<()> {
        let all_windows = Window::get_toplevel_windows()?;
        assert_gt!(all_windows.len(), 0);
        Ok(())
    }
}
