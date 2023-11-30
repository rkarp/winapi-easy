//! Filesystem functionality.

use std::io;
use std::path::Path;
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
use crate::string::ZeroTerminatedWideString;

/// Additional methods on [Path] using Windows-specific functionality.
pub trait PathExt: AsRef<Path> {
    /// Copies a file.
    ///
    /// - Will copy symlinks themselves, not their targets.
    /// - Will block until the operation is complete.
    /// - Will fail if the target path already exists.
    fn copy_file_to<Q: AsRef<Path>>(&self, new_path: Q) -> io::Result<()> {
        let source = ZeroTerminatedWideString::from_os_str(self.as_ref().as_os_str());
        let target = ZeroTerminatedWideString::from_os_str(new_path.as_ref().as_os_str());
        unsafe {
            CopyFileExW(
                source.as_raw_pcwstr(),
                target.as_raw_pcwstr(),
                None,
                None,
                None,
                COPY_FILE_COPY_SYMLINK | COPY_FILE_FAIL_IF_EXISTS,
            )
            .0
            .if_null_get_last_error()?;
        }
        Ok(())
    }

    /// Moves a file or directory within a volume or a file between volumes.
    ///
    /// - The operation is equivalent to a rename if the new path is on the same volume.
    /// - Only files can be moved between volumes, not directories.
    /// - Will move symlinks themselves, not their targets.
    /// - Symlinks can be moved within the same volume (renamed) without extended permission.
    /// - Will block until the operation is complete.
    /// - Will fail if the target path already exists.
    fn move_to<Q: AsRef<Path>>(&self, new_path: Q) -> io::Result<()> {
        let source = ZeroTerminatedWideString::from_os_str(self.as_ref().as_os_str());
        let target = ZeroTerminatedWideString::from_os_str(new_path.as_ref().as_os_str());
        unsafe {
            MoveFileExW(
                source.as_raw_pcwstr(),
                target.as_raw_pcwstr(),
                MOVEFILE_COPY_ALLOWED | MOVEFILE_WRITE_THROUGH,
            )
            .0
            .if_null_get_last_error()?;
        }
        Ok(())
    }
}

impl<T: AsRef<Path>> PathExt for T {}
