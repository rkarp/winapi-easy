use std::path::Path;
use std::{
    io,
    ptr,
};

use windows::Win32::Storage::FileSystem::{
    CopyFileExW,
    MoveFileExW,
    MOVEFILE_COPY_ALLOWED,
    MOVEFILE_WRITE_THROUGH,
};
use windows::Win32::System::WindowsProgramming::{
    COPY_FILE_COPY_SYMLINK,
    COPY_FILE_FAIL_IF_EXISTS,
};

use crate::internal::ReturnValue;

pub trait PathExt: AsRef<Path> {
    /// Copies a file.
    ///
    /// - Will copy symlinks themselves, not their targets
    /// - Will block until the operation is complete
    /// - Will fail if the target path already exists
    fn copy_file_to<Q: AsRef<Path>>(&self, new_path: Q) -> io::Result<()> {
        unsafe {
            CopyFileExW(
                self.as_ref().as_os_str(),
                new_path.as_ref().as_os_str(),
                None,
                ptr::null(),
                ptr::null_mut(),
                COPY_FILE_COPY_SYMLINK | COPY_FILE_FAIL_IF_EXISTS,
            )
            .0
            .if_null_get_last_error()?;
        }
        Ok(())
    }

    /// Moves a file or directory within a volume or a file between volumes.
    ///
    /// - The operation is equivalent to a rename if `new_name` is on the same volume.
    /// - Only files can be moved between volumes, not directories.
    /// - Will block until the operation is complete
    /// - Will fail if the target path already exists
    fn move_to<Q: AsRef<Path>>(&self, new_path: Q) -> io::Result<()> {
        unsafe {
            MoveFileExW(
                self.as_ref().as_os_str(),
                new_path.as_ref().as_os_str(),
                MOVEFILE_COPY_ALLOWED | MOVEFILE_WRITE_THROUGH,
            )
            .0
            .if_null_get_last_error()?;
        }
        Ok(())
    }
}

impl<T: AsRef<Path>> PathExt for T {}
