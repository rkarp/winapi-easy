/*!
Component Object Model (COM) initialization.
*/

use std::cell::Cell;
use std::io;

use windows::core::{
    ComInterface,
    GUID,
};
use windows::Win32::System::Com::{
    CoCreateInstance,
    CoInitializeEx,
    CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};

/// Initializes the COM library for the current thread. Will do nothing on further calls from the same thread.
pub fn initialize_com() -> io::Result<()> {
    thread_local! {
        static COM_INITIALIZED: Cell<bool> = Cell::new(false);
    }
    COM_INITIALIZED.with(|initialized| {
        if !initialized.get() {
            let init_result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            match init_result {
                Ok(()) => {
                    initialized.set(true);
                    Ok(())
                }
                Err(err) => Err(err.into()),
            }
        } else {
            Ok(())
        }
    })
}

pub(crate) trait ComInterfaceExt: ComInterface {
    const CLASS_GUID: GUID;

    fn new_instance() -> io::Result<Self> {
        initialize_com()?;
        let result = unsafe { CoCreateInstance(&Self::CLASS_GUID, None, CLSCTX_INPROC_SERVER) };
        result.map_err(Into::into)
    }
}
