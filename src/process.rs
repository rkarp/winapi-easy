/*!
Processes, threads.
*/

use std::convert::TryFrom;
use std::io;
use std::mem;
use std::ptr;
use std::ptr::NonNull;

use ntapi::ntpsapi::{
    NtQueryInformationProcess,
    NtSetInformationProcess,
    ProcessIoPriority,
};
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use winapi::ctypes::{
    c_int,
    c_void,
};
use winapi::shared::minwindef::{
    BOOL,
    DWORD,
    FALSE,
    HINSTANCE__,
    HMODULE,
    INT,
    LPARAM,
    TRUE,
    UINT,
    ULONG,
};
use winapi::shared::windef::HWND;
use winapi::um::libloaderapi::GetModuleHandleExW;
use winapi::um::processthreadsapi::{
    GetCurrentProcess,
    GetCurrentThread,
    GetProcessId,
    GetThreadId,
    OpenProcess,
    OpenThread,
    SetPriorityClass,
    SetThreadPriority,
};
use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot,
    Thread32First,
    Thread32Next,
    TH32CS_SNAPTHREAD,
    THREADENTRY32,
};
use winapi::um::winbase;
use winapi::um::winnt::{
    PROCESS_ALL_ACCESS,
    THREAD_ALL_ACCESS,
};
use winapi::um::winuser::EnumThreadWindows;

use crate::internal::{
    custom_err_with_code,
    sync_closure_to_callback2,
    AutoClose,
    ManagedHandle,
    RawHandle,
    ReturnValue,
};
use crate::ui::WindowHandle;

/// A Windows process
pub struct Process {
    handle: AutoClose<NonNull<c_void>>,
}

impl Process {
    /// Constructs a special handle that always points to the current process.
    ///
    /// When transferred to a different process, it will point to that process when used from it.
    pub fn current() -> Self {
        let pseudo_handle = unsafe { GetCurrentProcess() };
        Self {
            handle: pseudo_handle
                .to_non_null()
                .expect("Pseudo process handle should never be null")
                .into(),
        }
    }

    pub fn from_id<I>(id: I) -> io::Result<Self>
    where
        I: Into<ProcessId>,
    {
        let raw_handle = unsafe {
            OpenProcess(PROCESS_ALL_ACCESS, FALSE, id.into().0).to_non_null_else_get_last_error()?
        };
        Ok(Self {
            handle: raw_handle.into(),
        })
    }

    /// Sets the current process to background processing mode.
    ///
    /// This will also lower the I/O priority of the process, which will lower the impact of heavy disk I/O on other processes.
    pub fn begin_background_mode() -> io::Result<()> {
        unsafe {
            SetPriorityClass(
                Self::current().as_mutable_ptr(),
                winbase::PROCESS_MODE_BACKGROUND_BEGIN,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Ends background processing mode for the current process.
    pub fn end_background_mode() -> io::Result<()> {
        unsafe {
            SetPriorityClass(
                Self::current().as_mutable_ptr(),
                winbase::PROCESS_MODE_BACKGROUND_END,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Sets the priority of the process.
    ///
    /// # Examples
    ///
    /// ```
    /// use winapi_easy::process::{Process, ProcessPriority};
    ///
    /// Process::current().set_priority(ProcessPriority::Normal)?;
    ///
    /// # std::result::Result::<(), std::io::Error>::Ok(())
    /// ```
    pub fn set_priority(&mut self, priority: ProcessPriority) -> io::Result<()> {
        unsafe {
            SetPriorityClass(self.as_mutable_ptr(), priority.into()).if_null_get_last_error()?
        };
        Ok(())
    }

    pub fn get_io_priority(&self) -> io::Result<Option<IoPriority>> {
        let mut raw_io_priority: INT = 0;
        let mut return_length: ULONG = 0;
        let ret_val = unsafe {
            NtQueryInformationProcess(
                self.as_immutable_ptr(),
                ProcessInformationClass::ProcessIoPriority.into(),
                &mut raw_io_priority as *mut INT as *mut c_void,
                mem::size_of::<INT>() as ULONG,
                &mut return_length,
            )
        };
        ret_val
            .if_non_null_to_error(|| custom_err_with_code("Getting IO priority failed", ret_val))?;
        Ok(IoPriority::try_from(raw_io_priority as UINT).ok())
    }

    pub fn set_io_priority(&mut self, io_priority: IoPriority) -> io::Result<()> {
        let ret_val = unsafe {
            NtSetInformationProcess(
                self.as_mutable_ptr(),
                ProcessInformationClass::ProcessIoPriority.into(),
                &mut UINT::from(io_priority) as *mut UINT as *mut c_void,
                mem::size_of::<UINT>() as ULONG,
            )
        };
        ret_val
            .if_non_null_to_error(|| custom_err_with_code("Setting IO priority failed", ret_val))?;
        Ok(())
    }

    pub fn get_id(&self) -> ProcessId {
        let id = unsafe { GetProcessId(self.as_immutable_ptr()) };
        ProcessId(id)
    }
}

impl TryFrom<ProcessId> for Process {
    type Error = io::Error;

    fn try_from(value: ProcessId) -> Result<Self, Self::Error> {
        Process::from_id(value)
    }
}

impl ManagedHandle for Process {
    type Target = c_void;

    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.handle.as_immutable_ptr()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ProcessId(pub(crate) DWORD);

/// A thread inside a Windows process
pub struct Thread {
    handle: AutoClose<NonNull<c_void>>,
}

impl Thread {
    /// Constructs a special handle that always points to the current thread.
    ///
    /// When transferred to a different thread, it will point to that thread when used from it.
    pub fn current() -> Self {
        let pseudo_handle = unsafe { GetCurrentThread() };
        Thread {
            handle: pseudo_handle
                .to_non_null()
                .expect("Pseudo thread handle should never be null")
                .into(),
        }
    }

    pub fn from_id<I>(id: I) -> io::Result<Self>
    where
        I: Into<ThreadId>,
    {
        let raw_handle = unsafe {
            OpenThread(THREAD_ALL_ACCESS, FALSE, id.into().0).to_non_null_else_get_last_error()?
        };
        Ok(Self {
            handle: raw_handle.into(),
        })
    }

    /// Sets the current thread to background processing mode.
    ///
    /// This will also lower the I/O priority of the thread, which will lower the impact of heavy disk I/O on other threads and processes.
    pub fn begin_background_mode() -> io::Result<()> {
        unsafe {
            SetThreadPriority(
                Self::current().as_mutable_ptr(),
                winbase::THREAD_MODE_BACKGROUND_BEGIN as i32,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Ends background processing mode for the current thread.
    pub fn end_background_mode() -> io::Result<()> {
        unsafe {
            SetThreadPriority(
                Self::current().as_mutable_ptr(),
                winbase::THREAD_MODE_BACKGROUND_END as i32,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Sets the priority of the thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use winapi_easy::process::{Thread, ThreadPriority};
    ///
    /// Thread::current().set_priority(ThreadPriority::Normal)?;
    ///
    /// # std::result::Result::<(), std::io::Error>::Ok(())
    /// ```
    pub fn set_priority(&mut self, priority: ThreadPriority) -> Result<(), io::Error> {
        unsafe {
            SetThreadPriority(self.as_mutable_ptr(), u32::from(priority) as c_int)
                .if_null_get_last_error()?
        };
        Ok(())
    }

    pub fn get_id(&self) -> ThreadId {
        let id = unsafe { GetThreadId(self.as_immutable_ptr()) };
        ThreadId(id)
    }
}

impl TryFrom<ThreadId> for Thread {
    type Error = io::Error;

    fn try_from(value: ThreadId) -> Result<Self, Self::Error> {
        Thread::from_id(value)
    }
}

impl ManagedHandle for Thread {
    type Target = c_void;

    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.handle.as_immutable_ptr()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ThreadId(pub(crate) DWORD);

impl ThreadId {
    pub fn get_nonchild_windows(self: ThreadId) -> Vec<WindowHandle> {
        let mut result: Vec<WindowHandle> = Vec::new();
        let mut callback = |handle: HWND, _app_value: LPARAM| -> BOOL {
            let window_handle = handle
                .to_non_null()
                .expect("Window handle should not be null");
            result.push(WindowHandle::from_non_null(window_handle));
            TRUE
        };
        let _ =
            unsafe { EnumThreadWindows(self.0, Some(sync_closure_to_callback2(&mut callback)), 0) };
        result
    }
}

#[derive(Copy, Clone)]
pub struct ThreadInfo {
    raw_entry: THREADENTRY32,
}

impl ThreadInfo {
    pub fn all_threads() -> io::Result<Vec<Self>> {
        #[inline(always)]
        fn get_empty_thread_entry() -> THREADENTRY32 {
            THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as DWORD,
                ..Default::default()
            }
        }
        let mut result: Vec<Self> = Vec::new();
        let mut snapshot: AutoClose<_> = unsafe {
            CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).to_non_null_else_get_last_error()?
        }
        .into();

        let mut thread_entry = get_empty_thread_entry();
        unsafe {
            Thread32First(snapshot.as_mutable_ptr(), &mut thread_entry).if_null_get_last_error()?;
        }
        result.push(Self::from_raw(thread_entry));
        loop {
            let mut thread_entry = get_empty_thread_entry();
            let next_ret_val =
                unsafe { Thread32Next(snapshot.as_mutable_ptr(), &mut thread_entry) };
            if next_ret_val == TRUE {
                result.push(Self::from_raw(thread_entry));
            } else {
                break;
            }
        }
        Ok(result)
    }

    pub fn all_process_threads<P>(process_id: P) -> io::Result<Vec<Self>>
    where
        P: Into<ProcessId>,
    {
        let pid: ProcessId = process_id.into();
        let result = Self::all_threads()?
            .into_iter()
            .filter(|thread_info| thread_info.get_owner_process_id() == pid)
            .collect();
        Ok(result)
    }

    fn from_raw(raw_info: THREADENTRY32) -> Self {
        ThreadInfo {
            raw_entry: raw_info,
        }
    }

    pub fn get_thread_id(&self) -> ThreadId {
        ThreadId(self.raw_entry.th32ThreadID)
    }

    pub fn get_owner_process_id(&self) -> ProcessId {
        ProcessId(self.raw_entry.th32OwnerProcessID)
    }
}

#[derive(IntoPrimitive, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum ProcessPriority {
    Idle = winbase::IDLE_PRIORITY_CLASS,
    BelowNormal = winbase::BELOW_NORMAL_PRIORITY_CLASS,
    Normal = winbase::NORMAL_PRIORITY_CLASS,
    AboveNormal = winbase::ABOVE_NORMAL_PRIORITY_CLASS,
    High = winbase::HIGH_PRIORITY_CLASS,
    Realtime = winbase::REALTIME_PRIORITY_CLASS,
}

#[derive(IntoPrimitive, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum ThreadPriority {
    Idle = winbase::THREAD_PRIORITY_IDLE,
    Lowest = winbase::THREAD_PRIORITY_LOWEST,
    BelowNormal = winbase::THREAD_PRIORITY_BELOW_NORMAL,
    Normal = winbase::THREAD_PRIORITY_NORMAL,
    AboveNormal = winbase::THREAD_PRIORITY_ABOVE_NORMAL,
    Highest = winbase::THREAD_PRIORITY_HIGHEST,
    TimeCritical = winbase::THREAD_PRIORITY_TIME_CRITICAL,
}

#[derive(IntoPrimitive, TryFromPrimitive, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum IoPriority {
    VeryLow = 0,
    Low = 1,
    Normal = 2,
}

#[derive(IntoPrimitive, Clone, Copy, Debug)]
#[repr(u32)]
enum ProcessInformationClass {
    ProcessIoPriority = ProcessIoPriority,
}

pub struct ModuleHandle {
    raw_handle: NonNull<HINSTANCE__>,
}

impl ModuleHandle {
    pub fn get_current() -> io::Result<Self> {
        let raw_handle = unsafe {
            let mut h_module: HMODULE = ptr::null_mut();
            GetModuleHandleExW(0, ptr::null_mut(), &mut h_module);
            h_module.to_non_null_else_get_last_error()?
        };
        Ok(ModuleHandle { raw_handle })
    }
}

impl ManagedHandle for ModuleHandle {
    type Target = HINSTANCE__;

    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.raw_handle.as_immutable_ptr()
    }
}

#[cfg(test)]
mod tests {
    use more_asserts::*;

    use super::*;

    #[test]
    fn get_all_threads() -> io::Result<()> {
        let all_threads = ThreadInfo::all_threads()?;
        assert_gt!(all_threads.len(), 0);
        Ok(())
    }

    #[test]
    fn get_all_process_threads() -> io::Result<()> {
        let all_threads = ThreadInfo::all_process_threads(Process::current().get_id())?;
        assert_gt!(all_threads.len(), 0);
        Ok(())
    }

    #[test]
    fn get_all_threads_and_windows() -> io::Result<()> {
        let all_threads = ThreadInfo::all_threads()?;
        let all_windows: Vec<WindowHandle> = all_threads
            .into_iter()
            .flat_map(|thread_info| thread_info.get_thread_id().get_nonchild_windows())
            .collect();
        assert_gt!(all_windows.len(), 0);
        Ok(())
    }

    #[test]
    fn set_get_io_priority() -> io::Result<()> {
        let mut curr_process = Process::current();
        let target_priority = IoPriority::Low;
        curr_process.set_io_priority(target_priority)?;
        let priority = curr_process.get_io_priority()?.unwrap();
        assert_eq!(priority, target_priority);
        Ok(())
    }
}
