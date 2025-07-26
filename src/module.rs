use std::ffi::c_void;
use std::path::Path;
use std::{
    io,
    ptr,
};

use windows::Win32::Foundation::{
    FreeLibrary,
    HINSTANCE,
    HMODULE,
};
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleExW,
    GetProcAddress,
    LOAD_LIBRARY_AS_DATAFILE,
    LOAD_LIBRARY_AS_IMAGE_RESOURCE,
    LOAD_LIBRARY_FLAGS,
    LoadLibraryExW,
};
use windows::core::PCSTR;

use crate::internal::ResultExt;
use crate::string::{
    ZeroTerminatedString,
    ZeroTerminatedWideString,
};

/// A handle to a module (EXE or DLL).
#[derive(Eq, PartialEq, Debug)]
pub struct ExecutableModule {
    raw_handle: HMODULE,
}

impl ExecutableModule {
    /// Returns the module handle of the currently executed code.
    pub fn from_current_process_exe() -> io::Result<Self> {
        Self::get_loaded_internal(None::<&Path>)
    }

    pub fn from_loaded<A: AsRef<Path>>(name: A) -> io::Result<Self> {
        Self::get_loaded_internal(Some(name))
    }

    fn get_loaded_internal(name: Option<impl AsRef<Path>>) -> io::Result<Self> {
        let name_wide = name.map(|x| ZeroTerminatedWideString::from_os_str(x.as_ref()));
        let name_param = name_wide
            .as_ref()
            .map(ZeroTerminatedWideString::as_raw_pcwstr);
        let mut raw_handle: HMODULE = Default::default();
        unsafe { GetModuleHandleExW(0, name_param.as_ref(), &raw mut raw_handle) }?;
        Ok(ExecutableModule { raw_handle })
    }

    /// Loads a DLL or EXE module as a data file usable for extracting resources.
    pub fn load_module_as_data_file<P: AsRef<Path>>(file_name: P) -> io::Result<Self> {
        Self::load_module_internal(
            file_name,
            LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        )
    }

    /// Loads a DLL or EXE module.
    pub fn load_module<P: AsRef<Path>>(file_name: P) -> io::Result<Self> {
        Self::load_module_internal(file_name, Default::default())
    }

    fn load_module_internal(
        file_name: impl AsRef<Path>,
        flags: LOAD_LIBRARY_FLAGS,
    ) -> io::Result<Self> {
        let file_name = ZeroTerminatedWideString::from_os_str(file_name.as_ref());
        let raw_handle: HMODULE =
            unsafe { LoadLibraryExW(file_name.as_raw_pcwstr(), None, flags) }?;
        Ok(ExecutableModule { raw_handle })
    }

    pub fn get_symbol_ptr_by_ordinal(&self, ordinal: u16) -> io::Result<*const c_void> {
        self.get_symbol_ptr(&SymbolIdentifier::from(ordinal))
    }

    pub fn get_symbol_ptr_by_name<S: AsRef<str>>(&self, name: S) -> io::Result<*const c_void> {
        self.get_symbol_ptr(&SymbolIdentifier::from(name.as_ref()))
    }

    fn get_symbol_ptr(&self, symbol: &SymbolIdentifier) -> io::Result<*const c_void> {
        let symbol_ptr = unsafe { GetProcAddress(self.as_hmodule(), symbol.as_param()) }
            .ok_or_else(io::Error::last_os_error)?;
        Ok(ptr::with_exposed_provenance(symbol_ptr as usize))
    }

    pub(crate) fn as_hmodule(&self) -> HMODULE {
        self.raw_handle
    }

    #[allow(dead_code)]
    pub(crate) fn as_hinstance(&self) -> HINSTANCE {
        self.as_hmodule().into()
    }
}

impl Drop for ExecutableModule {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.as_hmodule()) }.unwrap_or_default_and_print_error();
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum SymbolIdentifier {
    Ordinal(u16),
    Name(ZeroTerminatedString),
}

impl SymbolIdentifier {
    fn as_param(&self) -> PCSTR {
        match self {
            SymbolIdentifier::Ordinal(ordinal) => PCSTR(usize::from(*ordinal) as *const u8),
            SymbolIdentifier::Name(name) => name.as_raw_pcstr(),
        }
    }
}

impl From<u16> for SymbolIdentifier {
    fn from(value: u16) -> Self {
        Self::Ordinal(value)
    }
}

impl From<&str> for SymbolIdentifier {
    fn from(value: &str) -> Self {
        Self::Name(ZeroTerminatedString::from(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_current_exe_module() -> io::Result<()> {
        let module = ExecutableModule::from_current_process_exe()?;
        assert!(!module.as_hmodule().is_invalid());
        Ok(())
    }

    #[test]
    fn load_shell32_module() -> io::Result<()> {
        let module = ExecutableModule::load_module_as_data_file("shell32.dll")?;
        assert!(!module.as_hmodule().is_invalid());
        Ok(())
    }

    #[test]
    fn get_symbol_ptr() -> io::Result<()> {
        let module = ExecutableModule::from_loaded("kernel32.dll")?;
        let symbol_ptr = module.get_symbol_ptr_by_name("GetProcAddress")?;
        assert!(!symbol_ptr.is_null());
        Ok(())
    }
}
