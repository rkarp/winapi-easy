//! Windows Shell functionality.

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    ILCreateFromPathW,
    SHCNRF_InterruptLevel,
    SHCNRF_NewDelivery,
    SHCNRF_RecursiveInterrupt,
    SHCNRF_ShellLevel,
    SHChangeNotification_Lock,
    SHChangeNotification_Unlock,
    SHChangeNotify,
    SHChangeNotifyDeregister,
    SHChangeNotifyEntry,
    SHChangeNotifyRegister,
    SHGetPathFromIDListEx,
    SHCNE_ASSOCCHANGED,
    SHCNE_CREATE,
    SHCNE_DELETE,
    SHCNE_ID,
    SHCNE_MKDIR,
    SHCNE_RENAMEFOLDER,
    SHCNE_RENAMEITEM,
    SHCNE_RMDIR,
    SHCNE_UPDATEDIR,
    SHCNE_UPDATEITEM,
    SHCNF_IDLIST,
};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use std::cell::Cell;
use std::ops::{
    BitOr,
    BitOrAssign,
};
use std::path::{
    Path,
    PathBuf,
};
use std::{
    io,
    ptr,
};
use windows::Win32::Foundation::{
    HANDLE,
    HWND,
    LPARAM,
    WPARAM,
};

use crate::com::ComTaskMemory;
use crate::internal::{
    CustomAutoDrop,
    ReturnValue,
};
use crate::messaging::ThreadMessageLoop;
use crate::string::{
    max_path_extend,
    ZeroTerminatedWideString,
};
use crate::ui::messaging::WindowMessageListener;
use crate::ui::{
    Window,
    WindowClass,
    WindowClassAppearance,
    WindowHandle,
};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct PathChangeEvent {
    pub event: FsChangeEvent,
    pub path_1: Option<PathBuf>,
    pub path_2: Option<PathBuf>,
}

impl Default for PathChangeEvent {
    fn default() -> Self {
        Self {
            event: FsChangeEvent::Other(0),
            path_1: None,
            path_2: None,
        }
    }
}

#[derive(IntoPrimitive, FromPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub(crate) enum FsChangeEvent {
    ItemCreated = SHCNE_CREATE.0,
    ItemRenamed = SHCNE_RENAMEITEM.0,
    ItemUpdated = SHCNE_UPDATEITEM.0,
    ItemDeleted = SHCNE_DELETE.0,
    FolderCreated = SHCNE_MKDIR.0,
    FolderRenamed = SHCNE_RENAMEFOLDER.0,
    FolderUpdated = SHCNE_UPDATEDIR.0,
    FolderDeleted = SHCNE_RMDIR.0,
    #[num_enum(catch_all)]
    Other(u32),
}

impl BitOr for FsChangeEvent {
    type Output = FsChangeEvent;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::from(u32::from(self) | u32::from(rhs))
    }
}

impl BitOrAssign for FsChangeEvent {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs
    }
}

impl From<FsChangeEvent> for SHCNE_ID {
    fn from(value: FsChangeEvent) -> Self {
        SHCNE_ID(value.into())
    }
}

pub(crate) struct MonitoredPath<'a> {
    pub path: &'a Path,
    pub recursive: bool,
}

#[allow(dead_code)]
pub(crate) fn monitor_path_changes<F>(
    monitored_paths: &[MonitoredPath],
    event_type: FsChangeEvent,
    mut callback: F,
) -> io::Result<()>
where
    F: FnMut(&PathChangeEvent) -> io::Result<()>,
{
    #[derive(Default)]
    struct Listener {
        data: Cell<PathChangeEvent>,
    }
    impl WindowMessageListener for Listener {
        fn handle_custom_user_message(
            &self,
            _window: &WindowHandle,
            message_id: u8,
            w_param: WPARAM,
            l_param: LPARAM,
        ) {
            assert_eq!(message_id, 0);
            // See: https://stackoverflow.com/a/72001352
            let mut raw_ppp_idl = ptr::null_mut();
            let mut raw_event = 0;
            let lock = unsafe {
                SHChangeNotification_Lock(
                    HANDLE(w_param.0 as isize),
                    l_param.0 as u32,
                    Some(&mut raw_ppp_idl),
                    Some(&mut raw_event),
                )
            };
            let _unlock_guard = CustomAutoDrop {
                value: lock,
                drop_fn: |x| unsafe {
                    SHChangeNotification_Unlock(HANDLE(x.0)).if_null_panic("Improper lock usage");
                },
            };

            let raw_pid_list_pair = unsafe { std::slice::from_raw_parts(raw_ppp_idl, 2) };
            let event_type = FsChangeEvent::from(raw_event as u32);

            fn get_path_from_id_list(raw_id_list: &ITEMIDLIST) -> PathBuf {
                let mut raw_path_buffer: ZeroTerminatedWideString =
                    ZeroTerminatedWideString(vec![0; 32000]);
                unsafe {
                    // Unclear if paths > MAX_PATH are even supported here
                    SHGetPathFromIDListEx(
                        raw_id_list,
                        raw_path_buffer.0.as_mut_slice(),
                        Default::default(),
                    )
                    .if_null_panic("Cannot get path from ID list");
                }
                raw_path_buffer.to_os_string().into()
            }

            let path_1 = unsafe { raw_pid_list_pair[0].as_ref() }.map(get_path_from_id_list);
            let path_2 = unsafe { raw_pid_list_pair[1].as_ref() }.map(get_path_from_id_list);
            self.data.replace(PathChangeEvent {
                event: event_type,
                path_1,
                path_2,
            });
        }
    }

    // Unclear if it works if only some items are recursive
    let recursive = monitored_paths.iter().any(|x| x.recursive);
    let path_id_lists: Vec<(SHChangeNotifyEntry, ComTaskMemory<_>)> = monitored_paths
        .iter()
        .map(|monitored_path| {
            let path_as_id_list: ComTaskMemory<_> = unsafe {
                // MAX_PATH extension seems possible: https://stackoverflow.com/questions/9980943/bypassing-max-path-limitation-for-itemidlist#comment12771197_9980943
                ILCreateFromPathW(
                    ZeroTerminatedWideString::from_os_str(max_path_extend(
                        monitored_path.path.as_os_str(),
                    ))
                    .as_raw_pcwstr(),
                )
                .into()
            };
            let raw_entry = SHChangeNotifyEntry {
                pidl: path_as_id_list.0,
                fRecursive: monitored_path.recursive.into(),
            };
            (raw_entry, path_as_id_list)
        })
        .collect();
    let raw_entries: Vec<SHChangeNotifyEntry> = path_id_lists.iter().map(|x| x.0).collect();

    let listener = Listener::default();

    let window_class = WindowClass::register_new(
        "Shell Change Listener Class",
        WindowClassAppearance::empty(),
    )?;
    let window = Window::create_new(&window_class, &listener, "Shell Change Listener")?;
    let reg_id = unsafe {
        SHChangeNotifyRegister(
            HWND::from(window.as_ref()),
            SHCNRF_InterruptLevel
                | SHCNRF_ShellLevel
                | SHCNRF_NewDelivery
                | if recursive {
                    SHCNRF_RecursiveInterrupt
                } else {
                    Default::default()
                },
            u32::from(event_type).try_into().unwrap(),
            WM_APP,
            raw_entries.len().try_into().unwrap(),
            raw_entries.as_ptr(),
        )
        .if_null_get_last_error()?
    };
    let _deregister_guard = CustomAutoDrop {
        value: reg_id,
        drop_fn: |x| unsafe {
            SHChangeNotifyDeregister(*x)
                .if_null_panic("Notification listener not registered properly");
        },
    };

    ThreadMessageLoop::run_thread_message_loop(|| callback(&listener.data.take()))?;
    Ok(())
}

/// Forces a refresh of the Windows icon cache.
pub fn refresh_icon_cache() {
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}
