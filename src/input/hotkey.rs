//! Global hotkeys.

use std::cell::{
    Cell,
    RefCell,
};
use std::collections::HashMap;
use std::io;
use std::marker::PhantomData;
use std::ops::Add;
use std::rc::Rc;

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
    MSG,
    WM_HOTKEY,
};

use crate::input::KeyboardKey;
use crate::messaging::{
    ListenerFn,
    ThreadMessageLoop,
};

pub type HotkeyId = u8;

/// Registers global hotkeys.
///
/// # Multithreading
///
/// This type is not [`Send`] and [`Sync`] because the hotkeys are registered only to the current thread.
pub struct GlobalHotkeySet<'a> {
    hotkey_defs: Rc<RefCell<HashMap<HotkeyId, HotkeyDef>>>,
    message_loop_listeners: Rc<RefCell<HashMap<u32, ListenerFn<'a>>>>,
    _marker: PhantomData<*mut ()>,
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(GlobalHotkeySet: Send, Sync);

impl<'a> GlobalHotkeySet<'a> {
    thread_local! {
        static RUNNING: Cell<bool> = const { Cell::new(false) };
    }

    /// Registers a new hotkey set with the system.
    ///
    /// The listener will be called on matching hotkey events if the given [`ThreadMessageLoop`] is running.
    ///
    /// # Panics
    ///
    /// Will panic if more than 1 instance is created per thread.
    pub fn new<F>(message_loop: &mut ThreadMessageLoop<'a>, mut listener: F) -> io::Result<Self>
    where
        F: FnMut(HotkeyId) -> io::Result<()> + 'a,
    {
        assert!(
            !Self::RUNNING.get(),
            "Only one hotkey set may be active per thread"
        );
        Self::RUNNING.set(true);
        let hotkey_defs: Rc<RefCell<HashMap<_, _>>> = Default::default();
        let message_loop_listener = {
            let hotkey_defs = hotkey_defs.clone();
            move |raw_message: MSG| {
                assert_eq!(raw_message.message, WM_HOTKEY);
                if let Ok(hotkey_id) = u8::try_from(raw_message.wParam.0) {
                    if hotkey_defs.borrow().contains_key(&hotkey_id) {
                        return listener(hotkey_id);
                    }
                }
                Ok(())
            }
        };
        let message_loop_listeners = message_loop.listeners.clone();
        message_loop_listeners
            .borrow_mut()
            .insert(WM_HOTKEY, Box::new(message_loop_listener));
        Ok(Self {
            hotkey_defs,
            message_loop_listeners,
            _marker: PhantomData,
        })
    }

    /// Adds a hotkey.
    ///
    /// Not all key combinations may work as hotkeys.
    pub fn add_hotkey<KC>(&mut self, user_id: HotkeyId, key_combination: KC) -> io::Result<()>
    where
        KC: Into<KeyCombination>,
    {
        let new_def = HotkeyDef::new(user_id, key_combination.into())?;
        self.hotkey_defs.borrow_mut().insert(user_id, new_def);
        Ok(())
    }

    pub fn listen_for_hotkeys_on(self, message_loop: &mut ThreadMessageLoop) -> io::Result<()> {
        message_loop.run_thread_message_loop_internal(|_| Ok(()), false, None)
    }
}

impl Drop for GlobalHotkeySet<'_> {
    fn drop(&mut self) {
        let _ = self
            .message_loop_listeners
            .borrow_mut()
            .remove(&WM_HOTKEY)
            .expect("Listener should exist when dropping");
        Self::RUNNING.set(false);
    }
}

#[derive(Debug)]
struct HotkeyDef {
    user_id: HotkeyId,
    #[allow(dead_code)]
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
        let mut message_loop = ThreadMessageLoop::new();
        let mut hotkeys = GlobalHotkeySet::new(&mut message_loop, |_| Ok(()))?;
        hotkeys.add_hotkey(
            0,
            Modifier::Ctrl + Modifier::Alt + Modifier::Shift + KeyboardKey::Oem1,
        )?;
        ThreadMessageLoop::post_quit_message();
        message_loop.run()?;
        Ok(())
    }
}
