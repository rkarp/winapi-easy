//! Menus and menu items.

use std::{
    io,
    mem,
};
use std::io::ErrorKind;
use std::marker::PhantomData;

use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu,
    DestroyMenu,
    GetMenuItemCount,
    GetMenuItemID,
    HMENU,
    InsertMenuItemW,
    MENUINFO,
    MENUITEMINFOW,
    MFT_SEPARATOR,
    MIIM_FTYPE,
    MIIM_ID,
    MIIM_STRING,
    MIM_APPLYTOSUBMENUS,
    MIM_STYLE,
    MNS_NOTIFYBYPOS,
    SetMenuInfo,
    TrackPopupMenu,
};
use windows::core::PWSTR;

use crate::internal::ReturnValue;
use crate::string::ToWideString;
use crate::ui::{
    Point,
    WindowHandle,
};

#[derive(Eq, PartialEq, Debug)]
pub(crate) struct MenuHandle {
    raw_handle: HMENU,
    marker: PhantomData<*mut ()>,
}

impl MenuHandle {
    fn new_popup_menu() -> io::Result<Self> {
        let handle = unsafe { CreatePopupMenu()?.if_null_get_last_error()? };
        let result = Self {
            raw_handle: handle,
            marker: PhantomData,
        };
        result.set_info()?;
        Ok(result)
    }

    #[allow(dead_code)]
    pub(crate) fn from_non_null(raw_handle: HMENU) -> Self {
        Self {
            raw_handle,
            marker: PhantomData,
        }
    }

    pub(crate) fn from_maybe_null(handle: HMENU) -> Option<Self> {
        if !handle.is_null() {
            Some(Self {
                raw_handle: handle,
                marker: PhantomData,
            })
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
            hbrBack: Default::default(),
            dwContextHelpID: 0,
            dwMenuData: 0,
        };
        unsafe {
            SetMenuInfo(self.raw_handle, &raw_menu_info)?;
        }
        Ok(())
    }

    fn insert_submenu_item(&self, idx: u32, item: MenuItem, id: u32) -> io::Result<()> {
        unsafe {
            InsertMenuItemW(
                self.raw_handle,
                idx,
                true,
                &MenuItemCallData::new(Some(&mut item.into()), Some(id)).item_info_struct,
            )?;
        }
        Ok(())
    }

    pub(crate) fn get_item_id(&self, item_idx: u32) -> io::Result<u32> {
        let id = unsafe {
            GetMenuItemID(
                self.raw_handle,
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
        let count = unsafe { GetMenuItemCount(Some(self.raw_handle)) };
        count.if_eq_to_error(-1, io::Error::last_os_error)?;
        Ok(count)
    }

    fn destroy(&self) -> io::Result<()> {
        unsafe {
            DestroyMenu(self.raw_handle)?;
        }
        Ok(())
    }
}

impl From<MenuHandle> for HMENU {
    fn from(value: MenuHandle) -> Self {
        value.raw_handle
    }
}

impl From<&MenuHandle> for HMENU {
    fn from(value: &MenuHandle) -> Self {
        value.raw_handle
    }
}

/// A popup menu for use with [`crate::ui::NotificationIcon`].
#[derive(Debug)]
pub struct PopupMenu {
    handle: MenuHandle,
}

impl PopupMenu {
    pub fn new() -> io::Result<Self> {
        Ok(PopupMenu {
            handle: MenuHandle::new_popup_menu()?,
        })
    }

    /// Inserts a menu item.
    ///
    /// If no index is given, it will be inserted after the last item.
    pub fn insert_menu_item(&self, item: MenuItem, id: u32, index: Option<u32>) -> io::Result<()> {
        let idx = match index {
            Some(idx) => idx,
            None => self.handle.get_item_count()?.try_into().unwrap(),
        };
        self.handle.insert_submenu_item(idx, item, id)?;
        Ok(())
    }

    /// Shows the popup menu at the given coordinates.
    ///
    /// The coordinates can for example be retrieved from the window message handler, see
    /// [crate::ui::messaging::WindowMessageListener::handle_notification_icon_context_select]
    pub fn show_popup_menu(&self, window: &WindowHandle, coords: Point) -> io::Result<()> {
        unsafe {
            TrackPopupMenu(
                self.handle.raw_handle,
                Default::default(),
                coords.x,
                coords.y,
                None,
                window.raw_handle,
                None,
            )
            .if_null_get_last_error_else_drop()?;
        }
        Ok(())
    }
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        self.handle.destroy().unwrap()
    }
}

/// A menu item.
///
/// Can be added with [`PopupMenu::insert_menu_item`].
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MenuItem<'a> {
    Text(&'a str),
    Separator,
}

enum MenuItemRaw {
    WideText(Vec<u16>),
    Separator,
}

impl<'a> From<MenuItem<'a>> for MenuItemRaw {
    fn from(item: MenuItem<'a>) -> Self {
        match item {
            MenuItem::Text(text) => MenuItemRaw::WideText(text.to_wide_string()),
            MenuItem::Separator => MenuItemRaw::Separator,
        }
    }
}

struct MenuItemCallData<'a> {
    item_info_struct: MENUITEMINFOW,
    phantom: PhantomData<&'a MenuItemRaw>,
}

impl<'a> MenuItemCallData<'a> {
    fn new(mut menu_item: Option<&'a mut MenuItemRaw>, id: Option<u32>) -> Self {
        let mut item_info = MENUITEMINFOW {
            cbSize: mem::size_of::<MENUITEMINFOW>()
                .try_into()
                .expect("MENUITEMINFOW size conversion failed"),
            ..Default::default()
        };
        match &mut menu_item {
            Some(MenuItemRaw::WideText(wide_string)) => {
                item_info.fMask |= MIIM_STRING;
                item_info.cch = wide_string.len().try_into().unwrap();
                item_info.dwTypeData = PWSTR::from_raw(wide_string.as_mut_ptr());
            }
            Some(MenuItemRaw::Separator) => {
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
        menu.insert_menu_item(MenuItem::Text("Show window"), TEST_ID, None)?;
        menu.insert_menu_item(MenuItem::Separator, TEST_ID + 1, None)?;
        assert_eq!(menu.handle.get_item_count()?, 2);
        assert_eq!(menu.handle.get_item_id(0)?, TEST_ID);
        Ok(())
    }
}
