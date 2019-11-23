/*!
Component Object Model (COM) initialization.
*/

use std::cell::Cell;
use std::io;
use std::ptr;

use winapi::shared::guiddef::IID;
use winapi::shared::winerror::{
    S_FALSE,
    S_OK,
};
use winapi::shared::wtypesbase::CLSCTX_INPROC_SERVER;
use winapi::um::combaseapi::{
    CoCreateInstance,
    CoInitializeEx,
};
use winapi::um::objbase::COINIT_APARTMENTTHREADED;
use winapi::um::shobjidl_core::{
    CLSID_TaskbarList,
    ITaskbarList3,
};
use winapi::Interface;
use wio::com::ComPtr;

use crate::internal::custom_err_with_code;

/// Initializes the COM library for the current thread. Will do nothing on further calls from the same thread.
pub fn initialize_com() -> io::Result<()> {
    thread_local! {
        static COM_INITIALIZED: Cell<bool> = Cell::new(false);
    }
    COM_INITIALIZED.with(|initialized| {
        if !initialized.get() {
            let init_result = unsafe { CoInitializeEx(ptr::null_mut(), COINIT_APARTMENTTHREADED) };
            match init_result {
                S_OK | S_FALSE => {
                    initialized.set(true);
                    Ok(())
                }
                err_code => Err(custom_err_with_code("Error initializing COM", err_code)),
            }
        } else {
            Ok(())
        }
    })
}

pub(crate) trait ComInterface: Interface + Sized {
    const CLSID: IID;

    fn new_instance() -> io::Result<ComPtr<Self>> {
        initialize_com()?;
        unsafe {
            let mut tb_ptr: *mut Self = ptr::null_mut();
            let hresult = CoCreateInstance(
                &Self::CLSID,
                ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &Self::uuidof(),
                &mut tb_ptr as *mut _ as *mut _,
            );
            match hresult {
                S_OK => Ok(ComPtr::from_raw(tb_ptr)),
                err_code => Err(custom_err_with_code(
                    "Error creating ComInterface instance",
                    err_code,
                )),
            }
        }
    }
}

impl ComInterface for ITaskbarList3 {
    const CLSID: IID = CLSID_TaskbarList;
}
