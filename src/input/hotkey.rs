//! Global hotkeys.

use std::cell::Cell;
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

use crate::input::KeyboardKey;
use crate::messaging::{
    ThreadMessage,
    ThreadMessageLoop,
};

pub type HotkeyId = u8;

/// Registers global hotkeys.
///
/// # Multithreading
///
/// This type is not [`Send`] and [`Sync`] because the hotkeys are registered only to the current thread.
pub struct GlobalHotkeySet {
    hotkey_defs: HashMap<HotkeyId, HotkeyDef>,
    _marker: PhantomData<*mut ()>,
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(GlobalHotkeySet: Send, Sync);

impl GlobalHotkeySet {
    thread_local! {
        static RUNNING: Cell<bool> = const { Cell::new(false) };
    }

    /// Registers a new hotkey set with the system.
    ///
    /// # Panics
    ///
    /// Will panic if more than 1 instance is created per thread.
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        assert!(
            !Self::RUNNING.get(),
            "Only one hotkey set may be active per thread"
        );
        Self::RUNNING.set(true);
        let hotkey_defs = Default::default();
        Self {
            hotkey_defs,
            _marker: PhantomData,
        }
    }

    /// Adds a hotkey.
    ///
    /// Not all key combinations may work as hotkeys.
    pub fn add_hotkey<KC>(&mut self, user_id: HotkeyId, key_combination: KC) -> io::Result<()>
    where
        KC: Into<KeyCombination>,
    {
        let new_def = HotkeyDef::new(user_id, key_combination.into())?;
        self.hotkey_defs.insert(user_id, new_def);
        Ok(())
    }

    pub fn listen_for_hotkeys<E, F>(&mut self, mut listener: F) -> Result<(), E>
    where
        E: From<io::Error>,
        F: FnMut(HotkeyId) -> Result<(), E>,
    {
        let message_listener = |message| {
            if let ThreadMessage::Hotkey(hotkey_id) = message {
                #[expect(clippy::missing_panics_doc)]
                {
                    assert!(self.hotkey_defs.contains_key(&hotkey_id));
                }
                listener(hotkey_id)
            } else {
                Ok(())
            }
        };
        ThreadMessageLoop::new().run_thread_message_loop_internal(message_listener, false, None)
    }
}

impl Drop for GlobalHotkeySet {
    fn drop(&mut self) {
        Self::RUNNING.set(false);
    }
}

#[derive(Debug)]
struct HotkeyDef {
    user_id: HotkeyId,
    #[expect(dead_code)]
    key_combination: KeyCombination,
}

impl HotkeyDef {
    fn new(user_id: HotkeyId, key_combination: KeyCombination) -> io::Result<Self> {
        unsafe {
            RegisterHotKey(
                None,
                user_id.into(),
                HOT_KEY_MODIFIERS(key_combination.modifiers.0),
                key_combination.key.into(),
            )
        }?;
        Ok(Self {
            user_id,
            key_combination,
        })
    }

    fn unregister(&self) -> io::Result<()> {
        unsafe { UnregisterHotKey(None, self.user_id.into()) }?;
        Ok(())
    }
}

impl Drop for HotkeyDef {
    fn drop(&mut self) {
        self.unregister().expect("Cannot unregister hotkey");
    }
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
        #[expect(clippy::suspicious_arithmetic_impl)]
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
        let mut message_loop = ThreadMessageLoop::new();
        let mut hotkeys = GlobalHotkeySet::new();
        hotkeys.add_hotkey(
            0,
            Modifier::Ctrl + Modifier::Alt + Modifier::Shift + KeyboardKey::Oem1,
        )?;
        ThreadMessageLoop::post_quit_message();
        message_loop.run()?;
        Ok(())
    }
}
