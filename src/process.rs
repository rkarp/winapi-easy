/*!
Processes, threads.
*/

use std::borrow::BorrowMut;
use std::io;
use std::ptr::NonNull;

use winapi::{
    um::{
        processthreadsapi::{
            GetCurrentProcess,
            GetCurrentThread,
            SetPriorityClass,
            SetThreadPriority,
        },
        winbase,
    },
};
use winapi::ctypes::c_void;
use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
use winapi::um::processthreadsapi::{GetProcessId, GetThreadId, OpenProcess, OpenThread};
use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, Thread32First, Thread32Next};
use winapi::um::tlhelp32::THREADENTRY32;
use winapi::um::winnt::{PROCESS_ALL_ACCESS, THREAD_ALL_ACCESS};

use crate::internal::{AutoClose, WinErrCheckable, WinErrCheckableHandle};

/// A Windows process
pub struct Process {
    raw_handle: NonNull<c_void>,
}

impl Process {
    /// Constructs a special handle that always points to the current process.
    ///
    /// When transferred to a different process, it will point to that process when used from it.
    pub fn current() -> Self {
        let pseudo_handle = unsafe {
            GetCurrentProcess()
        };
        Self {
            raw_handle: pseudo_handle.to_non_null().expect("Pseudo process handle should never be null"),
        }
    }

    pub fn from_id<I>(id: I) -> io::Result<Self>
        where I: Into<ProcessId>
    {
        let raw_handle = unsafe {
            OpenProcess(PROCESS_ALL_ACCESS, FALSE, id.into().0)
                .to_non_null_else_get_last_error()?
        };
        Ok(Self {
            raw_handle
        })
    }

    /// Sets the current process to background processing mode.
    ///
    /// This will also lower the I/O priority of the process, which will lower the impact of heavy disk I/O on other processes.
    pub fn begin_background_mode() -> io::Result<()> {
        unsafe {
            SetPriorityClass(
                Process::current().raw_handle.as_mut(),
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
                Process::current().raw_handle.as_mut(),
                winbase::PROCESS_MODE_BACKGROUND_END,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Sets the priority of the given process
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use winapi_easy::process::{Process, ProcessPriority};
    ///
    /// Process::current().set_priority(ProcessPriority::Idle);
    /// ```
    pub fn set_priority(&mut self, priority: ProcessPriority) -> io::Result<()> {
        unsafe { SetPriorityClass(self.raw_handle.as_mut(), priority as u32).if_null_get_last_error()? };
        Ok(())
    }

    fn get_id(&mut self) -> DWORD {
        unsafe {
            GetProcessId(self.raw_handle.as_mut())
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ProcessId(DWORD);

impl<P> From<P> for ProcessId
where P: BorrowMut<Process>
{
    fn from(mut process: P) -> Self {
        ProcessId(process.borrow_mut().get_id())
    }
}

/// A thread inside a Windows process
pub struct Thread {
    raw_handle: NonNull<c_void>,
}

impl Thread {
    /// Constructs a special handle that always points to the current thread.
    ///
    /// When transferred to a different thread, it will point to that thread when used from it.
    pub fn current() -> Self {
        let pseudo_handle = unsafe { GetCurrentThread() };
        Thread {
            raw_handle: pseudo_handle.to_non_null().expect("Pseudo thread handle should never be null"),
        }
    }

    pub fn from_id<I>(id: I) -> io::Result<Self>
        where I: Into<ThreadId>
    {
        let raw_handle = unsafe {
            OpenThread(THREAD_ALL_ACCESS, FALSE, id.into().0)
                .to_non_null_else_get_last_error()?
        };
        Ok(Self {
            raw_handle
        })
    }

    /// Sets the current thread to background processing mode.
    ///
    /// This will also lower the I/O priority of the threads, which will lower the impact of heavy disk I/O on other threads and processes.
    pub fn begin_background_mode() -> io::Result<()> {
        unsafe {
            SetThreadPriority(
                Thread::current().raw_handle.as_mut(),
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
                Thread::current().raw_handle.as_mut(),
                winbase::THREAD_MODE_BACKGROUND_END as i32,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Sets the priority of the given thread
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use winapi_easy::process::{Thread, ThreadPriority};
    ///
    /// Thread::current().set_priority(ThreadPriority::Idle);
    /// ```
    pub fn set_priority(&mut self, priority: ThreadPriority) -> Result<(), io::Error> {
        unsafe { SetThreadPriority(self.raw_handle.as_mut(), priority as i32).if_null_get_last_error()? };
        Ok(())
    }

    #[allow(dead_code)]
    fn get_id(&mut self) -> DWORD {
        unsafe {
            GetThreadId(self.raw_handle.as_mut())
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ThreadId(DWORD);

impl<T> From<T> for ThreadId
    where T: BorrowMut<Thread>
{
    fn from(mut thread: T) -> Self {
        ThreadId(thread.borrow_mut().get_id())
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
        }.into();

        let mut thread_entry = get_empty_thread_entry();
        unsafe {
            Thread32First(snapshot.as_mut(), &mut thread_entry).if_null_get_last_error()?;
        }
        result.push(Self::from_raw(thread_entry));
        loop {
            let mut thread_entry = get_empty_thread_entry();
            let next_ret_val = unsafe {
                Thread32Next(snapshot.as_mut(), &mut thread_entry)
            };
            if next_ret_val == TRUE {
                result.push(Self::from_raw(thread_entry));
            } else {
                break;
            }
        }
        Ok(result)
    }

    pub fn all_process_threads<P>(process_id: P) -> io::Result<Vec<Self>>
    where P: Into<ProcessId>
    {
        let pid: ProcessId = process_id.into();
        let result = Self::all_threads()?
            .into_iter()
            .filter(|thread_info| thread_info.get_owner_process_id() == pid.0)
            .collect();
        Ok(result)
    }

    fn from_raw(raw_info: THREADENTRY32) -> Self {
        ThreadInfo {
            raw_entry: raw_info
        }
    }

    #[allow(dead_code)]
    fn get_thread_id(&self) -> DWORD {
        self.raw_entry.th32ThreadID
    }

    fn get_owner_process_id(&self) -> DWORD {
        self.raw_entry.th32OwnerProcessID
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum ProcessPriority {
    Idle = winbase::IDLE_PRIORITY_CLASS,
    BelowNormal = winbase::BELOW_NORMAL_PRIORITY_CLASS,
    Normal = winbase::NORMAL_PRIORITY_CLASS,
    AboveNormal = winbase::ABOVE_NORMAL_PRIORITY_CLASS,
    High = winbase::HIGH_PRIORITY_CLASS,
    Realtime = winbase::REALTIME_PRIORITY_CLASS,
}

#[derive(Clone, Copy, Debug)]
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
        let all_threads = ThreadInfo::all_process_threads(Process::current())?;
        assert_gt!(all_threads.len(), 0);
        Ok(())
    }
}