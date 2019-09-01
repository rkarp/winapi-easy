use std::{
    cell::Cell,
    io,
    ptr,
};

use winapi::{
    shared::winerror::{
        S_FALSE,
        S_OK,
    },
    um::{
        combaseapi::CoInitializeEx,
        objbase::COINIT_MULTITHREADED,
    },
};

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
