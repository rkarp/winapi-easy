//! Menus and menu items.

use std::cell::RefCell;
use std::io::ErrorKind;
use std::marker::PhantomData;
use std::rc::Rc;
use std::{
    io,
    mem,
};

use windows::Win32::UI::WindowsAndMessaging::{
    CreateMenu,
    CreatePopupMenu,
    DestroyMenu,
    GetMenuItemCount,
    GetMenuItemID,
    HMENU,
    InsertMenuItemW,
    IsMenu,
    MENUINFO,
    MENUITEMINFOW,
    MF_BYPOSITION,
    MFS_CHECKED,
    MFS_DISABLED,
    MFT_RADIOCHECK,
    MFT_SEPARATOR,
    MFT_STRING,
    MIIM_FTYPE,
    MIIM_ID,
    MIIM_STATE,
    MIIM_STRING,
    MIIM_SUBMENU,
    MIM_STYLE,
    MNS_NOTIFYBYPOS,
    RemoveMenu,
    SetMenuInfo,
    SetMenuItemInfoW,
    TrackPopupMenu,
};

use crate::internal::ReturnValue;
#[rustversion::before(1.87)]
use crate::internal::std_unstable::CastUnsigned;
use crate::string::ZeroTerminatedWideString;
use crate::ui::{
    Point,
    WindowHandle,
};

#[cfg(test)]
static_assertions::assert_not_impl_any!(MenuHandle: Send, Sync);

#[derive(Eq, PartialEq, Debug)]
pub(crate) struct MenuHandle {
    raw_handle: HMENU,
    marker: PhantomData<*mut ()>,
}

impl MenuHandle {
    #[allow(dead_code)]
    fn new_menu() -> io::Result<Self> {
        let handle = unsafe { CreateMenu()?.if_null_get_last_error()? };
        let result = Self {
            raw_handle: handle,
            marker: PhantomData,
        };
        result.set_notify_by_pos()?;
        Ok(result)
    }

    fn new_submenu() -> io::Result<Self> {
        let handle = unsafe { CreatePopupMenu()?.if_null_get_last_error()? };
        let result = Self {
            raw_handle: handle,
            marker: PhantomData,
        };
        result.set_notify_by_pos()?;
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
        if handle.is_null() {
            None
        } else {
            Some(Self {
                raw_handle: handle,
                marker: PhantomData,
            })
        }
    }

    /// Sets the menu to send `WMWM_MENUCOMMAND` instead of `WM_COMMAND` messages.
    ///
    /// According to docs: This is a menu header style and has no effect when applied to individual sub menus.
    fn set_notify_by_pos(&self) -> io::Result<()> {
        let raw_menu_info = MENUINFO {
            cbSize: mem::size_of::<MENUINFO>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            fMask: MIM_STYLE,
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

    fn insert_submenu_item(&self, item: &SubMenuItem, idx: u32) -> io::Result<()> {
        let insert_call = |raw_item_info| {
            unsafe {
                InsertMenuItemW(self.raw_handle, idx, true, &raw_item_info)?;
            }
            Ok(())
        };
        item.call_with_raw_menu_info(insert_call)
    }

    fn modify_submenu_item(&self, item: &SubMenuItem, idx: u32) -> io::Result<()> {
        let insert_call = |raw_item_info| {
            unsafe {
                SetMenuItemInfoW(self.raw_handle, idx, true, &raw_item_info)?;
            }
            Ok(())
        };
        item.call_with_raw_menu_info(insert_call)
    }

    /// Removes an item.
    ///
    /// If the item contains a submenu, the submenu itself is preserved.
    fn remove_item(&self, idx: u32) -> io::Result<()> {
        unsafe {
            RemoveMenu(self.raw_handle, idx, MF_BYPOSITION)?;
        }
        Ok(())
    }

    pub(crate) fn get_item_id(&self, item_idx: u32) -> io::Result<u32> {
        let id = unsafe { GetMenuItemID(self.raw_handle, item_idx.cast_signed()) };
        id.if_eq_to_error((-1i32).cast_unsigned(), || ErrorKind::Other.into())?;
        Ok(id)
    }

    fn get_item_count(&self) -> io::Result<i32> {
        let count = unsafe { GetMenuItemCount(Some(self.raw_handle)) };
        count.if_eq_to_error(-1, io::Error::last_os_error)?;
        Ok(count)
    }

    #[allow(dead_code)]
    fn is_menu(&self) -> bool {
        unsafe { IsMenu(self.raw_handle).as_bool() }
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

#[cfg(any())]
#[cfg(test)]
static_assertions::assert_not_impl_any!(Menu: Send, Sync);

#[cfg(any())]
#[derive(Debug)]
pub struct Menu {
    handle: MenuHandle,
    items: Vec<TextMenuItem>,
}

#[cfg(any())]
impl Menu {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            handle: MenuHandle::new_menu()?,
            items: Vec::new(),
        })
    }
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(SubMenu: Send, Sync);

/// A popup menu for use with [`crate::ui::window::NotificationIcon`].
#[derive(Debug)]
pub struct SubMenu {
    handle: MenuHandle,
    items: Vec<SubMenuItem>,
}

impl SubMenu {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            handle: MenuHandle::new_submenu()?,
            items: Vec::new(),
        })
    }

    /// Inserts a menu item before the item with the given index.
    ///
    /// If no index is given, it will be inserted after the last item.
    ///
    /// # Panics
    ///
    /// Will panic if the given index is greater than the current amount of items.
    pub fn insert_menu_item(&mut self, item: SubMenuItem, index: Option<u32>) -> io::Result<()> {
        let handle_item_count: u32 = self
            .handle
            .get_item_count()?
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(handle_item_count, self.items.len().try_into().unwrap());
        let idx = match index {
            Some(idx) => idx,
            None => handle_item_count,
        };
        self.handle.insert_submenu_item(&item, idx)?;
        self.items.insert(idx.try_into().unwrap(), item);
        Ok(())
    }

    /// Modifies a menu item using the given closure.
    ///
    /// # Panics
    ///
    /// Will panic if the given index is out of bounds.
    pub fn modify_menu_item(
        &mut self,
        index: u32,
        modify_fn: impl FnOnce(&mut SubMenuItem) -> io::Result<()>,
    ) -> io::Result<()> {
        let item = &mut self.items[usize::try_from(index).unwrap()];
        modify_fn(item)?;
        self.handle.modify_submenu_item(item, index)?;
        Ok(())
    }

    /// Removes a menu item.
    ///
    /// # Panics
    ///
    /// Will panic if the given index is out of bounds.
    pub fn remove_menu_item(&mut self, index: u32) -> io::Result<()> {
        let index_usize = usize::try_from(index).unwrap();
        assert!(index_usize < self.items.len());
        self.handle.remove_item(index)?;
        let _ = self.items.remove(index_usize);
        Ok(())
    }

    /// Shows the popup menu at the given coordinates.
    ///
    /// The coordinates can for example be retrieved from the window message handler, see
    /// [`crate::ui::messaging::WindowMessageListener::handle_notification_icon_context_select`]
    pub fn show_menu(&self, window: WindowHandle, coords: Point) -> io::Result<()> {
        unsafe {
            TrackPopupMenu(
                self.handle.raw_handle,
                Default::default(),
                coords.x,
                coords.y,
                None,
                window.into(),
                None,
            )
            .if_null_get_last_error_else_drop()?;
        }
        Ok(())
    }
}

impl Drop for SubMenu {
    fn drop(&mut self) {
        let size_u32 = u32::try_from(self.items.len()).unwrap();
        // Remove all items first to avoid submenus getting destroyed by `DestroyMenu`
        for index in (0..size_u32).rev() {
            self.remove_menu_item(index).unwrap();
        }
        self.handle.destroy().unwrap();
    }
}

/// A submenu item.
///
/// Can be added with [`SubMenu::insert_menu_item`].
#[derive(Clone, Debug)]
pub enum SubMenuItem {
    Text(TextMenuItem),
    Separator,
}

impl SubMenuItem {
    fn call_with_raw_menu_info<O>(&self, call: impl FnOnce(MENUITEMINFOW) -> O) -> O {
        match self {
            SubMenuItem::Text(text_item) => text_item.call_with_raw_menu_info(call),
            SubMenuItem::Separator => {
                let mut item_info = default_raw_item_info();
                item_info.fMask |= MIIM_FTYPE;
                item_info.fType |= MFT_SEPARATOR;
                call(item_info)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextMenuItem {
    pub id: u32,
    pub text: String,
    pub disabled: bool,
    pub item_symbol: Option<ItemSymbol>,
    pub sub_menu: Option<Rc<RefCell<SubMenu>>>,
}

impl TextMenuItem {
    pub fn default_with_text(id: u32, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            disabled: false,
            item_symbol: None,
            sub_menu: None,
        }
    }

    fn call_with_raw_menu_info<O>(&self, call: impl FnOnce(MENUITEMINFOW) -> O) -> O {
        // Must outlive the `MENUITEMINFOW` struct
        let mut text_wide_string = ZeroTerminatedWideString::from_os_str(&self.text);
        let mut item_info = default_raw_item_info();
        item_info.fMask |= MIIM_FTYPE | MIIM_STATE | MIIM_ID | MIIM_SUBMENU | MIIM_STRING;
        item_info.fType |= MFT_STRING;
        item_info.cch = text_wide_string.0.len().try_into().unwrap();
        item_info.dwTypeData = text_wide_string.as_raw_pwstr();
        if self.disabled {
            item_info.fState |= MFS_DISABLED;
        }
        if let Some(checkmark) = self.item_symbol {
            item_info.fState |= MFS_CHECKED;
            match checkmark {
                ItemSymbol::CheckMark => (),
                ItemSymbol::RadioButton => item_info.fType |= MFT_RADIOCHECK,
            }
        }
        // `MFS_HILITE` highlights an item as if selected, but only once, and has no further effects, so we skip it.

        item_info.wID = self.id;
        if let Some(submenu) = &self.sub_menu {
            item_info.hSubMenu = submenu.borrow().handle.raw_handle;
        }
        call(item_info)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ItemSymbol {
    #[default]
    CheckMark,
    RadioButton,
}

fn default_raw_item_info() -> MENUITEMINFOW {
    MENUITEMINFOW {
        cbSize: mem::size_of::<MENUITEMINFOW>()
            .try_into()
            .unwrap_or_else(|_| unreachable!()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_test_menu() -> io::Result<()> {
        let mut menu = SubMenu::new()?;
        const TEST_ID: u32 = 42;
        const TEST_ID2: u32 = 43;
        menu.insert_menu_item(
            SubMenuItem::Text(TextMenuItem::default_with_text(TEST_ID, "text")),
            None,
        )?;
        menu.insert_menu_item(SubMenuItem::Separator, None)?;
        menu.modify_menu_item(0, |item| {
            if let SubMenuItem::Text(item) = item {
                item.disabled = true;
                Ok(())
            } else {
                panic!()
            }
        })?;
        menu.modify_menu_item(1, |item| {
            *item = SubMenuItem::Text(TextMenuItem::default_with_text(TEST_ID2, "text2"));
            Ok(())
        })?;
        let mut submenu = SubMenu::new()?;
        submenu.insert_menu_item(SubMenuItem::Separator, None)?;
        let submenu = Rc::new(RefCell::new(submenu));
        {
            let mut menu2 = SubMenu::new()?;
            menu2.insert_menu_item(
                SubMenuItem::Text(TextMenuItem {
                    sub_menu: Some(submenu.clone()),
                    ..TextMenuItem::default_with_text(0, "")
                }),
                None,
            )?;
        }
        menu.insert_menu_item(
            SubMenuItem::Text(TextMenuItem {
                sub_menu: Some(submenu),
                ..TextMenuItem::default_with_text(0, "Submenu")
            }),
            None,
        )?;
        assert_eq!(menu.handle.get_item_count()?, 3);
        assert_eq!(menu.handle.get_item_id(0)?, TEST_ID);
        assert_eq!(menu.handle.get_item_id(1)?, TEST_ID2);
        Ok(())
    }
}
