#![allow(unused_imports)]

use std::cell::RefCell;
use std::convert::TryInto;
use std::io;
use std::io::ErrorKind;
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use std::ptr::NonNull;

use winapi::shared::minwindef::TRUE;
use winapi::shared::windef::HMENU__;
use winapi::um::winuser::{
    CreatePopupMenu,
    DestroyMenu,
    GetMenuItemCount,
    GetMenuItemID,
    InsertMenuItemW,
    SetMenuInfo,
    TrackPopupMenu,
    MENUINFO,
    MENUITEMINFOW,
    MFT_SEPARATOR,
    MIIM_FTYPE,
    MIIM_ID,
    MIIM_STRING,
    MIM_APPLYTOSUBMENUS,
    MIM_STYLE,
    MNS_NOTIFYBYPOS,
};

use crate::internal::{
    custom_err_with_code,
    sync_closure_to_callback2,
    ManagedHandle,
    RawHandle,
    ReturnValue,
};
use crate::process::{
    ProcessId,
    ThreadId,
};
use crate::string::{
    to_wide_chars_iter,
    FromWideString,
    ToWideString,
};
use crate::ui::{
    Point,
    WindowHandle,
};

#[derive(Eq, PartialEq)]
pub struct MenuHandle {
    raw_handle: NonNull<HMENU__>,
}

impl MenuHandle {
    fn new_popup_menu() -> io::Result<Self> {
        let handle = unsafe { CreatePopupMenu().to_non_null_else_get_last_error()? };
        let result = Self { raw_handle: handle };
        result.set_info()?;
        Ok(result)
    }

    pub(crate) fn from_non_null(raw_handle: NonNull<HMENU__>) -> Self {
        Self { raw_handle }
    }

    fn set_info(&self) -> io::Result<()> {
        let raw_menu_info = MENUINFO {
            cbSize: mem::size_of::<MENUINFO>() as u32,
            fMask: MIM_APPLYTOSUBMENUS | MIM_STYLE,
            dwStyle: MNS_NOTIFYBYPOS,
            cyMax: 0,
            hbrBack: ptr::null_mut(),
            dwContextHelpID: 0,
            dwMenuData: 0,
        };
        unsafe {
            SetMenuInfo(self.as_immutable_ptr(), &raw_menu_info).if_null_get_last_error()?;
        }
        Ok(())
    }

    fn insert_submenu_item(&self, idx: u32, item: SubMenuItem, id: u32) -> io::Result<()> {
        unsafe {
            InsertMenuItemW(
                self.as_immutable_ptr(),
                idx,
                TRUE,
                &SubMenuItemCallData::new(Some(&mut item.into()), Some(id)).item_info_struct,
            )
            .if_null_get_last_error()?;
        }
        Ok(())
    }

    pub(crate) fn get_item_id(&self, item_idx: u32) -> io::Result<u32> {
        let id = unsafe { GetMenuItemID(self.as_immutable_ptr(), item_idx as i32) };
        id.if_eq_to_error(-1i32 as u32, || ErrorKind::Other.into())?;
        Ok(id)
    }

    fn get_item_count(&self) -> io::Result<i32> {
        let count = unsafe { GetMenuItemCount(self.as_immutable_ptr()) };
        count.if_eq_to_error(-1, || io::Error::last_os_error())?;
        Ok(count)
    }

    fn destroy(&mut self) -> io::Result<()> {
        unsafe {
            DestroyMenu(self.as_mutable_ptr()).if_null_get_last_error()?;
        }
        Ok(())
    }
}

impl ManagedHandle for MenuHandle {
    type Target = HMENU__;

    #[inline(always)]
    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.raw_handle.as_immutable_ptr()
    }
}

pub struct PopupMenu {
    handle: MenuHandle,
}

impl PopupMenu {
    pub fn new() -> io::Result<Self> {
        Ok(PopupMenu {
            handle: MenuHandle::new_popup_menu()?,
        })
    }

    pub fn insert_menu_item(&self, item: SubMenuItem, id: u32, idx: Option<u32>) -> io::Result<()> {
        let idx = match idx {
            Some(idx) => idx,
            None => self.handle.get_item_count()?.try_into().unwrap(),
        };
        self.handle.insert_submenu_item(idx, item, id)?;
        Ok(())
    }

    pub fn show_popup_menu(&self, window: &WindowHandle, coords: Point) -> io::Result<()> {
        unsafe {
            TrackPopupMenu(
                self.handle.as_immutable_ptr(),
                0,
                coords.x,
                coords.y,
                0,
                window.as_immutable_ptr(),
                ptr::null(),
            )
            .if_null_get_last_error()?;
        }
        Ok(())
    }
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        self.handle.destroy().unwrap()
    }
}

#[derive(Copy, Clone)]
pub enum SubMenuItem<'a> {
    Text(&'a str),
    Separator,
}

enum SubMenuItemRaw {
    WideText(Vec<u16>),
    Separator,
}

impl<'a> From<SubMenuItem<'a>> for SubMenuItemRaw {
    fn from(item: SubMenuItem<'a>) -> Self {
        match item {
            SubMenuItem::Text(text) => SubMenuItemRaw::WideText(text.to_wide_string()),
            SubMenuItem::Separator => SubMenuItemRaw::Separator,
        }
    }
}

struct SubMenuItemCallData<'a> {
    item_info_struct: MENUITEMINFOW,
    phantom: PhantomData<&'a SubMenuItemRaw>,
}

impl<'a> SubMenuItemCallData<'a> {
    fn new(mut menu_item: Option<&'a mut SubMenuItemRaw>, id: Option<u32>) -> Self {
        let mut item_info = MENUITEMINFOW {
            cbSize: mem::size_of::<MENUITEMINFOW>() as u32,
            ..Default::default()
        };
        match &mut menu_item {
            Some(SubMenuItemRaw::WideText(ref mut wide_string)) => {
                item_info.fMask |= MIIM_STRING;
                item_info.cch = wide_string.len().try_into().unwrap();
                item_info.dwTypeData = wide_string.as_mut_ptr();
            }
            Some(SubMenuItemRaw::Separator) => {
                item_info.fMask |= MIIM_FTYPE;
                item_info.fType |= MFT_SEPARATOR;
            }
            None => (),
        }
        if let Some(id) = id {
            item_info.fMask |= MIIM_ID;
            item_info.wID = id;
        }
        Self {
            item_info_struct: item_info,
            phantom: PhantomData,
        }
    }
}
