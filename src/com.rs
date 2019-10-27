/*!
Component Object Model (COM) initialization.
*/

use std::{
    cell::Cell,
    io,
    ptr,
};

use winapi::{
    Interface,
    shared::{
        guiddef::IID,
        winerror::{
            S_FALSE,
            S_OK,
        },
        wtypesbase::CLSCTX_INPROC_SERVER,
    },
    um::{
        combaseapi::{
            CoCreateInstance,
            CoInitializeEx,
        },
        objbase::COINIT_MULTITHREADED,
        shobjidl_core::{
            CLSID_TaskbarList,
            ITaskbarList3,
        },
    },
};
use wio::com::ComPtr;

use crate::custom_hresult_err;

/// Initializes the COM library for the current thread. Will do nothing on further calls from the same thread.
pub fn initialize_com() -> io::Result<()> {
    thread_local! {
        static COM_INITIALIZED: Cell<bool> = Cell::new(false);
    }
    COM_INITIALIZED.with(|initialized| {
        if !initialized.get() {
            let init_result = unsafe { CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED) };
            match init_result {
                S_OK | S_FALSE => {
                    initialized.set(true);
                    Ok(())
                }
                err_code => custom_hresult_err("Error initializing COM", err_code),
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
                err_code => custom_hresult_err("Error creating ComInterface instance", err_code),
            }
        }
    }
}

impl ComInterface for ITaskbarList3 {
    const CLSID: IID = CLSID_TaskbarList;
}
