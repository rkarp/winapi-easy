//! Processes, threads.

use std::ffi::c_void;
use std::{
    io,
    mem,
    ptr,
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
    WAIT_ABANDONED,
    WAIT_FAILED,
    WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot,
    TH32CS_SNAPTHREAD,
    THREADENTRY32,
    Thread32First,
    Thread32Next,
};
use windows::Win32::System::Memory::{
    MEM_COMMIT,
    MEM_DECOMMIT,
    MEM_RELEASE,
    MEM_RESERVE,
    PAGE_READWRITE,
    VirtualAllocEx,
    VirtualFreeEx,
};
use windows::Win32::System::Threading::{
    self,
    CreateRemoteThreadEx,
    GetCurrentProcess,
    GetCurrentProcessId,
    GetCurrentThread,
    GetCurrentThreadId,
    GetProcessId,
    GetThreadId,
    INFINITE,
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
    WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    PostThreadMessageW,
    WM_QUIT,
};

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

    pub fn get_id(&self) -> ProcessId {
        let id = unsafe { GetProcessId(self.handle.entity) };
        ProcessId(id)
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
    pub fn set_priority(&self, priority: ProcessPriority) -> io::Result<()> {
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
                &raw mut return_length,
            )
        };
        ret_val.0.if_non_null_to_error(|| {
            custom_err_with_code("Getting IO priority failed", ret_val.0)
        })?;
        Ok(IoPriority::try_from(raw_io_priority.cast_unsigned()).ok())
    }

    pub fn set_io_priority(&self, io_priority: IoPriority) -> io::Result<()> {
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

    /// Creates a thread in another process.
    ///
    /// # Safety
    ///
    /// Requires `start_address` to be a function pointer valid in the remote process
    /// and ABI-compatible with the signature: `unsafe extern "system" fn(*mut std::ffi::c_void) -> u32`
    pub unsafe fn create_remote_thread(
        &self,
        start_address: *const c_void,
        call_param0: Option<*const c_void>,
    ) -> io::Result<Thread> {
        let thread_handle = unsafe {
            let start_fn =
                mem::transmute::<*const c_void, unsafe extern "system" fn(_) -> _>(start_address);
            CreateRemoteThreadEx(
                self.as_raw_handle(),
                None,
                0,
                Some(start_fn),
                call_param0,
                0,
                None,
                None,
            )
        }?;
        Ok(Thread::from_non_null(thread_handle))
    }

    fn as_raw_handle(&self) -> HANDLE {
        self.handle.entity
    }

    #[expect(dead_code)]
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

impl AsRef<Process> for Process {
    fn as_ref(&self) -> &Process {
        self
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

    pub fn join(&self) -> io::Result<()> {
        let event = unsafe { WaitForSingleObject(self.handle.entity, INFINITE) };
        match event {
            _ if event == WAIT_OBJECT_0 => Ok(()),
            _ if event == WAIT_FAILED => Err(io::Error::last_os_error()),
            _ if event == WAIT_ABANDONED => Err(io::ErrorKind::InvalidData.into()),
            _ if event == WAIT_TIMEOUT => Err(io::ErrorKind::TimedOut.into()),
            _ => unreachable!(),
        }
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
    pub fn set_priority(&self, priority: ThreadPriority) -> Result<(), io::Error> {
        unsafe { SetThreadPriority(self.handle.entity, priority.into())? };
        Ok(())
    }

    pub fn get_id(&self) -> ThreadId {
        let id = unsafe { GetThreadId(self.handle.entity) };
        ThreadId(id)
    }

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
            Thread32First(snapshot.entity, &raw mut thread_entry)?;
        }
        result.push(Self::from_raw(thread_entry));
        loop {
            let mut thread_entry = get_empty_thread_entry();
            let next_ret_val = unsafe { Thread32Next(snapshot.entity, &raw mut thread_entry) };
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

pub struct ProcessMemoryAllocation<P: AsRef<Process>> {
    remote_ptr: *mut c_void,
    num_bytes: usize,
    process: P,
    pre_reserved: bool,
}

impl<P: AsRef<Process>> ProcessMemoryAllocation<P> {
    /// Allocates memory in another process and writes the raw bytes of `*data` into it.
    ///
    /// # Panics
    ///
    /// Will panic if the size of `data` is zero.
    pub fn with_data<D: ?Sized>(process: P, pre_reserve: bool, data: &D) -> io::Result<Self> {
        let data_size = mem::size_of_val(data);
        assert_ne!(data_size, 0);
        let allocation = Self::new_empty(process, pre_reserve, data_size)?;
        allocation.write(data)?;
        Ok(allocation)
    }

    fn new_empty(process: P, pre_reserve: bool, num_bytes: usize) -> io::Result<Self> {
        assert_ne!(num_bytes, 0);
        let mut allocation_type = MEM_COMMIT;
        if pre_reserve {
            // Potentially reserves less than a full 64K block, wasting address space: https://stackoverflow.com/q/31586303
            allocation_type |= MEM_RESERVE;
        }
        let remote_ptr = unsafe {
            VirtualAllocEx(
                process.as_ref().as_raw_handle(),
                None,
                num_bytes,
                allocation_type,
                PAGE_READWRITE,
            )
        }
        .if_null_get_last_error()?;
        Ok(Self {
            remote_ptr,
            num_bytes,
            process,
            pre_reserved: pre_reserve,
        })
    }

    fn write<D: ?Sized>(&self, data: &D) -> io::Result<()> {
        let data_size = mem::size_of_val(data);
        assert_ne!(data_size, 0);
        assert!(data_size <= self.num_bytes);
        unsafe {
            WriteProcessMemory(
                self.process.as_ref().as_raw_handle(),
                self.remote_ptr,
                ptr::from_ref(data).cast::<c_void>(),
                data_size,
                None,
            )?;
        }
        Ok(())
    }

    fn free(&self) -> io::Result<()> {
        let free_type = if self.pre_reserved {
            MEM_RELEASE
        } else {
            MEM_DECOMMIT
        };
        unsafe {
            VirtualFreeEx(
                self.process.as_ref().as_raw_handle(),
                self.remote_ptr,
                0,
                free_type,
            )?;
        }
        Ok(())
    }
}

impl<P: AsRef<Process>> Drop for ProcessMemoryAllocation<P> {
    fn drop(&mut self) {
        self.free().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use more_asserts::*;

    use super::*;
    use crate::module::ExecutableModule;
    use crate::string::ZeroTerminatedString;
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
        let curr_process = Process::current();
        let target_priority = IoPriority::Low;
        curr_process.set_io_priority(target_priority)?;
        let priority = curr_process.get_io_priority()?.unwrap();
        assert_eq!(priority, target_priority);
        Ok(())
    }

    #[test]
    fn write_process_memory() -> io::Result<()> {
        write_process_memory_internal(true)?;
        write_process_memory_internal(false)?;
        Ok(())
    }

    fn write_process_memory_internal(pre_reserve: bool) -> io::Result<()> {
        let process = Process::current();
        let memory = ProcessMemoryAllocation::with_data(process, pre_reserve, "123")?;
        assert!(!memory.remote_ptr.is_null());
        Ok(())
    }

    #[test]
    fn create_remote_thread_locally() -> io::Result<()> {
        let process = Process::current();
        let module = ExecutableModule::from_loaded("kernel32.dll")?;
        let load_library_fn_ptr = module.get_symbol_ptr_by_name("LoadLibraryA")?;
        let raw_lib_name = ZeroTerminatedString::from("kernel32.dll");
        let thread = unsafe {
            process.create_remote_thread(
                load_library_fn_ptr,
                Some(raw_lib_name.as_raw_pcstr().as_ptr().cast::<c_void>()),
            )
        }?;
        thread.join()
    }
}
