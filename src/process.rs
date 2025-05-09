//! Processes, threads.

use std::ffi::c_void;
use std::{
    io,
    mem,
};

use ntapi::ntpsapi::NtSetInformationProcess;
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use windows::Wdk::System::Threading::{
    NtQueryInformationProcess,
    ProcessIoPriority,
};
use windows::Win32::Foundation::{
    HANDLE,
    HMODULE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot,
    TH32CS_SNAPTHREAD,
    THREADENTRY32,
    Thread32First,
    Thread32Next,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleExW;
use windows::Win32::System::Threading;
use windows::Win32::System::Threading::{
    GetCurrentProcess,
    GetCurrentProcessId,
    GetCurrentThread,
    GetCurrentThreadId,
    GetProcessId,
    GetThreadId,
    OpenProcess,
    OpenThread,
    PROCESS_ALL_ACCESS,
    PROCESS_CREATION_FLAGS,
    PROCESS_MODE_BACKGROUND_BEGIN,
    PROCESS_MODE_BACKGROUND_END,
    SetPriorityClass,
    SetThreadPriority,
    THREAD_ALL_ACCESS,
    THREAD_MODE_BACKGROUND_BEGIN,
    THREAD_MODE_BACKGROUND_END,
    THREAD_PRIORITY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    PostThreadMessageW,
    WM_QUIT,
};

#[rustversion::before(1.87)]
use crate::internal::std_unstable::CastUnsigned;
use crate::internal::{
    AutoClose,
    ReturnValue,
    custom_err_with_code,
};

/// A Windows process.
pub struct Process {
    handle: AutoClose<HANDLE>,
}

impl Process {
    /// Constructs a special handle that always points to the current process.
    ///
    /// When transferred to a different process, it will point to that process when used from it.
    pub fn current() -> Self {
        let pseudo_handle = unsafe { GetCurrentProcess() };
        Self::from_maybe_null(pseudo_handle)
            .unwrap_or_else(|| unreachable!("Pseudo process handle should never be null"))
    }

    /// Tries to acquire a process handle from an ID.
    ///
    /// This may fail due to insufficient access rights.
    pub fn from_id<I>(id: I) -> io::Result<Self>
    where
        I: Into<ProcessId>,
    {
        let raw_handle = unsafe { OpenProcess(PROCESS_ALL_ACCESS, false, id.into().0)? };
        Ok(Self {
            handle: raw_handle.into(),
        })
    }

    /// Sets the current process to background processing mode.
    ///
    /// This will also lower the I/O priority of the process, which will lower the impact of heavy disk I/O on other processes.
    pub fn begin_background_mode() -> io::Result<()> {
        unsafe { SetPriorityClass(Self::current().handle.entity, PROCESS_MODE_BACKGROUND_BEGIN)? };
        Ok(())
    }

    /// Ends background processing mode for the current process.
    pub fn end_background_mode() -> io::Result<()> {
        unsafe { SetPriorityClass(Self::current().handle.entity, PROCESS_MODE_BACKGROUND_END)? };
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
    /// # Result::<(), std::io::Error>::Ok(())
    /// ```
    pub fn set_priority(&mut self, priority: ProcessPriority) -> io::Result<()> {
        unsafe { SetPriorityClass(self.handle.entity, priority.into())? };
        Ok(())
    }

    /// Returns the I/O priority of the process.
    ///
    /// Will return `None` if it is an unknown value.
    pub fn get_io_priority(&self) -> io::Result<Option<IoPriority>> {
        let mut raw_io_priority: i32 = 0;
        let mut return_length: u32 = 0;
        let ret_val = unsafe {
            NtQueryInformationProcess(
                self.handle.entity,
                ProcessIoPriority,
                (&raw mut raw_io_priority).cast::<c_void>(),
                u32::try_from(mem::size_of::<i32>()).unwrap_or_else(|_| unreachable!()),
                &mut return_length,
            )
        };
        ret_val.0.if_non_null_to_error(|| {
            custom_err_with_code("Getting IO priority failed", ret_val.0)
        })?;
        Ok(IoPriority::try_from(raw_io_priority.cast_unsigned()).ok())
    }

    pub fn set_io_priority(&mut self, io_priority: IoPriority) -> io::Result<()> {
        let mut raw_io_priority = u32::from(io_priority);
        let ret_val = unsafe {
            NtSetInformationProcess(
                self.handle.entity.0.cast::<ntapi::winapi::ctypes::c_void>(),
                ProcessIoPriority.0.cast_unsigned(),
                (&raw mut raw_io_priority).cast::<ntapi::winapi::ctypes::c_void>(),
                u32::try_from(mem::size_of::<u32>()).unwrap_or_else(|_| unreachable!()),
            )
        };
        ret_val
            .if_non_null_to_error(|| custom_err_with_code("Setting IO priority failed", ret_val))?;
        Ok(())
    }

    pub fn get_id(&self) -> ProcessId {
        let id = unsafe { GetProcessId(self.handle.entity) };
        ProcessId(id)
    }

    #[allow(dead_code)]
    fn from_non_null(handle: HANDLE) -> Self {
        Self {
            handle: handle.into(),
        }
    }

    fn from_maybe_null(handle: HANDLE) -> Option<Self> {
        if handle.is_null() {
            None
        } else {
            Some(Self {
                handle: handle.into(),
            })
        }
    }
}

impl TryFrom<ProcessId> for Process {
    type Error = io::Error;

    fn try_from(value: ProcessId) -> Result<Self, Self::Error> {
        Process::from_id(value)
    }
}

/// ID of a [`Process`].
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ProcessId(pub(crate) u32);

impl ProcessId {
    /// Returns the current process ID.
    pub fn current() -> Self {
        Self(unsafe { GetCurrentProcessId() })
    }
}

/// A thread inside a Windows process.
pub struct Thread {
    handle: AutoClose<HANDLE>,
}

impl Thread {
    /// Constructs a special handle that always points to the current thread.
    ///
    /// When transferred to a different thread, it will point to that thread when used from it.
    pub fn current() -> Self {
        let pseudo_handle = unsafe { GetCurrentThread() };
        Self::from_maybe_null(pseudo_handle)
            .unwrap_or_else(|| unreachable!("Pseudo thread handle should never be null"))
    }

    /// Tries to acquire a thread handle from an ID.
    ///
    /// This may fail due to insufficient access rights.
    pub fn from_id<I>(id: I) -> io::Result<Self>
    where
        I: Into<ThreadId>,
    {
        let raw_handle = unsafe { OpenThread(THREAD_ALL_ACCESS, false, id.into().0)? };
        Ok(Self {
            handle: raw_handle.into(),
        })
    }

    /// Sets the current thread to background processing mode.
    ///
    /// This will also lower the I/O priority of the thread, which will lower the impact of heavy disk I/O on other threads and processes.
    pub fn begin_background_mode() -> io::Result<()> {
        unsafe { SetThreadPriority(Self::current().handle.entity, THREAD_MODE_BACKGROUND_BEGIN)? };
        Ok(())
    }

    /// Ends background processing mode for the current thread.
    pub fn end_background_mode() -> io::Result<()> {
        unsafe { SetThreadPriority(Self::current().handle.entity, THREAD_MODE_BACKGROUND_END)? };
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
    /// # Result::<(), std::io::Error>::Ok(())
    /// ```
    pub fn set_priority(&mut self, priority: ThreadPriority) -> Result<(), io::Error> {
        unsafe { SetThreadPriority(self.handle.entity, priority.into())? };
        Ok(())
    }

    pub fn get_id(&self) -> ThreadId {
        let id = unsafe { GetThreadId(self.handle.entity) };
        ThreadId(id)
    }

    #[allow(dead_code)]
    fn from_non_null(handle: HANDLE) -> Self {
        Self {
            handle: handle.into(),
        }
    }

    fn from_maybe_null(handle: HANDLE) -> Option<Self> {
        if handle.is_null() {
            None
        } else {
            Some(Self {
                handle: handle.into(),
            })
        }
    }
}

impl TryFrom<ThreadId> for Thread {
    type Error = io::Error;

    fn try_from(value: ThreadId) -> Result<Self, Self::Error> {
        Thread::from_id(value)
    }
}

/// ID of a [`Thread`].
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ThreadId(pub(crate) u32);

impl ThreadId {
    /// Returns the current thread ID.
    pub fn current() -> Self {
        Self(unsafe { GetCurrentThreadId() })
    }

    pub fn post_quit_message(self) -> io::Result<()> {
        unsafe { PostThreadMessageW(self.0, WM_QUIT, Default::default(), Default::default())? }
        Ok(())
    }
}

/// Infos about a [`Thread`].
#[derive(Copy, Clone, Debug)]
pub struct ThreadInfo {
    raw_entry: THREADENTRY32,
}

impl ThreadInfo {
    /// Returns all threads of all processes.
    pub fn all_threads() -> io::Result<Vec<Self>> {
        fn get_empty_thread_entry() -> THREADENTRY32 {
            THREADENTRY32 {
                dwSize: mem::size_of::<THREADENTRY32>().try_into().unwrap(),
                ..Default::default()
            }
        }
        let mut result: Vec<Self> = Vec::new();
        let snapshot: AutoClose<HANDLE> =
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)? }.into();

        let mut thread_entry = get_empty_thread_entry();
        unsafe {
            Thread32First(snapshot.entity, &mut thread_entry)?;
        }
        result.push(Self::from_raw(thread_entry));
        loop {
            let mut thread_entry = get_empty_thread_entry();
            let next_ret_val = unsafe { Thread32Next(snapshot.entity, &mut thread_entry) };
            if next_ret_val.is_ok() {
                result.push(Self::from_raw(thread_entry));
            } else {
                break;
            }
        }
        Ok(result)
    }

    /// Returns all threads of the given process.
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

    /// Returns the ID of the thread.
    pub fn get_thread_id(&self) -> ThreadId {
        ThreadId(self.raw_entry.th32ThreadID)
    }

    /// Returns the ID of the process that contains the thread.
    pub fn get_owner_process_id(&self) -> ProcessId {
        ProcessId(self.raw_entry.th32OwnerProcessID)
    }
}

/// Process CPU priority.
#[derive(IntoPrimitive, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum ProcessPriority {
    Idle = Threading::IDLE_PRIORITY_CLASS.0,
    BelowNormal = Threading::BELOW_NORMAL_PRIORITY_CLASS.0,
    Normal = Threading::NORMAL_PRIORITY_CLASS.0,
    AboveNormal = Threading::ABOVE_NORMAL_PRIORITY_CLASS.0,
    High = Threading::HIGH_PRIORITY_CLASS.0,
    Realtime = Threading::REALTIME_PRIORITY_CLASS.0,
}

impl From<ProcessPriority> for PROCESS_CREATION_FLAGS {
    fn from(value: ProcessPriority) -> Self {
        PROCESS_CREATION_FLAGS(value.into())
    }
}

/// Thread CPU priority.
#[derive(IntoPrimitive, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(i32)]
pub enum ThreadPriority {
    Idle = Threading::THREAD_PRIORITY_IDLE.0,
    Lowest = Threading::THREAD_PRIORITY_LOWEST.0,
    BelowNormal = Threading::THREAD_PRIORITY_BELOW_NORMAL.0,
    Normal = Threading::THREAD_PRIORITY_NORMAL.0,
    AboveNormal = Threading::THREAD_PRIORITY_ABOVE_NORMAL.0,
    Highest = Threading::THREAD_PRIORITY_HIGHEST.0,
    TimeCritical = Threading::THREAD_PRIORITY_TIME_CRITICAL.0,
}

impl From<ThreadPriority> for THREAD_PRIORITY {
    fn from(value: ThreadPriority) -> Self {
        THREAD_PRIORITY(value.into())
    }
}

/// Process or thread IO priority. This is independent of the standard CPU priorities.
#[derive(IntoPrimitive, TryFromPrimitive, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum IoPriority {
    VeryLow = 0,
    Low = 1,
    Normal = 2,
}

/// A handle to a module (EXE or DLL).
pub struct ModuleHandle {
    #[allow(dead_code)]
    raw_handle: HMODULE,
}

impl ModuleHandle {
    /// Returns the module handle of the currently executed code.
    pub fn get_current() -> io::Result<Self> {
        let raw_handle = unsafe {
            let mut h_module: HMODULE = Default::default();
            GetModuleHandleExW(0, None, &mut h_module)?;
            h_module.if_null_get_last_error()?
        };
        Ok(ModuleHandle { raw_handle })
    }
}

#[cfg(test)]
mod tests {
    use more_asserts::*;

    use super::*;
    #[cfg(feature = "ui")]
    use crate::ui::window::WindowHandle;

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

    #[cfg(feature = "ui")]
    #[test]
    fn get_all_threads_and_windows() -> io::Result<()> {
        let all_threads = ThreadInfo::all_threads()?;
        let all_windows: Vec<WindowHandle> = all_threads
            .into_iter()
            .flat_map(|thread_info| WindowHandle::get_nonchild_windows(thread_info.get_thread_id()))
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
