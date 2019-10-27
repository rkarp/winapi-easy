/*!
Processes, threads.
*/

use std::io;

use winapi::{
    shared::ntdef::HANDLE,
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

use crate::WinErrCheckable;

/// A Windows process
#[derive(Clone)]
pub struct Process {
    raw_handle: HANDLE,
}

impl Process {
    /// Constructs a special handle that always points to the current process.
    ///
    /// When transferred to a different process, it will point to that process when used from it.
    pub fn current() -> Self {
        Process {
            raw_handle: unsafe { GetCurrentProcess() },
        }
    }

    /// Sets the current process to background processing mode.
    ///
    /// This will also lower the I/O priority of the process, which will lower the impact of heavy disk I/O on other processes.
    pub fn begin_background_mode() -> Result<(), io::Error> {
        unsafe {
            SetPriorityClass(
                Process::current().raw_handle,
                winbase::PROCESS_MODE_BACKGROUND_BEGIN,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Ends background processing mode for the current process.
    pub fn end_background_mode() -> Result<(), io::Error> {
        unsafe {
            SetPriorityClass(
                Process::current().raw_handle,
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
    pub fn set_priority(&mut self, priority: ProcessPriority) -> Result<(), io::Error> {
        unsafe { SetPriorityClass(self.raw_handle, priority as u32).if_null_get_last_error()? };
        Ok(())
    }
}

/// A thread inside a Windows process
#[derive(Clone)]
pub struct Thread {
    raw_handle: HANDLE,
}

impl Thread {
    /// Constructs a special handle that always points to the current thread.
    ///
    /// When transferred to a different thread, it will point to that thread when used from it.
    pub fn current() -> Self {
        Thread {
            raw_handle: unsafe { GetCurrentThread() },
        }
    }

    /// Sets the current thread to background processing mode.
    ///
    /// This will also lower the I/O priority of the threads, which will lower the impact of heavy disk I/O on other threads and processes.
    pub fn begin_background_mode() -> Result<(), io::Error> {
        unsafe {
            SetThreadPriority(
                Thread::current().raw_handle,
                winbase::THREAD_MODE_BACKGROUND_BEGIN as i32,
            )
            .if_null_get_last_error()?
        };
        Ok(())
    }

    /// Ends background processing mode for the current thread.
    pub fn end_background_mode() -> Result<(), io::Error> {
        unsafe {
            SetThreadPriority(
                Thread::current().raw_handle,
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
        unsafe { SetThreadPriority(self.raw_handle, priority as i32).if_null_get_last_error()? };
        Ok(())
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
