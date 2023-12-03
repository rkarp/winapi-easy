//! Clipboard access.

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::System::DataExchange::{
    CloseClipboard,
    GetClipboardData,
    OpenClipboard,
};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::UI::Shell::{
    DragQueryFileW,
    HDROP,
};

use crate::internal::{
    GlobalLockedData,
    ReturnValue,
};

/// Returns a list of file paths that have been copied to the clipboard.
///
/// Will return `Err` if the clipboard cannot be accessed or does not contain files.
pub fn get_file_list() -> io::Result<Vec<PathBuf>> {
    let f = || {
        let mut clipboard_data = {
            let clipboard_data = unsafe { GetClipboardData(CF_HDROP.0.into()) }?;
            GlobalLockedData::lock(clipboard_data)?
        };

        let num_files =
            unsafe { DragQueryFileW(HDROP(clipboard_data.ptr() as isize), u32::MAX, None) };
        let file_names: io::Result<Vec<PathBuf>> = (0..num_files)
            .into_iter()
            .map(|file_index| {
                let required_size = unsafe {
                    1 + DragQueryFileW(HDROP(clipboard_data.ptr() as isize), file_index, None)
                }
                .if_null_to_error(|| io::ErrorKind::Other.into())?;
                let file_str_buf = {
                    let mut buffer = vec![0; required_size as usize];
                    unsafe {
                        DragQueryFileW(
                            HDROP(clipboard_data.ptr() as isize),
                            file_index,
                            Some(buffer.as_mut_slice()),
                        )
                    }
                    .if_null_to_error(|| io::ErrorKind::Other.into())?;
                    // Set length, remove terminating zero
                    buffer.truncate(buffer.len() - 1);
                    buffer
                };
                let os_string = OsString::from_wide(&file_str_buf);
                Ok(PathBuf::from(os_string))
            })
            .collect();
        file_names
    };
    with_open_clipboard_do(f)
}

fn with_open_clipboard_do<F, R>(f: F) -> io::Result<R>
where
    F: FnOnce() -> io::Result<R>,
{
    unsafe {
        OpenClipboard(None).if_null_get_last_error()?;
    }
    let result = f();
    unsafe {
        CloseClipboard().if_null_get_last_error()?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn open_clipboard() -> io::Result<()> {
        with_open_clipboard_do(|| {
            std::thread::sleep(Duration::from_millis(0));
            Ok(())
        })
    }
}
