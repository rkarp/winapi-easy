use std::{
    io,
    ptr,
    sync::Mutex
};

use once_cell::sync::Lazy;
use winapi::{
    shared::winerror::{S_FALSE, S_OK},
    um::{
        combaseapi::CoInitializeEx,
        objbase::COINIT_MULTITHREADED
    },
};

use crate::custom_hresult_err;

pub fn initialize_com() -> io::Result<()> {
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
