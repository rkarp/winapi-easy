//! Filesystem functionality.

use std::ffi::c_void;
use std::path::Path;
use std::{
    io,
    ptr,
};

use num_enum::IntoPrimitive;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    COPY_FILE_COPY_SYMLINK,
    COPY_FILE_FAIL_IF_EXISTS,
    COPYPROGRESSROUTINE_PROGRESS,
    CopyFileExW,
    LPPROGRESS_ROUTINE,
    LPPROGRESS_ROUTINE_CALLBACK_REASON,
    MOVEFILE_COPY_ALLOWED,
    MOVEFILE_WRITE_THROUGH,
    MoveFileWithProgressW,
    PROGRESS_CANCEL,
    PROGRESS_CONTINUE,
    PROGRESS_QUIET,
    PROGRESS_STOP,
};

use crate::internal::catch_unwind_and_abort;
use crate::string::{
    ZeroTerminatedWideString,
    max_path_extend,
};

/// Optional function called by Windows for every transferred chunk of a file.
///
/// This is used in [`PathExt::copy_file_to`] and [`PathExt::move_to`]
/// to receive progress notifications and to potentially pause or cancel the operation.
///
/// Use [`Default::default`] to disable.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct ProgressCallback<F>(Option<F>);

impl<F> ProgressCallback<F>
where
    F: FnMut(ProgressStatus) -> ProgressRetVal,
{
    pub fn new(value: F) -> Self {
        // No `From` impl since that has problems with type inference when declaring the closure
        ProgressCallback(Some(value))
    }

    fn typed_raw_progress_callback(&self) -> LPPROGRESS_ROUTINE {
        if self.0.is_some() {
            Some(transfer_internal_callback::<F> as _)
        } else {
            None
        }
    }

    fn as_raw_lpdata(&mut self) -> Option<*const c_void> {
        self.0
            .as_mut()
            .map(|callback| ptr::from_mut::<F>(callback).cast_const().cast::<c_void>())
    }
}

impl Default for ProgressCallback<fn(ProgressStatus) -> ProgressRetVal> {
    fn default() -> Self {
        Self(None)
    }
}

/// Progress status used in [`ProgressCallback`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ProgressStatus {
    /// Total size in bytes of the file being transferred.
    pub total_file_bytes: u64,
    /// Total bytes completed in the current file transfer.
    pub total_transferred_bytes: u64,
}

/// Return value used in [`ProgressCallback`] to control the ongoing file transfer.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(u32)]
pub enum ProgressRetVal {
    /// Continue the operation.
    #[default]
    Continue = PROGRESS_CONTINUE.0,
    /// Stop the operation with the option of continuing later.
    Stop = PROGRESS_STOP.0,
    /// Cancel the operation.
    Cancel = PROGRESS_CANCEL.0,
    /// Continue but stop calling the user callback.
    Quiet = PROGRESS_QUIET.0,
}

impl From<ProgressRetVal> for COPYPROGRESSROUTINE_PROGRESS {
    fn from(value: ProgressRetVal) -> Self {
        COPYPROGRESSROUTINE_PROGRESS(u32::from(value))
    }
}

/// Additional methods on [`Path`] using Windows-specific functionality.
pub trait PathExt: AsRef<Path> {
    /// Copies a file.
    ///
    /// - Will copy symlinks themselves, not their targets.
    /// - Will block until the operation is complete.
    /// - Will fail if the target path already exists.
    /// - Supports file names longer than `MAX_PATH` characters.
    ///
    /// Progress notifications can be enabled using a [`ProgressCallback`].
    /// Use [`Default::default`] to disable.
    fn copy_file_to<Q, F>(
        &self,
        new_path: Q,
        mut progress_callback: ProgressCallback<F>,
    ) -> io::Result<()>
    where
        Q: AsRef<Path>,
        F: FnMut(ProgressStatus) -> ProgressRetVal,
    {
        let source =
            ZeroTerminatedWideString::from_os_str(max_path_extend(self.as_ref().as_os_str()));
        let target =
            ZeroTerminatedWideString::from_os_str(max_path_extend(new_path.as_ref().as_os_str()));
        unsafe {
            CopyFileExW(
                source.as_raw_pcwstr(),
                target.as_raw_pcwstr(),
                progress_callback.typed_raw_progress_callback(),
                progress_callback.as_raw_lpdata(),
                None,
                COPY_FILE_COPY_SYMLINK | COPY_FILE_FAIL_IF_EXISTS,
            )?;
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
    /// - Supports file names longer than `MAX_PATH` characters.
    ///
    /// Progress notifications can be enabled using a [`ProgressCallback`].
    /// Use [`Default::default`] to disable.
    fn move_to<Q, F>(
        &self,
        new_path: Q,
        mut progress_callback: ProgressCallback<F>,
    ) -> io::Result<()>
    where
        Q: AsRef<Path>,
        F: FnMut(ProgressStatus) -> ProgressRetVal,
    {
        let source =
            ZeroTerminatedWideString::from_os_str(max_path_extend(self.as_ref().as_os_str()));
        let target =
            ZeroTerminatedWideString::from_os_str(max_path_extend(new_path.as_ref().as_os_str()));
        unsafe {
            MoveFileWithProgressW(
                source.as_raw_pcwstr(),
                target.as_raw_pcwstr(),
                progress_callback.typed_raw_progress_callback(),
                progress_callback.as_raw_lpdata(),
                MOVEFILE_COPY_ALLOWED | MOVEFILE_WRITE_THROUGH,
            )?;
        }
        Ok(())
    }
}

impl<T: AsRef<Path>> PathExt for T {}

unsafe extern "system" fn transfer_internal_callback<F>(
    totalfilesize: i64,
    totalbytestransferred: i64,
    _streamsize: i64,
    _streambytestransferred: i64,
    _dwstreamnumber: u32,
    _dwcallbackreason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _hsourcefile: HANDLE,
    _hdestinationfile: HANDLE,
    lpdata: *const c_void,
) -> COPYPROGRESSROUTINE_PROGRESS
where
    F: FnMut(ProgressStatus) -> ProgressRetVal,
{
    let call = move || {
        let user_callback: &mut F = unsafe { &mut *(lpdata.cast_mut().cast::<F>()) };
        user_callback(ProgressStatus {
            total_file_bytes: totalfilesize.try_into().unwrap_or_else(|_| unreachable!()),
            total_transferred_bytes: totalbytestransferred
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        })
    };
    catch_unwind_and_abort(call).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_transfer_internal_callback() -> io::Result<()> {
        let target_progress_status = ProgressStatus {
            total_file_bytes: 1,
            total_transferred_bytes: 1,
        };
        let progress_ret_val = ProgressRetVal::Stop;
        let mut progress_callback = ProgressCallback::new(|progress_status| {
            assert_eq!(progress_status, target_progress_status);
            progress_ret_val
        });
        let raw_progress_callback = progress_callback
            .typed_raw_progress_callback()
            .unwrap_or_else(|| unreachable!());
        let raw_call_result = unsafe {
            raw_progress_callback(
                target_progress_status
                    .total_file_bytes
                    .try_into()
                    .unwrap_or_else(|_| unreachable!()),
                target_progress_status
                    .total_transferred_bytes
                    .try_into()
                    .unwrap_or_else(|_| unreachable!()),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                progress_callback
                    .as_raw_lpdata()
                    .unwrap_or_else(|| unreachable!()),
            )
        };
        assert_eq!(raw_call_result, progress_ret_val.into());
        Ok(())
    }
}
