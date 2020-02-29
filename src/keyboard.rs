/*!
Keyboard and hotkeys.

## Hotkeys
* [GlobalHotkeySet](keyboard::GlobalHotkeySet): Define and listen to global hotkeys
*/

use std::collections::HashMap;
use std::io;
use std::mem::MaybeUninit;
use std::ops::Add;
use std::ptr;
use std::sync::mpsc;
use std::thread;

use num_enum::IntoPrimitive;
use winapi::ctypes::c_int;
use winapi::shared::minwindef::UINT;
use winapi::shared::minwindef::{
    BOOL,
    INT,
    LPARAM,
};
use winapi::um::winuser::{
    self,
    GetMessageW,
    RegisterHotKey,
    UnregisterHotKey,
    MOD_ALT,
    MOD_CONTROL,
    MOD_NOREPEAT,
    MOD_SHIFT,
    MOD_WIN,
    VK_ADD,
    VK_BACK,
    VK_DELETE,
    VK_DIVIDE,
    VK_DOWN,
    VK_END,
    VK_ESCAPE,
    VK_F1,
    VK_F10,
    VK_F11,
    VK_F12,
    VK_F2,
    VK_F3,
    VK_F4,
    VK_F5,
    VK_F6,
    VK_F7,
    VK_F8,
    VK_F9,
    VK_HOME,
    VK_INSERT,
    VK_LEFT,
    VK_MULTIPLY,
    VK_NEXT,
    VK_NUMPAD0,
    VK_NUMPAD1,
    VK_NUMPAD2,
    VK_NUMPAD3,
    VK_NUMPAD4,
    VK_NUMPAD5,
    VK_NUMPAD6,
    VK_NUMPAD7,
    VK_NUMPAD8,
    VK_NUMPAD9,
    VK_OEM_1,
    VK_OEM_102,
    VK_OEM_2,
    VK_OEM_3,
    VK_OEM_4,
    VK_OEM_5,
    VK_OEM_6,
    VK_OEM_7,
    VK_OEM_8,
    VK_OEM_COMMA,
    VK_OEM_MINUS,
    VK_OEM_PERIOD,
    VK_OEM_PLUS,
    VK_PAUSE,
    VK_PRIOR,
    VK_RETURN,
    VK_RIGHT,
    VK_SPACE,
    VK_SUBTRACT,
    VK_TAB,
    VK_UP,
    WM_HOTKEY,
};

use crate::internal::ReturnValue;
use winapi::_core::time::Duration;

#[derive(Copy, Clone)]
struct HotkeyDef<ID> {
    user_id: ID,
    key_combination: KeyCombination,
}

/// A group of global hotkeys that can be listened for.
///
/// # Examples
///
/// ```no_run
/// use winapi_easy::keyboard::{GlobalHotkeySet, Modifier, Key};
///
/// #[derive(Copy, Clone)]
/// enum MyAction {
///     One,
///     Two,
/// }
///
/// let hotkeys = GlobalHotkeySet::new()
///     .add_global_hotkey(MyAction::One, Modifier::Ctrl + Modifier::Alt + Key::A)
///     .add_global_hotkey(MyAction::Two, Modifier::Shift + Modifier::Alt + Key::B);
///
/// for action in hotkeys.listen_for_hotkeys()? {
///     match action? {
///         MyAction::One => println!("One!"),
///         MyAction::Two => println!("Two!"),
///     }
/// }
///
/// # std::result::Result::<(), std::io::Error>::Ok(())
/// ```
#[derive(Clone)]
pub struct GlobalHotkeySet<ID> {
    hotkey_defs: Vec<HotkeyDef<ID>>,
    hotkeys_active: bool,
}

impl<ID> GlobalHotkeySet<ID> {
    const MIN_ID: c_int = 1;
}

impl<ID> GlobalHotkeySet<ID>
where
    ID: 'static + Copy + Send + Sync,
{
    pub fn new() -> Self {
        Self {
            hotkey_defs: Vec::new(),
            hotkeys_active: false,
        }
    }

    pub fn add_global_hotkey<KC>(mut self, id: ID, key_combination: KC) -> Self
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
        self.listen_for_hotkeys_with_sleeptime(None)
    }

    pub fn listen_for_hotkeys_with_sleeptime(mut self, sleep_time: Option<Duration>) -> io::Result<impl IntoIterator<Item = io::Result<ID>>> {
        let (tx_hotkey, rx_hotkey) = mpsc::channel();
        thread::spawn(move || {
            let ids = || Self::MIN_ID..;
            let register_result: io::Result<()> =
                ids()
                    .zip(&self.hotkey_defs)
                    .try_for_each(|(curr_id, hotkey_def)| {
                        let result: io::Result<BOOL> = unsafe {
                            RegisterHotKey(
                                ptr::null_mut(),
                                curr_id,
                                hotkey_def.key_combination.modifiers.0 as UINT,
                                c_int::from(hotkey_def.key_combination.key) as UINT,
                            )
                            .if_null_get_last_error()
                        };
                        if result.is_ok() {
                            Ok(())
                        } else {
                            (Self::MIN_ID..=curr_id - 1).rev().for_each(|id| unsafe {
                                UnregisterHotKey(ptr::null_mut(), id)
                                    .if_null_panic("Cannot unregister hotkey");
                            });
                            result.map(|_| ())
                        }
                    });
            if let Err(err) = register_result {
                tx_hotkey.send(Err(err)).unwrap_or(());
            } else {
                self.hotkeys_active = true;
                let id_assocs: HashMap<INT, ID> = ids()
                    .zip(self.hotkey_defs.iter().map(|def| def.user_id))
                    .collect();
                loop {
                    if let Some(sleep) = sleep_time {
                        std::thread::sleep(sleep);
                    };
                    let mut message: MaybeUninit<winuser::MSG> = MaybeUninit::uninit();
                    let getmsg_result = unsafe {
                        GetMessageW(message.as_mut_ptr(), ptr::null_mut(), WM_HOTKEY, WM_HOTKEY)
                    };
                    let message = unsafe { message.assume_init() };
                    let to_send = match getmsg_result {
                        -1 => Some(Err(io::Error::last_os_error())),
                        0 => break, // WM_QUIT
                        _ => {
                            if let Some(user_id) = id_assocs.get(&(message.wParam as INT)) {
                                Some(Ok(*user_id))
                            } else {
                                None
                            }
                        }
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
        Ok(rx_hotkey)
    }
}

impl<ID> Drop for GlobalHotkeySet<ID> {
    fn drop(&mut self) {
        if self.hotkeys_active {
            for id in (Self::MIN_ID..).take(self.hotkey_defs.len()) {
                unsafe {
                    UnregisterHotKey(ptr::null_mut(), id).if_null_panic("Cannot unregister hotkey");
                }
            }
        }
    }
}

/// Non-modifier key usable for hotkeys.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
#[repr(i32)]
pub enum Key {
    Backspace = VK_BACK,
    Tab = VK_TAB,
    Return = VK_RETURN,
    Pause = VK_PAUSE,
    Esc = VK_ESCAPE,
    Space = VK_SPACE,
    PgUp = VK_PRIOR,
    PgDown = VK_NEXT,
    End = VK_END,
    Home = VK_HOME,
    LeftArrow = VK_LEFT,
    UpArrow = VK_UP,
    RightArrow = VK_RIGHT,
    DownArrow = VK_DOWN,
    Insert = VK_INSERT,
    Delete = VK_DELETE,
    Number0 = 0x30,
    Number1 = 0x31,
    Number2 = 0x32,
    Number3 = 0x33,
    Number4 = 0x34,
    Number5 = 0x35,
    Number6 = 0x36,
    Number7 = 0x37,
    Number8 = 0x38,
    Number9 = 0x39,
    A = 0x41,
    B = 0x42,
    C = 0x43,
    D = 0x44,
    E = 0x45,
    F = 0x46,
    G = 0x47,
    H = 0x48,
    I = 0x49,
    J = 0x4A,
    K = 0x4B,
    L = 0x4C,
    M = 0x4D,
    N = 0x4E,
    O = 0x4F,
    P = 0x50,
    Q = 0x51,
    R = 0x52,
    S = 0x53,
    T = 0x54,
    U = 0x55,
    V = 0x56,
    W = 0x57,
    X = 0x58,
    Y = 0x59,
    Z = 0x5A,
    Numpad0 = VK_NUMPAD0,
    Numpad1 = VK_NUMPAD1,
    Numpad2 = VK_NUMPAD2,
    Numpad3 = VK_NUMPAD3,
    Numpad4 = VK_NUMPAD4,
    Numpad5 = VK_NUMPAD5,
    Numpad6 = VK_NUMPAD6,
    Numpad7 = VK_NUMPAD7,
    Numpad8 = VK_NUMPAD8,
    Numpad9 = VK_NUMPAD9,
    Multiply = VK_MULTIPLY,
    Add = VK_ADD,
    Subtract = VK_SUBTRACT,
    Divide = VK_DIVIDE,
    F1 = VK_F1,
    F2 = VK_F2,
    F3 = VK_F3,
    F4 = VK_F4,
    F5 = VK_F5,
    F6 = VK_F6,
    F7 = VK_F7,
    F8 = VK_F8,
    F9 = VK_F9,
    F10 = VK_F10,
    F11 = VK_F11,
    F12 = VK_F12,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the ';:' key
    /// * For the German keyboard, the 'ü' key
    Oem1 = VK_OEM_1,
    /// For any country/region, the '+' key
    OemPlus = VK_OEM_PLUS,
    /// For any country/region, the ',' key
    OemComma = VK_OEM_COMMA,
    /// For any country/region, the '-' key
    OemMinus = VK_OEM_MINUS,
    /// For any country/region, the '.' key
    OemPeriod = VK_OEM_PERIOD,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '/?' key
    /// * For the German keyboard, the '#'' key
    Oem2 = VK_OEM_2,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '`~' key
    /// * For the German keyboard, the 'ö' key
    Oem3 = VK_OEM_3,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '[{' key
    /// * For the German keyboard, the 'ß?' key
    Oem4 = VK_OEM_4,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '\|' key besides 'Enter'
    /// * For the German keyboard, the '^°' key
    Oem5 = VK_OEM_5,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the ']}' key
    /// * For the German keyboard, the '´`' key
    Oem6 = VK_OEM_6,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the 'single-quote/double-quote' key
    /// * For the German keyboard, the 'ä' key
    Oem7 = VK_OEM_7,
    Oem8 = VK_OEM_8,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '\|' key besides the 'z' key
    /// * For the German keyboard, the '<>' key
    Oem102 = VK_OEM_102,
}

/// Modifier key than cannot be used by itself for hotkeys.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq)]
#[repr(isize)]
pub enum Modifier {
    Alt = MOD_ALT,
    Ctrl = MOD_CONTROL,
    Shift = MOD_SHIFT,
    Win = MOD_WIN,
}

/// A combination of modifier keys.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ModifierCombination(LPARAM);

/// A combination of zero or more modifiers and exactly one normal key.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct KeyCombination {
    modifiers: ModifierCombination,
    key: Key,
}

impl KeyCombination {
    fn new_from(modifiers: ModifierCombination, key: Key) -> Self {
        KeyCombination {
            /// Changes the hotkey behavior so that the keyboard auto-repeat does not yield multiple hotkey notifications.
            modifiers: ModifierCombination(modifiers.0 | MOD_NOREPEAT),
            key,
        }
    }
}

impl From<Modifier> for ModifierCombination {
    fn from(modifier: Modifier) -> Self {
        ModifierCombination(modifier.into())
    }
}

impl From<Key> for KeyCombination {
    fn from(key: Key) -> Self {
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
        ModifierCombination(self.0 | rhs.into().0)
    }
}

impl Add<Key> for ModifierCombination {
    type Output = KeyCombination;

    fn add(self, rhs: Key) -> Self::Output {
        KeyCombination::new_from(self, rhs)
    }
}

impl Add<Key> for Modifier {
    type Output = KeyCombination;

    fn add(self, rhs: Key) -> Self::Output {
        KeyCombination::new_from(self.into(), rhs)
    }
}
