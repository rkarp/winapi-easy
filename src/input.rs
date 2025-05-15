//! Keyboard and hotkeys.

use std::ffi::c_void;
use std::{
    io,
    mem,
};

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
#[allow(clippy::wildcard_imports)]
use private::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState,
    GetKeyState,
    INPUT,
    INPUT_0,
    INPUT_KEYBOARD,
    INPUT_MOUSE,
    KEYBDINPUT,
    KEYEVENTF_KEYUP,
    MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP,
    MOUSEINPUT,
    SendInput,
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
    VK_F2,
    VK_F3,
    VK_F4,
    VK_F5,
    VK_F6,
    VK_F7,
    VK_F8,
    VK_F9,
    VK_F10,
    VK_F11,
    VK_F12,
    VK_G,
    VK_H,
    VK_HOME,
    VK_I,
    VK_INSERT,
    VK_J,
    VK_K,
    VK_L,
    VK_LBUTTON,
    VK_LCONTROL,
    VK_LEFT,
    VK_LMENU,
    VK_LSHIFT,
    VK_LWIN,
    VK_M,
    VK_MBUTTON,
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
    VK_OEM_2,
    VK_OEM_3,
    VK_OEM_4,
    VK_OEM_5,
    VK_OEM_6,
    VK_OEM_7,
    VK_OEM_8,
    VK_OEM_102,
    VK_OEM_COMMA,
    VK_OEM_MINUS,
    VK_OEM_PERIOD,
    VK_OEM_PLUS,
    VK_P,
    VK_PAUSE,
    VK_PRIOR,
    VK_Q,
    VK_R,
    VK_RBUTTON,
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
    VK_XBUTTON1,
    VK_XBUTTON2,
    VK_Y,
    VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETMOUSESPEED,
    SPI_SETMOUSESPEED,
    SPIF_SENDCHANGE,
    SPIF_UPDATEINIFILE,
    SystemParametersInfoW,
    WHEEL_DELTA,
    XBUTTON1,
    XBUTTON2,
};

use crate::internal::ReturnValue;
#[rustversion::before(1.87)]
use crate::internal::std_unstable::CastUnsigned;

pub mod hotkey;

/// A [`KeyboardKey`] or a [`MouseButton`].
pub trait GenericKey: GenericKeyInternal {
    fn is_pressed(self) -> io::Result<bool> {
        let result = unsafe {
            GetAsyncKeyState(self.into())
                .if_null_to_error(|| io::ErrorKind::PermissionDenied.into())?
        };
        Ok(result.cast_unsigned() >> (u16::BITS - 1) == 1)
    }

    /// Globally sends a 'press' event (without a corresponding 'release').
    ///
    /// This can conflict with existing user key presses. Use [`Self::is_pressed`] to avoid this.
    fn press(self) -> io::Result<()> {
        self.send_input(false)
    }

    /// Globally sends a 'release' event.
    fn release(self) -> io::Result<()> {
        self.send_input(true)
    }

    /// Globally sends a key (or mouse button) combination as if the user had performed it.
    ///
    /// This will cause a 'press' event for each key in the list (in the given order),
    /// followed by a sequence of 'release' events (in the inverse order).
    fn send_combination(keys: &[Self]) -> io::Result<()> {
        let raw_input_pairs: Vec<_> = keys
            .iter()
            .copied()
            .map(|key: Self| {
                let raw_input = key.get_press_raw_input(false);
                let raw_input_release = key.get_press_raw_input(true);
                (raw_input, raw_input_release)
            })
            .collect();
        let raw_inputs: Vec<_> = raw_input_pairs
            .iter()
            .map(|x| x.0)
            .chain(raw_input_pairs.iter().rev().map(|x| x.1))
            .collect();
        send_raw_inputs(raw_inputs.as_slice())
    }
}

// No generic impl to generate better docs
impl GenericKey for KeyboardKey {}
impl GenericKey for MouseButton {}

mod private {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub trait GenericKeyInternal: Copy + Into<i32> {
        fn send_input(self, is_release: bool) -> io::Result<()> {
            let raw_input = self.get_press_raw_input(is_release);
            send_raw_inputs(&[raw_input])
        }
        fn get_press_raw_input(self, is_release: bool) -> INPUT;
    }

    impl GenericKeyInternal for KeyboardKey {
        fn get_press_raw_input(self, is_release: bool) -> INPUT {
            let raw_key: u16 = self.into();
            let raw_keybdinput = KEYBDINPUT {
                wVk: VIRTUAL_KEY(raw_key),
                dwFlags: if is_release {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            };
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 { ki: raw_keybdinput },
            }
        }
    }

    impl GenericKeyInternal for MouseButton {
        fn get_press_raw_input(self, is_release: bool) -> INPUT {
            let (flags, mouse_data) = match (self, is_release) {
                (MouseButton::Left, false) => (MOUSEEVENTF_LEFTDOWN, 0),
                (MouseButton::Left, true) => (MOUSEEVENTF_LEFTUP, 0),
                (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTDOWN, 0),
                (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTUP, 0),
                (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEUP, 0),
                (MouseButton::X1, false) => (MOUSEEVENTF_XDOWN, XBUTTON1),
                (MouseButton::X1, true) => (MOUSEEVENTF_XUP, XBUTTON1),
                (MouseButton::X2, false) => (MOUSEEVENTF_XDOWN, XBUTTON2),
                (MouseButton::X2, true) => (MOUSEEVENTF_XUP, XBUTTON2),
            };
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        mouseData: mouse_data.into(),
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            }
        }
    }
}

/// Keyboard key with a virtual key code, usable for hotkeys.
///
/// # Related docs
///
/// [Microsoft docs for virtual key codes](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes)
#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u16)]
pub enum KeyboardKey {
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
    /// * For the US standard keyboard, the '\`~' key
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
    /// * For the German keyboard, the '´\`' key
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

impl KeyboardKey {
    /// Returns true if the key has lock functionality (e.g. Caps Lock) and the lock is toggled.
    pub fn is_lock_toggled(self) -> bool {
        let result = unsafe { GetKeyState(self.into()).cast_unsigned() };
        result & 1 == 1
    }
}

impl From<KeyboardKey> for u32 {
    fn from(value: KeyboardKey) -> Self {
        Self::from(u16::from(value))
    }
}

impl From<KeyboardKey> for i32 {
    fn from(value: KeyboardKey) -> Self {
        u16::from(value).into()
    }
}

fn send_raw_inputs(raw_inputs: &[INPUT]) -> io::Result<()> {
    let raw_input_size = mem::size_of::<INPUT>()
        .try_into()
        .expect("Struct size conversion failed");

    let expected_sent_size =
        u32::try_from(raw_inputs.len()).expect("Inputs length conversion failed");
    unsafe {
        SendInput(raw_inputs, raw_input_size)
            .if_null_get_last_error()?
            .if_not_eq_to_error(expected_sent_size, || {
                io::Error::from(io::ErrorKind::Interrupted)
            })?;
    }
    Ok(())
}

/// Mouse button.
///
/// Note that X-Buttons above #2 are only handled by the mouse driver.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u16)]
pub enum MouseButton {
    Left = VK_LBUTTON.0,
    Right = VK_RBUTTON.0,
    Middle = VK_MBUTTON.0,
    X1 = VK_XBUTTON1.0,
    X2 = VK_XBUTTON2.0,
}

impl From<MouseButton> for i32 {
    fn from(value: MouseButton) -> Self {
        u16::from(value).into()
    }
}

/// Mouse scroll wheel 'up' or 'down' event, possibly continuous.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MouseScrollEvent {
    /// One or more full up-scroll events.
    ///
    /// Equivalent to [`Self::Continuous`] with a multiple of [`Self::WHEEL_DELTA`].
    Up(u16),
    /// One or more full down-scroll events.
    ///
    /// Equivalent to [`Self::Continuous`] with a multiple of -[`Self::WHEEL_DELTA`].
    Down(u16),
    /// Continuous 'up' (positive value) or 'down' (negative value) scroll event.
    ///
    /// Values other than multiples of positive or negative [`Self::WHEEL_DELTA`] are used for mouses
    /// with continuous scroll wheels.
    Continuous(i16),
}

impl MouseScrollEvent {
    #[allow(clippy::cast_possible_truncation)]
    pub const WHEEL_DELTA: i16 = WHEEL_DELTA as _;

    /// Globally sends a single scroll event.
    pub fn send(self) -> io::Result<()> {
        // Should never overflow due to data types
        let mouse_data: i32 = match self {
            MouseScrollEvent::Up(amount) => i32::from(Self::WHEEL_DELTA) * i32::from(amount),
            MouseScrollEvent::Down(amount) => -i32::from(Self::WHEEL_DELTA) * i32::from(amount),
            MouseScrollEvent::Continuous(delta) => i32::from(delta),
        };
        let raw_input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    // bit-cast semantics necessary here because negative values should be allowed
                    mouseData: mouse_data.cast_unsigned(),
                    dwFlags: MOUSEEVENTF_WHEEL,
                    ..Default::default()
                },
            },
        };
        send_raw_inputs(&[raw_input])
    }

    #[cfg(feature = "hooking")]
    pub(crate) fn from_raw_movement(raw_movement: i16) -> Self {
        if raw_movement % Self::WHEEL_DELTA != 0 {
            MouseScrollEvent::Continuous(raw_movement)
        } else if raw_movement > 0 {
            MouseScrollEvent::Up((raw_movement / Self::WHEEL_DELTA).cast_unsigned())
        } else {
            MouseScrollEvent::Down((-raw_movement / Self::WHEEL_DELTA).cast_unsigned())
        }
    }
}

/// Returns the global mouse speed.
pub fn get_mouse_speed() -> io::Result<u32> {
    let mut speed: u32 = 0;
    unsafe {
        SystemParametersInfoW(
            SPI_GETMOUSESPEED,
            0,
            Some((&raw mut speed).cast::<c_void>()),
            Default::default(),
        )?;
    }
    Ok(speed)
}

/// Globally sets the mouse speed.
///
/// Valid values are `1` to `20` inclusive. The change can be persisted between login sessions.
pub fn set_mouse_speed(speed: u32, persist: bool) -> io::Result<()> {
    let flags = if persist {
        SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
    } else {
        SPIF_SENDCHANGE
    };
    unsafe {
        SystemParametersInfoW(
            SPI_SETMOUSESPEED,
            0,
            Some(std::ptr::with_exposed_provenance_mut(
                usize::try_from(speed).unwrap_or_else(|_| unreachable!()),
            )),
            flags,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_get_mouse_speed() -> io::Result<()> {
        let speed = get_mouse_speed()?;
        dbg!(speed);
        assert!((1..=20).contains(&speed));
        Ok(())
    }
}
