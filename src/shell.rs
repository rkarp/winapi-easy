//! Windows Shell functionality.

use std::cell::Cell;
use std::ops::{
    BitOr,
    BitOrAssign,
};
use std::path::{
    Path,
    PathBuf,
};
use std::rc::Rc;
use std::{
    io,
    ptr,
};

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use windows::Win32::Foundation::{
    HANDLE,
    HWND,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    ILCreateFromPathW,
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
};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::com::ComTaskMemory;
use crate::internal::{
    CustomAutoDrop,
    ReturnValue,
};
use crate::messaging::ThreadMessageLoop;
use crate::string::{
    ZeroTerminatedWideString,
    max_path_extend,
};
use crate::ui::messaging::{
    ListenerAnswer,
    ListenerMessage,
    ListenerMessageVariant,
};
use crate::ui::window::{
    Window,
    WindowClass,
    WindowClassAppearance,
};

#[expect(dead_code)]
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
        *self = *self | rhs;
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

#[expect(dead_code)]
pub(crate) fn monitor_path_changes<F>(
    monitored_paths: &[MonitoredPath],
    event_type: FsChangeEvent,
    mut callback: F,
) -> io::Result<()>
where
    F: FnMut(&PathChangeEvent) -> io::Result<()>,
{
    let listener_data: Rc<Cell<PathChangeEvent>> = Cell::default().into();
    let listener_data_clone = listener_data.clone();
    let listener = move |message: &ListenerMessage| {
        if let ListenerMessageVariant::CustomUserMessage(custom_message) = message.variant {
            fn get_path_from_id_list(raw_id_list: ITEMIDLIST) -> PathBuf {
                let mut raw_path_buffer = vec![0; 32000];
                unsafe {
                    // Unclear if paths > MAX_PATH are even supported here
                    SHGetPathFromIDListEx(
                        &raw const raw_id_list,
                        raw_path_buffer.as_mut_slice(),
                        Default::default(),
                    )
                    .if_null_panic_else_drop("Cannot get path from ID list");
                }
                let wide_string = unsafe { ZeroTerminatedWideString::from_raw(raw_path_buffer) };
                wide_string.to_os_string().into()
            }

            // Should be `WM_APP`
            assert_eq!(custom_message.message_id, 0);
            // See: https://stackoverflow.com/a/72001352
            let mut raw_ppp_idl = ptr::null_mut();
            let mut raw_event = 0;
            let lock = unsafe {
                SHChangeNotification_Lock(
                    HANDLE(ptr::with_exposed_provenance_mut(custom_message.w_param)),
                    custom_message
                        .l_param
                        .try_into()
                        .unwrap_or_else(|_| unreachable!()),
                    Some(&raw mut raw_ppp_idl),
                    Some(&raw mut raw_event),
                )
            };
            let _unlock_guard = CustomAutoDrop {
                value: lock,
                drop_fn: |x| unsafe {
                    SHChangeNotification_Unlock(HANDLE(x.0))
                        .if_null_panic_else_drop("Improper lock usage");
                },
            };

            let raw_pid_list_pair = unsafe { std::slice::from_raw_parts(raw_ppp_idl, 2) };
            let event_type = FsChangeEvent::from(raw_event.cast_unsigned());

            let path_1 =
                unsafe { raw_pid_list_pair[0].as_ref() }.map(|x| get_path_from_id_list(*x));
            let path_2 =
                unsafe { raw_pid_list_pair[1].as_ref() }.map(|x| get_path_from_id_list(*x));
            listener_data.replace(PathChangeEvent {
                event: event_type,
                path_1,
                path_2,
            });
            ListenerAnswer::StopMessageProcessing
        } else {
            ListenerAnswer::default()
        }
    };

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

    let window_class = WindowClass::register_new(
        "Shell Change Listener Class",
        WindowClassAppearance::empty(),
    )?;
    let window = Window::new::<_, ()>(
        window_class.into(),
        Some(listener),
        "Shell Change Listener",
        Default::default(),
        None,
    )?;
    let reg_id = unsafe {
        SHChangeNotifyRegister(
            HWND::from(window.as_handle()),
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
                .if_null_panic_else_drop("Notification listener not registered properly");
        },
    };

    ThreadMessageLoop::new().run_with::<io::Error, _>(|_| callback(&listener_data_clone.take()))?;
    Ok(())
}

/// Forces a refresh of the Windows icon cache.
pub fn refresh_icon_cache() {
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}
