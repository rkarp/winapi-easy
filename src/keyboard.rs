/*!
Keyboard and hotkeys.

## Hotkeys
* [GlobalHotkeySet](keyboard::GlobalHotkeySet): Define and listen to global hotkeys
*/

use std::collections::HashMap;
use std::io;
use std::mem::MaybeUninit;
use std::ops::Add;
use std::sync::mpsc;
use std::thread;

use num_enum::IntoPrimitive;
use windows::Win32::Foundation::BOOL;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey,
    UnregisterHotKey,
    HOT_KEY_MODIFIERS,
    MOD_ALT,
    MOD_CONTROL,
    MOD_NOREPEAT,
    MOD_SHIFT,
    MOD_WIN,
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
    VK_B,
    VK_BACK,
    VK_C,
    VK_D,
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
    VK_LEFT,
    VK_M,
    VK_MULTIPLY,
    VK_N,
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
    VK_RETURN,
    VK_RIGHT,
    VK_S,
    VK_SPACE,
    VK_SUBTRACT,
    VK_T,
    VK_TAB,
    VK_U,
    VK_UP,
    VK_V,
    VK_W,
    VK_X,
    VK_Y,
    VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW,
    MSG,
    WM_HOTKEY,
};

use crate::internal::ReturnValue;

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
/// # Result::<(), std::io::Error>::Ok(())
/// ```
#[derive(Clone)]
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
                    let mut message: MaybeUninit<MSG> = MaybeUninit::uninit();
                    let getmsg_result =
                        unsafe { GetMessageW(message.as_mut_ptr(), None, WM_HOTKEY, WM_HOTKEY) };
                    let message = unsafe { message.assume_init() };
                    let to_send = match getmsg_result {
                        BOOL(-1) => Some(Err(io::Error::last_os_error())),
                        BOOL(0) => break, // WM_QUIT
                        _ => {
                            if let Some(user_id) = id_assocs
                                .get(&message.wParam.0.try_into().expect(
                                "ID from GetMessageW should be in range for ID map integer type",
                            )) {
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
                    UnregisterHotKey(None, id).if_null_panic("Cannot unregister hotkey");
                }
            }
        }
    }
}

/// Non-modifier key usable for hotkeys.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u16)]
pub enum Key {
    Backspace = VK_BACK.0,
    Tab = VK_TAB.0,
    Return = VK_RETURN.0,
    Pause = VK_PAUSE.0,
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
}

impl From<Key> for u32 {
    fn from(value: Key) -> Self {
        Self::from(u16::from(value))
    }
}

/// Modifier key than cannot be used by itself for hotkeys.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum Modifier {
    Alt = MOD_ALT.0,
    Ctrl = MOD_CONTROL.0,
    Shift = MOD_SHIFT.0,
    Win = MOD_WIN.0,
}

/// A combination of modifier keys.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ModifierCombination(u32);

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
