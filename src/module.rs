use std::io;
use std::path::Path;

use windows::Win32::Foundation::{
    FreeLibrary,
    HINSTANCE,
    HMODULE,
};
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleExW,
    LOAD_LIBRARY_AS_DATAFILE,
    LOAD_LIBRARY_AS_IMAGE_RESOURCE,
    LoadLibraryExW,
};

use crate::string::ZeroTerminatedWideString;

/// A handle to a module (EXE or DLL).
#[derive(Eq, PartialEq, Debug)]
pub struct Module {
    raw_handle: HMODULE,
}

impl Module {
    /// Returns the module handle of the currently executed code.
    pub fn current_process_exe() -> io::Result<Self> {
        let mut raw_handle: HMODULE = Default::default();
        unsafe { GetModuleHandleExW(0, None, &raw mut raw_handle) }?;
        Ok(Module { raw_handle })
    }

    /// Loads a DLL or EXE module as a data file usable for extracting resources.
    pub fn load_module_as_data_file(file_name: impl AsRef<Path>) -> io::Result<Self> {
        let file_name = ZeroTerminatedWideString::from_os_str(file_name.as_ref());
        let raw_handle: HMODULE = unsafe {
            LoadLibraryExW(
                file_name.as_raw_pcwstr(),
                None,
                LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
            )
        }?;
        Ok(Module { raw_handle })
    }

    pub(crate) fn as_hmodule(&self) -> HMODULE {
        self.raw_handle
    }

    #[allow(dead_code)]
    pub(crate) fn as_hinstance(&self) -> HINSTANCE {
        self.as_hmodule().into()
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.as_hmodule()) }.expect("Cannot release module");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_current_exe_module() -> io::Result<()> {
        let module = Module::current_process_exe()?;
        assert!(!module.as_hmodule().is_invalid());
        Ok(())
    }

    #[test]
    fn load_shell32_module() -> io::Result<()> {
        let module = Module::load_module_as_data_file("shell32.dll")?;
        assert!(!module.as_hmodule().is_invalid());
        Ok(())
    }
}
