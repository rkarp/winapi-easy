//! Global hotkeys.

use std::collections::HashMap;
use std::io;
use std::marker::PhantomData;
use std::ops::Add;

use num_enum::IntoPrimitive;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS,
    MOD_ALT,
    MOD_CONTROL,
    MOD_NOREPEAT,
    MOD_SHIFT,
    MOD_WIN,
    RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW,
    MSG,
    WM_HOTKEY,
};
use windows::core::BOOL;

use crate::input::KeyboardKey;

/// A group of global hotkeys that can be listened for.
///
/// # Examples
///
/// ```no_run
/// use winapi_easy::input::hotkeys::{GlobalHotkeySet, Modifier};
/// use winapi_easy::input::KeyboardKey;
///
/// #[derive(Copy, Clone)]
/// enum MyAction {
///     One,
///     Two,
/// }
///
/// let hotkeys = GlobalHotkeySet::new()
///     .add_hotkey(MyAction::One, Modifier::Ctrl + Modifier::Alt + KeyboardKey::A)
///     .add_hotkey(MyAction::Two, Modifier::Shift + Modifier::Alt + KeyboardKey::B);
///
/// for action in hotkeys.listen_for_hotkeys()? {
///     match action? {
///         MyAction::One => println!("One!"),
///         MyAction::Two => println!("Two!"),
///     }
/// }
///
/// # Result::<(), std::io::Error>::Ok(())
/// ```
#[derive(Clone, Debug)]
pub struct GlobalHotkeySet<ID> {
    hotkey_defs: Vec<HotkeyDef<ID>>,
}

impl<ID> GlobalHotkeySet<ID> {
    const MIN_ID: i32 = 1;
}

impl<ID> GlobalHotkeySet<ID>
where
    ID: 'static + Copy + Send + Sync,
{
    pub fn new() -> Self {
        Default::default()
    }

    /// Adds a hotkey.
    ///
    /// This does not register the hotkey combination with Windows yet.
    ///
    /// Not all key combinations may work as hotkeys.
    #[must_use]
    pub fn add_hotkey<KC>(mut self, id: ID, key_combination: KC) -> Self
    where
        KC: Into<KeyCombination>,
    {
        let new_def = HotkeyDef {
            user_id: id,
            key_combination: key_combination.into(),
        };
        self.hotkey_defs.push(new_def);
        self
    }

    pub fn listen_for_hotkeys(self) -> io::Result<impl IntoIterator<Item = io::Result<ID>>> {
        GlobalHotkeyIter::new(self)
    }
}

impl<ID> Default for GlobalHotkeySet<ID> {
    fn default() -> Self {
        Self {
            hotkey_defs: Vec::new(),
        }
    }
}

/// Registers global hotkeys and then yields hotkey events.
///
/// # Multithreading
///
/// This iterator is not [`Send`] and [`Sync`] because the hotkeys are registered only to the current thread.
#[derive(Clone, Debug)]
pub struct GlobalHotkeyIter<ID> {
    hotkeys: GlobalHotkeySet<ID>,
    id_assocs: HashMap<i32, ID>,
    _marker: PhantomData<*mut ()>,
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(GlobalHotkeyIter<u64>: Send, Sync);

impl<ID> GlobalHotkeyIter<ID>
where
    ID: 'static + Copy + Send + Sync,
{
    /// Registers the given [`GlobalHotkeySet`] with the system.
    pub fn new(hotkeys: GlobalHotkeySet<ID>) -> io::Result<Self> {
        let ids = || GlobalHotkeySet::<ID>::MIN_ID..;
        ids()
            .zip(&hotkeys.hotkey_defs)
            .try_for_each(|(curr_id, hotkey_def)| {
                let result: io::Result<()> = unsafe {
                    RegisterHotKey(
                        None,
                        curr_id,
                        HOT_KEY_MODIFIERS(hotkey_def.key_combination.modifiers.0),
                        hotkey_def.key_combination.key.into(),
                    )
                    .map_err(From::from)
                };
                if result.is_err() {
                    for id in (GlobalHotkeySet::<ID>::MIN_ID..curr_id).rev() {
                        unsafe {
                            let _ = UnregisterHotKey(None, id);
                        }
                    }
                }
                result
            })?;
        let id_assocs: HashMap<i32, ID> = ids()
            .zip(hotkeys.hotkey_defs.iter().map(|def| def.user_id))
            .collect();
        Ok(Self {
            hotkeys,
            id_assocs,
            _marker: PhantomData,
        })
    }
}

impl<ID> Iterator for GlobalHotkeyIter<ID>
where
    ID: 'static + Copy + Send + Sync,
{
    type Item = io::Result<ID>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut message: MSG = Default::default();
        let getmsg_result = unsafe { GetMessageW(&mut message, None, WM_HOTKEY, WM_HOTKEY) };
        match getmsg_result {
            BOOL(-1) => Some(Err(io::Error::from(windows::core::Error::from_win32()))),
            BOOL(0) => None, // WM_QUIT
            _ => {
                self.id_assocs
                    .get(
                        &message.wParam.0.try_into().expect(
                            "ID from GetMessageW should be in range for ID map integer type",
                        ),
                    )
                    .map(|user_id| Ok(*user_id))
            }
        }
    }
}

impl<ID> Drop for GlobalHotkeyIter<ID> {
    fn drop(&mut self) {
        for id in (GlobalHotkeySet::<ID>::MIN_ID..).take(self.hotkeys.hotkey_defs.len()) {
            unsafe {
                UnregisterHotKey(None, id).expect("Cannot unregister hotkey");
            }
        }
    }
}

impl<ID> TryFrom<GlobalHotkeySet<ID>> for GlobalHotkeyIter<ID>
where
    ID: 'static + Copy + Send + Sync,
{
    type Error = io::Error;

    fn try_from(value: GlobalHotkeySet<ID>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(Copy, Clone, Debug)]
struct HotkeyDef<ID> {
    user_id: ID,
    key_combination: KeyCombination,
}

/// Modifier key than cannot be used by itself for hotkeys.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum Modifier {
    Alt = MOD_ALT.0,
    Ctrl = MOD_CONTROL.0,
    Shift = MOD_SHIFT.0,
    Win = MOD_WIN.0,
}

/// A combination of modifier keys.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ModifierCombination(u32);

/// A combination of zero or more modifiers and exactly one normal key.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct KeyCombination {
    modifiers: ModifierCombination,
    key: KeyboardKey,
}

impl KeyCombination {
    fn new_from(modifiers: ModifierCombination, key: KeyboardKey) -> Self {
        KeyCombination {
            // Changes the hotkey behavior so that the keyboard auto-repeat does not yield multiple hotkey notifications.
            modifiers: ModifierCombination(modifiers.0 | MOD_NOREPEAT.0),
            key,
        }
    }
}

impl From<Modifier> for ModifierCombination {
    fn from(modifier: Modifier) -> Self {
        ModifierCombination(modifier.into())
    }
}

impl From<KeyboardKey> for KeyCombination {
    fn from(key: KeyboardKey) -> Self {
        KeyCombination::new_from(ModifierCombination(0), key)
    }
}

impl<T2> Add<T2> for Modifier
where
    T2: Into<ModifierCombination>,
{
    type Output = ModifierCombination;

    fn add(self, rhs: T2) -> Self::Output {
        rhs.into() + self
    }
}

impl<T2> Add<T2> for ModifierCombination
where
    T2: Into<ModifierCombination>,
{
    type Output = ModifierCombination;

    fn add(self, rhs: T2) -> Self::Output {
        #[allow(clippy::suspicious_arithmetic_impl)]
        ModifierCombination(self.0 | rhs.into().0)
    }
}

impl Add<KeyboardKey> for ModifierCombination {
    type Output = KeyCombination;

    fn add(self, rhs: KeyboardKey) -> Self::Output {
        KeyCombination::new_from(self, rhs)
    }
}

impl Add<KeyboardKey> for Modifier {
    type Output = KeyCombination;

    fn add(self, rhs: KeyboardKey) -> Self::Output {
        KeyCombination::new_from(self.into(), rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_hotkey_listener() -> io::Result<()> {
        let hotkeys = GlobalHotkeySet::new().add_hotkey(
            0,
            Modifier::Ctrl + Modifier::Alt + Modifier::Shift + KeyboardKey::Oem1,
        );

        let _ = GlobalHotkeyIter::new(hotkeys)?;
        Ok(())
    }
}
