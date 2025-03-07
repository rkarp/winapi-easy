//! Global hotkeys.

use std::collections::HashMap;
use std::{
    io,
    thread,
};
use std::ops::Add;
use std::sync::mpsc;

use num_enum::IntoPrimitive;
use windows::core::BOOL;
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
/// for action in hotkeys.listen_for_hotkeys() {
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
    hotkeys_active: bool,
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

    /// Registers the hotkeys with the system and then reacts to hotkey events.
    pub fn listen_for_hotkeys(mut self) -> impl IntoIterator<Item = io::Result<ID>> {
        let (tx_hotkey, rx_hotkey) = mpsc::channel();
        thread::spawn(move || {
            let ids = || Self::MIN_ID..;
            let register_result: io::Result<()> =
                ids()
                    .zip(&self.hotkey_defs)
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
                            (Self::MIN_ID..=curr_id - 1).rev().for_each(|id| unsafe {
                                UnregisterHotKey(None, id).expect("Cannot unregister hotkey");
                            });
                        }
                        result
                    });
            if let Err(err) = register_result {
                tx_hotkey.send(Err(err)).unwrap_or(());
            } else {
                self.hotkeys_active = true;
                let id_assocs: HashMap<i32, ID> = ids()
                    .zip(self.hotkey_defs.iter().map(|def| def.user_id))
                    .collect();
                loop {
                    let mut message: MSG = Default::default();
                    let getmsg_result =
                        unsafe { GetMessageW(&mut message, None, WM_HOTKEY, WM_HOTKEY) };
                    let to_send = match getmsg_result {
                        BOOL(-1) => Some(Err(io::Error::from(windows::core::Error::from_win32()))),
                        BOOL(0) => break, // WM_QUIT
                        _ => id_assocs
                            .get(&message.wParam.0.try_into().expect(
                                "ID from GetMessageW should be in range for ID map integer type",
                            ))
                            .map(|user_id| Ok(*user_id)),
                    };
                    if let Some(to_send) = to_send {
                        let send_result = tx_hotkey.send(to_send);
                        if send_result.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        rx_hotkey
    }
}

impl<ID> Default for GlobalHotkeySet<ID> {
    fn default() -> Self {
        Self {
            hotkey_defs: Vec::new(),
            hotkeys_active: false,
        }
    }
}

impl<ID> Drop for GlobalHotkeySet<ID> {
    fn drop(&mut self) {
        if self.hotkeys_active {
            for id in (Self::MIN_ID..).take(self.hotkey_defs.len()) {
                unsafe {
                    UnregisterHotKey(None, id).expect("Cannot unregister hotkey");
                }
            }
        }
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
