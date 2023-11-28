use std::convert::TryInto;
use std::io;
use std::io::ErrorKind;
use std::marker::PhantomData;
use std::mem;

use windows::core::PWSTR;
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu,
    DestroyMenu,
    GetMenuItemCount,
    GetMenuItemID,
    InsertMenuItemW,
    SetMenuInfo,
    TrackPopupMenu,
    HMENU,
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

use crate::internal::ReturnValue;
use crate::string::ToWideString;
use crate::ui::{
    Point,
    WindowHandle,
};

#[derive(Eq, PartialEq, Debug)]
pub(crate) struct MenuHandle {
    raw_handle: HMENU,
}

impl MenuHandle {
    fn new_popup_menu() -> io::Result<Self> {
        let handle = unsafe { CreatePopupMenu()?.if_null_get_last_error()? };
        let result = Self { raw_handle: handle };
        result.set_info()?;
        Ok(result)
    }

    #[allow(unused)]
    pub(crate) fn from_non_null(raw_handle: HMENU) -> Self {
        Self { raw_handle }
    }

    pub(crate) fn from_maybe_null(handle: HMENU) -> Option<Self> {
        if handle.0 != 0 {
            Some(Self { raw_handle: handle })
        } else {
            None
        }
    }

    fn set_info(&self) -> io::Result<()> {
        let raw_menu_info = MENUINFO {
            cbSize: mem::size_of::<MENUINFO>()
                .try_into()
                .expect("MENUINFO size conversion failed"),
            fMask: MIM_APPLYTOSUBMENUS | MIM_STYLE,
            dwStyle: MNS_NOTIFYBYPOS,
            cyMax: 0,
            hbrBack: None.into(),
            dwContextHelpID: 0,
            dwMenuData: 0,
        };
        unsafe {
            SetMenuInfo(self, &raw_menu_info).if_null_get_last_error()?;
        }
        Ok(())
    }

    fn insert_submenu_item(&self, idx: u32, item: SubMenuItem, id: u32) -> io::Result<()> {
        unsafe {
            InsertMenuItemW(
                self,
                idx,
                true,
                &SubMenuItemCallData::new(Some(&mut item.into()), Some(id)).item_info_struct,
            )
            .if_null_get_last_error()?;
        }
        Ok(())
    }

    pub(crate) fn get_item_id(&self, item_idx: u32) -> io::Result<u32> {
        let id = unsafe {
            GetMenuItemID(
                self,
                item_idx.try_into().map_err(|_err| {
                    io::Error::new(
                        ErrorKind::InvalidInput,
                        format!("Bad item index: {}", item_idx),
                    )
                })?,
            )
        };
        id.if_eq_to_error(-1i32 as u32, || ErrorKind::Other.into())?;
        Ok(id)
    }

    fn get_item_count(&self) -> io::Result<i32> {
        let count = unsafe { GetMenuItemCount(self) };
        count.if_eq_to_error(-1, io::Error::last_os_error)?;
        Ok(count)
    }

    fn destroy(&self) -> io::Result<()> {
        unsafe {
            DestroyMenu(self).if_null_get_last_error()?;
        }
        Ok(())
    }
}

impl From<&MenuHandle> for HMENU {
    fn from(value: &MenuHandle) -> Self {
        value.raw_handle
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
                &self.handle,
                Default::default(),
                coords.x,
                coords.y,
                0,
                window,
                None,
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

#[derive(Copy, Clone, Debug)]
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
            cbSize: mem::size_of::<MENUITEMINFOW>()
                .try_into()
                .expect("MENUITEMINFOW size conversion failed"),
            ..Default::default()
        };
        match &mut menu_item {
            Some(SubMenuItemRaw::WideText(ref mut wide_string)) => {
                item_info.fMask |= MIIM_STRING;
                item_info.cch = wide_string.len().try_into().unwrap();
                item_info.dwTypeData = PWSTR::from_raw(wide_string.as_mut_ptr());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_test_menu() -> io::Result<()> {
        let menu = PopupMenu::new()?;
        const TEST_ID: u32 = 42;
        menu.insert_menu_item(SubMenuItem::Text("Show window"), TEST_ID, None)?;
        menu.insert_menu_item(SubMenuItem::Separator, TEST_ID + 1, None)?;
        assert_eq!(menu.handle.get_item_count()?, 2);
        assert_eq!(menu.handle.get_item_id(0)?, TEST_ID);
        Ok(())
    }
}
