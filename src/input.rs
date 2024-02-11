//! Keyboard and hotkeys.

use std::collections::HashMap;
use std::ops::Add;
use std::sync::mpsc;
use std::thread;
use std::{
    io,
    mem,
};

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use windows::Win32::Foundation::BOOL;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState,
    GetKeyState,
    RegisterHotKey,
    SendInput,
    UnregisterHotKey,
    HOT_KEY_MODIFIERS,
    INPUT,
    INPUT_0,
    INPUT_KEYBOARD,
    KEYBDINPUT,
    KEYEVENTF_KEYUP,
    MOD_ALT,
    MOD_CONTROL,
    MOD_NOREPEAT,
    MOD_SHIFT,
    MOD_WIN,
    VIRTUAL_KEY,
    VK_0,
    VK_1,
    VK_2,
    VK_3,
    VK_4,
    VK_5,
    VK_6,
    VK_7,
    VK_8,
    VK_9,
    VK_A,
    VK_ADD,
    VK_APPS,
    VK_B,
    VK_BACK,
    VK_C,
    VK_CAPITAL,
    VK_D,
    VK_DECIMAL,
    VK_DELETE,
    VK_DIVIDE,
    VK_DOWN,
    VK_E,
    VK_END,
    VK_ESCAPE,
    VK_F,
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
    VK_G,
    VK_H,
    VK_HOME,
    VK_I,
    VK_INSERT,
    VK_J,
    VK_K,
    VK_L,
    VK_LCONTROL,
    VK_LEFT,
    VK_LMENU,
    VK_LSHIFT,
    VK_LWIN,
    VK_M,
    VK_MULTIPLY,
    VK_N,
    VK_NEXT,
    VK_NUMLOCK,
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
    VK_O,
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
    VK_P,
    VK_PAUSE,
    VK_PRIOR,
    VK_Q,
    VK_R,
    VK_RCONTROL,
    VK_RETURN,
    VK_RIGHT,
    VK_RMENU,
    VK_RSHIFT,
    VK_RWIN,
    VK_S,
    VK_SCROLL,
    VK_SNAPSHOT,
    VK_SPACE,
    VK_SUBTRACT,
    VK_T,
    VK_TAB,
    VK_U,
    VK_UP,
    VK_V,
    VK_VOLUME_DOWN,
    VK_VOLUME_MUTE,
    VK_VOLUME_UP,
    VK_W,
    VK_X,
    VK_Y,
    VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW,
    MSG,
    WHEEL_DELTA,
    WM_HOTKEY,
};

use crate::internal::ReturnValue;

pub mod hooking;

#[derive(Copy, Clone, Debug)]
struct HotkeyDef<ID> {
    user_id: ID,
    key_combination: KeyCombination,
}

/// A group of global hotkeys that can be listened for.
///
/// # Examples
///
/// ```no_run
/// use winapi_easy::input::{GlobalHotkeySet, Modifier, Key};
///
/// #[derive(Copy, Clone)]
/// enum MyAction {
///     One,
///     Two,
/// }
///
/// let hotkeys = GlobalHotkeySet::new()
///     .add_hotkey(MyAction::One, Modifier::Ctrl + Modifier::Alt + Key::A)
///     .add_hotkey(MyAction::Two, Modifier::Shift + Modifier::Alt + Key::B);
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
    pub fn listen_for_hotkeys(mut self) -> io::Result<impl IntoIterator<Item = io::Result<ID>>> {
        let (tx_hotkey, rx_hotkey) = mpsc::channel();
        thread::spawn(move || {
            let ids = || Self::MIN_ID..;
            let register_result: io::Result<()> =
                ids()
                    .zip(&self.hotkey_defs)
                    .try_for_each(|(curr_id, hotkey_def)| {
                        let result: io::Result<BOOL> = unsafe {
                            RegisterHotKey(
                                None,
                                curr_id,
                                HOT_KEY_MODIFIERS(hotkey_def.key_combination.modifiers.0),
                                hotkey_def.key_combination.key.into(),
                            )
                            .if_null_get_last_error()
                        };
                        if result.is_ok() {
                            Ok(())
                        } else {
                            (Self::MIN_ID..=curr_id - 1).rev().for_each(|id| unsafe {
                                UnregisterHotKey(None, id)
                                    .if_null_panic("Cannot unregister hotkey");
                            });
                            result.map(|_| ())
                        }
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
                        BOOL(-1) => Some(Err(io::Error::last_os_error())),
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
        Ok(rx_hotkey)
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
                    UnregisterHotKey(None, id).if_null_panic("Cannot unregister hotkey");
                }
            }
        }
    }
}

/// Non-modifier key usable for hotkeys.
///
/// # Related docs
///
/// [Microsoft docs for virtual key codes](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes)
#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u16)]
pub enum Key {
    Backspace = VK_BACK.0,
    Tab = VK_TAB.0,
    Return = VK_RETURN.0,
    Pause = VK_PAUSE.0,
    CapsLock = VK_CAPITAL.0,
    Esc = VK_ESCAPE.0,
    Space = VK_SPACE.0,
    PgUp = VK_PRIOR.0,
    PgDown = VK_NEXT.0,
    End = VK_END.0,
    Home = VK_HOME.0,
    LeftArrow = VK_LEFT.0,
    UpArrow = VK_UP.0,
    RightArrow = VK_RIGHT.0,
    DownArrow = VK_DOWN.0,
    PrintScreen = VK_SNAPSHOT.0,
    Insert = VK_INSERT.0,
    Delete = VK_DELETE.0,
    Number0 = VK_0.0,
    Number1 = VK_1.0,
    Number2 = VK_2.0,
    Number3 = VK_3.0,
    Number4 = VK_4.0,
    Number5 = VK_5.0,
    Number6 = VK_6.0,
    Number7 = VK_7.0,
    Number8 = VK_8.0,
    Number9 = VK_9.0,
    A = VK_A.0,
    B = VK_B.0,
    C = VK_C.0,
    D = VK_D.0,
    E = VK_E.0,
    F = VK_F.0,
    G = VK_G.0,
    H = VK_H.0,
    I = VK_I.0,
    J = VK_J.0,
    K = VK_K.0,
    L = VK_L.0,
    M = VK_M.0,
    N = VK_N.0,
    O = VK_O.0,
    P = VK_P.0,
    Q = VK_Q.0,
    R = VK_R.0,
    S = VK_S.0,
    T = VK_T.0,
    U = VK_U.0,
    V = VK_V.0,
    W = VK_W.0,
    X = VK_X.0,
    Y = VK_Y.0,
    Z = VK_Z.0,
    LeftWindows = VK_LWIN.0,
    RightWindows = VK_RWIN.0,
    Menu = VK_APPS.0,
    Numpad0 = VK_NUMPAD0.0,
    Numpad1 = VK_NUMPAD1.0,
    Numpad2 = VK_NUMPAD2.0,
    Numpad3 = VK_NUMPAD3.0,
    Numpad4 = VK_NUMPAD4.0,
    Numpad5 = VK_NUMPAD5.0,
    Numpad6 = VK_NUMPAD6.0,
    Numpad7 = VK_NUMPAD7.0,
    Numpad8 = VK_NUMPAD8.0,
    Numpad9 = VK_NUMPAD9.0,
    Multiply = VK_MULTIPLY.0,
    Add = VK_ADD.0,
    Subtract = VK_SUBTRACT.0,
    Decimal = VK_DECIMAL.0,
    Divide = VK_DIVIDE.0,
    F1 = VK_F1.0,
    F2 = VK_F2.0,
    F3 = VK_F3.0,
    F4 = VK_F4.0,
    F5 = VK_F5.0,
    F6 = VK_F6.0,
    F7 = VK_F7.0,
    F8 = VK_F8.0,
    F9 = VK_F9.0,
    F10 = VK_F10.0,
    F11 = VK_F11.0,
    F12 = VK_F12.0,
    NumLock = VK_NUMLOCK.0,
    ScrollLock = VK_SCROLL.0,
    LeftShift = VK_LSHIFT.0,
    RightShift = VK_RSHIFT.0,
    LeftCtrl = VK_LCONTROL.0,
    RightCtrl = VK_RCONTROL.0,
    LeftAlt = VK_LMENU.0,
    RightAlt = VK_RMENU.0,
    VolumeMute = VK_VOLUME_MUTE.0,
    VolumeDown = VK_VOLUME_DOWN.0,
    VolumeUp = VK_VOLUME_UP.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the ';:' key
    /// * For the German keyboard, the 'ü' key
    Oem1 = VK_OEM_1.0,
    /// For any country/region, the '+' key
    OemPlus = VK_OEM_PLUS.0,
    /// For any country/region, the ',' key
    OemComma = VK_OEM_COMMA.0,
    /// For any country/region, the '-' key
    OemMinus = VK_OEM_MINUS.0,
    /// For any country/region, the '.' key
    OemPeriod = VK_OEM_PERIOD.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '/?' key
    /// * For the German keyboard, the '#'' key
    Oem2 = VK_OEM_2.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '`~' key
    /// * For the German keyboard, the 'ö' key
    Oem3 = VK_OEM_3.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '[{' key
    /// * For the German keyboard, the 'ß?' key
    Oem4 = VK_OEM_4.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '\|' key besides 'Enter'
    /// * For the German keyboard, the '^°' key
    Oem5 = VK_OEM_5.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the ']}' key
    /// * For the German keyboard, the '´`' key
    Oem6 = VK_OEM_6.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the 'single-quote/double-quote' key
    /// * For the German keyboard, the 'ä' key
    Oem7 = VK_OEM_7.0,
    Oem8 = VK_OEM_8.0,
    /// Used for miscellaneous characters; it can vary by keyboard.
    ///
    /// * For the US standard keyboard, the '\|' key besides the 'z' key
    /// * For the German keyboard, the '<>' key
    Oem102 = VK_OEM_102.0,
    /// Other virtual key code.
    #[num_enum(catch_all)]
    Other(u16),
}

impl Key {
    pub fn is_pressed(self) -> io::Result<bool> {
        let result = unsafe {
            GetAsyncKeyState(self.into())
                .if_null_to_error(|| io::ErrorKind::PermissionDenied.into())? as u16
        };
        Ok(result >> (u16::BITS - 1) == 1)
    }

    /// Returns true if the key has lock functionality (e.g. Caps Lock) and the lock is toggled.
    pub fn is_lock_toggled(self) -> bool {
        let result = unsafe { GetKeyState(self.into()) as u16 };
        result & 1 == 1
    }
}

impl From<Key> for u32 {
    fn from(value: Key) -> Self {
        Self::from(u16::from(value))
    }
}

impl From<Key> for i32 {
    fn from(value: Key) -> Self {
        u16::from(value) as Self
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
    key: Key,
}

impl KeyCombination {
    fn new_from(modifiers: ModifierCombination, key: Key) -> Self {
        KeyCombination {
            /// Changes the hotkey behavior so that the keyboard auto-repeat does not yield multiple hotkey notifications.
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
        #[allow(clippy::suspicious_arithmetic_impl)]
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

/// Globally sends a key combination as if the user had performed it.
///
/// This will cause a 'press' event for each key in the list (in the given order),
/// followed by a sequence of 'release' events (in the inverse order of the list).
pub fn send_key_combination(keys: &[Key]) -> io::Result<()> {
    let raw_input_pairs: Vec<_> = keys
        .iter()
        .copied()
        .map(|key: Key| {
            let raw_key = u16::from(key);
            let raw_keybdinput = KEYBDINPUT {
                wVk: VIRTUAL_KEY(raw_key),
                wScan: 0,
                dwFlags: Default::default(),
                time: 0,
                dwExtraInfo: 0,
            };
            let raw_keybdinput_release = KEYBDINPUT {
                dwFlags: raw_keybdinput.dwFlags | KEYEVENTF_KEYUP,
                ..raw_keybdinput
            };
            let raw_input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 { ki: raw_keybdinput },
            };
            let raw_input_release = INPUT {
                Anonymous: INPUT_0 {
                    ki: raw_keybdinput_release,
                },
                ..raw_input
            };
            (raw_input, raw_input_release)
        })
        .collect();
    let raw_inputs: Vec<_> = raw_input_pairs
        .iter()
        .map(|x| x.0)
        .chain(raw_input_pairs.iter().rev().map(|x| x.1))
        .collect();
    let raw_input_size = mem::size_of::<INPUT>()
        .try_into()
        .expect("Struct size conversion failed");

    let expected_sent_size =
        u32::try_from(raw_inputs.len()).expect("Inputs length conversion failed");
    unsafe {
        SendInput(raw_inputs.as_slice(), raw_input_size)
            .if_not_eq_to_error(expected_sent_size, io::Error::last_os_error)?;
    }
    Ok(())
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// X-Button 1 or 2.
    ///
    /// Other X-Buttons are not handled by the Windows API.
    XButton(u16),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MouseScrollEvent {
    Up,
    Down,
    /// Values other than positive or negative [`WHEEL_DELTA`] are used for mouses
    /// with continuous scroll wheels.
    Continuous(i16),
}

impl MouseScrollEvent {
    pub(crate) fn from_raw_movement(raw_movement: u16) -> Self {
        let raw_movement = raw_movement as i16;
        const WHEEL_DELTA_INT: i16 = WHEEL_DELTA as _;
        if raw_movement == WHEEL_DELTA_INT {
            MouseScrollEvent::Up
        } else if raw_movement == -WHEEL_DELTA_INT {
            MouseScrollEvent::Down
        } else {
            MouseScrollEvent::Continuous(raw_movement)
        }
    }
}
