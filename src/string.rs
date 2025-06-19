#![allow(dead_code)]

use std::borrow::Cow;
use std::ffi::{
    CString,
    OsStr,
    OsString,
};
use std::iter::once;
use std::os::windows::ffi::{
    OsStrExt,
    OsStringExt,
};
use std::path::Path;
use std::{
    io,
    mem,
};

use windows::Win32::Foundation::UNICODE_STRING;
use windows::core::{
    PCSTR,
    PCWSTR,
    PWSTR,
};

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ZeroTerminatedString(CString);

impl ZeroTerminatedString {
    pub(crate) fn as_raw_pcstr(&self) -> PCSTR {
        PCSTR(self.0.as_ptr().cast::<u8>())
    }
}

impl<T: AsRef<str>> From<T> for ZeroTerminatedString {
    fn from(value: T) -> Self {
        Self(CString::new(value.as_ref()).unwrap_or_else(|_| unreachable!()))
    }
}

pub(crate) trait ToWideString: AsRef<OsStr> + Sized {
    /// Converts to a zero-terminated wide string.
    fn to_wide_string(&self) -> Vec<u16> {
        to_wide_chars_iter(self).collect()
    }
}
impl<T: AsRef<OsStr> + Sized> ToWideString for T {}

pub(crate) fn to_wide_chars_iter(
    str: &(impl AsRef<OsStr> + ?Sized),
) -> impl Iterator<Item = u16> + '_ {
    str.as_ref().encode_wide().chain(once(0))
}

pub(crate) trait FromWideString: AsRef<[u16]> + Sized {
    fn to_string_lossy(&self) -> String {
        self.to_os_string().to_string_lossy().into_owned()
    }

    fn to_os_string(&self) -> OsString {
        OsString::from_wide(self.as_ref())
    }
}
impl<T: AsRef<[u16]> + Sized> FromWideString for T {}

pub(crate) struct ZeroTerminatedWideString(pub Vec<u16>);

impl ZeroTerminatedWideString {
    pub(crate) fn from_os_str<T: AsRef<OsStr> + Sized>(input: T) -> Self {
        Self(input.to_wide_string())
    }

    pub(crate) fn to_os_string(&self) -> OsString {
        // Make this more efficient at some point, see also: https://stackoverflow.com/q/42066381
        let first_zero_index = self.0.iter().position(|x| *x == 0).unwrap();
        // Remove terminating 0
        self.0[..first_zero_index].as_ref().to_os_string()
    }

    pub(crate) fn as_raw_pcwstr(&self) -> PCWSTR {
        PCWSTR::from_raw(self.0.as_ptr())
    }

    pub(crate) fn as_raw_pwstr(&mut self) -> PWSTR {
        PWSTR::from_raw(self.0.as_mut_ptr())
    }
}

pub(crate) struct WinUnicodeString {
    win_unicode_string: UNICODE_STRING,
    wide_string: Vec<u16>,
}

impl WinUnicodeString {
    #[expect(dead_code)]
    pub(crate) fn new(string: &str) -> io::Result<Self> {
        let os_str: &OsStr = string.as_ref();
        let mut wide_string: Vec<u16> = os_str.encode_wide().collect();
        Ok(WinUnicodeString {
            win_unicode_string: UNICODE_STRING {
                Length: u16::try_from(wide_string.len() * mem::size_of::<u16>())
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?,
                MaximumLength: 0,
                Buffer: PWSTR::from_raw(wide_string.as_mut_ptr()),
            },
            wide_string,
        })
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

pub(crate) fn max_path_extend(path: &OsStr) -> Cow<'_, OsStr> {
    let special_prefix: &OsStr = OsStr::new(r"\\?\");
    if Path::new(path).starts_with(special_prefix) {
        Cow::Borrowed(path)
    } else {
        let mut combined_path = special_prefix.to_os_string();
        combined_path.push(path);
        Cow::Owned(combined_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_zero_terminated_wstr() {
        const TEST_STR: &'static str = "test";
        let transformed_string = unsafe {
            ZeroTerminatedWideString::from_os_str(TEST_STR)
                .as_raw_pcwstr()
                .to_string()
                .unwrap()
        };
        assert_eq!(&transformed_string, TEST_STR);
    }
}
