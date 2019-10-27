/*!
Clipboard access.
*/

use std::{
    ffi::OsString,
    io,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    ptr,
};

use winapi::{
    um::{
        shellapi::{
            DragQueryFileW,
            HDROP,
        },
        winuser::{
            CF_HDROP,
            GetClipboardData,
            CloseClipboard,
            OpenClipboard,
        },
    },
};

use crate::{
    GlobalLockedData,
    WinErrCheckable,
};

pub struct Clipboard(());

/// An opened Windows clipboard. Will be closed again when dropped.
impl Clipboard {
    pub fn new() -> io::Result<Clipboard> {
        unsafe {
            OpenClipboard(ptr::null_mut())
                .if_null_get_last_error()
                .map(|_| Clipboard(()))
        }
    }

    /// Returns a list of file paths that have been copied to the clipboard.
    ///
    /// Will return `Err` if the clipboard cannot be accessed or does not contain files.
    pub fn get_file_list(&self) -> io::Result<Vec<PathBuf>> {
        unsafe {
            let mut clipboard_data = {
                let clipboard_data = GetClipboardData(CF_HDROP).if_null_get_last_error()?;
                GlobalLockedData::lock(clipboard_data)?
            };

            let num_files = DragQueryFileW(
                clipboard_data.ptr() as HDROP,
                std::u32::MAX,
                ptr::null_mut(),
                0,
            );
            let file_names: io::Result<Vec<PathBuf>> = (0..num_files)
                .into_iter()
                .map(|file_index| {
                    let required_size = 1 + DragQueryFileW(
                        clipboard_data.ptr() as HDROP,
                        file_index,
                        ptr::null_mut(),
                        0,
                    )
                    .if_null_to_error(|| io::ErrorKind::Other.into())?;
                    let file_str_buf = {
                        let mut buffer = Vec::with_capacity(required_size as usize);
                        DragQueryFileW(
                            clipboard_data.ptr() as HDROP,
                            file_index,
                            buffer.as_mut_ptr(),
                            required_size,
                        )
                        .if_null_to_error(|| io::ErrorKind::Other.into())?;
                        // Set length, remove terminating zero
                        buffer.set_len(required_size as usize - 1);
                        buffer
                    };
                    let os_string = OsString::from_wide(&file_str_buf);
                    Ok(PathBuf::from(os_string))
                })
                .collect();
            file_names
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
