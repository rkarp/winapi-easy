/*!
Clipboard access.
*/

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

/// An opened Windows clipboard.
///
/// Will be closed again when dropped.
pub struct Clipboard(());

impl Clipboard {
    pub fn new() -> io::Result<Clipboard> {
        unsafe {
            OpenClipboard(None)
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
                let clipboard_data = GetClipboardData(CF_HDROP.0.into())?;
                GlobalLockedData::lock(clipboard_data)?
            };

            let num_files = DragQueryFileW(HDROP(clipboard_data.ptr() as isize), u32::MAX, None);
            let file_names: io::Result<Vec<PathBuf>> = (0..num_files)
                .into_iter()
                .map(|file_index| {
                    let required_size =
                        1 + DragQueryFileW(HDROP(clipboard_data.ptr() as isize), file_index, None)
                            .if_null_to_error(|| io::ErrorKind::Other.into())?;
                    let file_str_buf = {
                        let mut buffer = vec![0; required_size as usize];
                        DragQueryFileW(
                            HDROP(clipboard_data.ptr() as isize),
                            file_index,
                            Some(buffer.as_mut_slice()),
                        )
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
