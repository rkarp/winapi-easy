use std::{
    io,
    ops::Deref,
    ptr::{
        self,
        NonNull
    },
    sync::Mutex
};

use once_cell::sync::Lazy;
use winapi::{
    Interface,
    shared::{
        windef::HWND__,
        winerror::{
            S_FALSE,
            S_OK,
        },
        wtypesbase::CLSCTX_INPROC_SERVER
    },
    um::{
        combaseapi::{CoCreateInstance, CoInitializeEx},
        objbase::COINIT_MULTITHREADED,
        shobjidl_core::{
            CLSID_TaskbarList,
            ITaskbarList3,
            TBPF_ERROR,
            TBPF_INDETERMINATE,
            TBPF_NOPROGRESS,
            TBPF_NORMAL,
            TBPF_PAUSED,
            TBPFLAG,
        },
        wincon::GetConsoleWindow
    }
};
use wio::com::ComPtr;

use crate::{
    custom_hresult_err
};

#[derive(Copy, Clone)]
pub struct Window(NonNull<HWND__>);

impl Window {
    pub fn get_console_window() -> Option<Self> {
        unsafe { ptr::NonNull::new(GetConsoleWindow()).map(Self) }
    }
}

#[derive(Copy, Clone)]
#[repr(u32)]
pub enum ProgressState {
    NoProgress = TBPF_NOPROGRESS,
    /// Show a "working" animation. Needs animations on the taskbar to be enabled in the OS config,
    /// otherwise it will not show anything to the user.
    Indeterminate = TBPF_INDETERMINATE,
    Normal = TBPF_NORMAL,
    Error = TBPF_ERROR,
    Paused = TBPF_PAUSED,
}

fn initialize_com() -> io::Result<()> {
    static COM_INITIALIZED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
    let mut initialized = COM_INITIALIZED.lock().expect("Unexpected panic in previous mutex lock");
    if !*initialized {
        let init_result = unsafe {
            CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED)
        };
        match init_result {
            S_OK | S_FALSE => {
                *initialized = true;
                Ok(())
            }
            err_code => custom_hresult_err("Error initializing COM", err_code),
        }
    } else {
        Ok(())
    }
}

pub fn get_taskbar_list_3() -> io::Result<ComPtr<ITaskbarList3>> {
    initialize_com()?;
    unsafe {
        let mut tb_ptr: *mut ITaskbarList3 = ptr::null_mut();
        let hresult = CoCreateInstance(
            &CLSID_TaskbarList,
            ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &ITaskbarList3::uuidof(),
            &mut tb_ptr as *mut _ as *mut _,
        );
        match hresult {
            S_OK => Ok(ComPtr::from_raw(tb_ptr)),
            err_code => custom_hresult_err("Error creating ITaskbarList3 instance", err_code),
        }
    }
}

pub trait TaskbarFunctionality: Deref<Target = ITaskbarList3> + Sized + Copy {
    fn set_progress_state(self, window: Window, state: ProgressState) -> io::Result<()> {
        unsafe {
            match self.SetProgressState(window.0.as_ptr(), state as TBPFLAG) {
                S_OK => Ok(()),
                err_code => custom_hresult_err("Error setting progress state", err_code),
            }
        }
    }
    fn set_progress_value(self, window: Window, completed: u64, total: u64) -> io::Result<()> {
        unsafe {
            match self.SetProgressValue(window.0.as_ptr(), completed, total) {
                S_OK => Ok(()),
                err_code => custom_hresult_err("Error setting progress value", err_code),
            }
        }
    }
}
impl<T> TaskbarFunctionality for T where T: Deref<Target = ITaskbarList3> + Sized + Copy {}
