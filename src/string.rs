use std::ffi::{
    OsStr,
    OsString,
};
use std::mem;
use std::os::windows::ffi::{
    OsStrExt,
    OsStringExt,
};

use winapi::shared::ntdef::{
    UNICODE_STRING,
    WCHAR,
};

pub(crate) trait ToWideString: AsRef<OsStr> + Sized {}
impl<T: AsRef<OsStr> + Sized> ToWideString for T {}

pub(crate) trait FromWideString: AsRef<[WCHAR]> + Sized {
    fn into_string_lossy(self) -> String {
        let os_string: OsString = OsString::from_wide(self.as_ref());
        os_string.to_string_lossy().into_owned()
    }
}
impl<T: AsRef<[WCHAR]> + Sized> FromWideString for T {}

pub(crate) struct WinUnicodeString {
    win_unicode_string: UNICODE_STRING,
    wide_string: Vec<WCHAR>,
}

impl WinUnicodeString {
    #[allow(dead_code)]
    pub(crate) fn new(string: &str) -> Self {
        let os_str: &OsStr = string.as_ref();
        let wide_string: Vec<WCHAR> = os_str.encode_wide().collect();
        WinUnicodeString {
            win_unicode_string: UNICODE_STRING {
                Length: (wide_string.len() * mem::size_of::<WCHAR>()) as u16,
                MaximumLength: 0,
                Buffer: wide_string.as_ptr() as *mut _,
            },
            wide_string,
        }
    }
}

impl AsRef<UNICODE_STRING> for WinUnicodeString {
    fn as_ref(&self) -> &UNICODE_STRING {
        &self.win_unicode_string
    }
}

impl AsMut<UNICODE_STRING> for WinUnicodeString {
    fn as_mut(&mut self) -> &mut UNICODE_STRING {
        &mut self.win_unicode_string
    }
}

impl AsRef<[WCHAR]> for WinUnicodeString {
    fn as_ref(&self) -> &[WCHAR] {
        &self.wide_string
    }
}

impl AsMut<[WCHAR]> for WinUnicodeString {
    fn as_mut(&mut self) -> &mut [WCHAR] {
        &mut self.wide_string
    }
}
