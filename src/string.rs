use std::ffi::{
    OsStr,
    OsString,
};
use std::iter::once;
use std::mem;
use std::os::windows::ffi::{
    OsStrExt,
    OsStringExt,
};

use windows::core::{
    PCWSTR,
    PWSTR,
};
use windows::Win32::Foundation::UNICODE_STRING;

pub(crate) trait ToWideString: AsRef<OsStr> + Sized {
    fn to_wide_string(&self) -> Vec<u16> {
        to_wide_chars_iter(self).collect()
    }
}
impl<T: AsRef<OsStr> + Sized> ToWideString for T {}

#[allow(clippy::needless_lifetimes)]
pub(crate) fn to_wide_chars_iter<'a>(
    str: &'a (impl AsRef<OsStr> + ?Sized),
) -> impl Iterator<Item = u16> + 'a {
    str.as_ref().encode_wide().chain(once(0))
}

pub(crate) trait FromWideString: AsRef<[u16]> + Sized {
    fn to_string_lossy(&self) -> String {
        let os_string: OsString = OsString::from_wide(self.as_ref());
        os_string.to_string_lossy().into_owned()
    }
}
impl<T: AsRef<[u16]> + Sized> FromWideString for T {}

pub(crate) struct ZeroTerminatedWideString(Vec<u16>);

impl ZeroTerminatedWideString {
    pub(crate) fn from_os_str<T: AsRef<OsStr> + Sized>(input: T) -> Self {
        Self(input.to_wide_string())
    }

    pub(crate) fn as_raw_pcwstr(&self) -> PCWSTR {
        PCWSTR::from_raw(self.0.as_ptr())
    }
}

pub(crate) struct WinUnicodeString {
    win_unicode_string: UNICODE_STRING,
    wide_string: Vec<u16>,
}

impl WinUnicodeString {
    #[allow(dead_code)]
    pub(crate) fn new(string: &str) -> Self {
        let os_str: &OsStr = string.as_ref();
        let mut wide_string: Vec<u16> = os_str.encode_wide().collect();
        WinUnicodeString {
            win_unicode_string: UNICODE_STRING {
                Length: (wide_string.len() * mem::size_of::<u16>()) as u16,
                MaximumLength: 0,
                Buffer: PWSTR::from_raw(wide_string.as_mut_ptr()),
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

impl AsRef<[u16]> for WinUnicodeString {
    fn as_ref(&self) -> &[u16] {
        &self.wide_string
    }
}

impl AsMut<[u16]> for WinUnicodeString {
    fn as_mut(&mut self) -> &mut [u16] {
        &mut self.wide_string
    }
}
