use std::io;

use winapi::{
    shared::{
        ntdef::HANDLE
    },
    um::{
        processthreadsapi::{
            GetCurrentProcess,
            GetCurrentThread,
            SetPriorityClass,
            SetThreadPriority,
        },
        winbase
    }
};

use crate::WinErrCheckable;

/// A Windows process
#[derive(Clone)]
pub struct Process {
    handle: HANDLE,
}

impl Process {
    pub fn current() -> Self {
        Process {
            handle: unsafe { GetCurrentProcess() },
        }
    }

    pub fn set_priority(&mut self, priority: ProcessPriority) -> Result<(), io::Error> {
        unsafe { SetPriorityClass(self.handle, priority as u32).if_null_get_last_error()? };
        Ok(())
    }
}

/// A thread inside a Windows process
#[derive(Clone)]
pub struct Thread {
    handle: HANDLE,
}

impl Thread {
    /// Constructs a special handle that always points to the current thread.
    pub fn current() -> Self {
        Thread {
            handle: unsafe { GetCurrentThread() },
        }
    }

    pub fn set_priority(&mut self, priority: ThreadPriority) -> Result<(), io::Error> {
        unsafe { SetThreadPriority(self.handle, priority as i32).if_null_get_last_error()? };
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum ProcessPriority {
    BackgroundModeBegin = winbase::PROCESS_MODE_BACKGROUND_BEGIN,
    BackgroundModeEnd = winbase::PROCESS_MODE_BACKGROUND_END,
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
    BackgroundModeBegin = winbase::THREAD_MODE_BACKGROUND_BEGIN,
    BackgroundModeEnd = winbase::THREAD_MODE_BACKGROUND_END,
    Idle = winbase::THREAD_PRIORITY_IDLE,
    Lowest = winbase::THREAD_PRIORITY_LOWEST,
    BelowNormal = winbase::THREAD_PRIORITY_BELOW_NORMAL,
    Normal = winbase::THREAD_PRIORITY_NORMAL,
    AboveNormal = winbase::THREAD_PRIORITY_ABOVE_NORMAL,
    Highest = winbase::THREAD_PRIORITY_HIGHEST,
    TimeCritical = winbase::THREAD_PRIORITY_TIME_CRITICAL,
}
