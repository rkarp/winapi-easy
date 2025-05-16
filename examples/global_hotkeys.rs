use std::io;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::input::hotkey::{
    GlobalHotkeySet,
    Modifier,
};
use winapi_easy::input::{
    GenericKey,
    KeyboardKey,
};
use winapi_easy::ui::lock_workstation;
use winapi_easy::ui::window::{
    MonitorPower,
    WindowHandle,
    WindowShowState,
};

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Debug)]
#[repr(u8)]
enum Action {
    MonitorOff,
    MonitorOffPlusLock,
    VolumeUp,
    VolumeDown,
    #[num_enum(catch_all)]
    Other(u8),
}

fn main() -> io::Result<()> {
    if let Some(console_window) = WindowHandle::get_console_window() {
        console_window.set_show_state(WindowShowState::Minimize)?;
    }
    let listener = |hotkey_id| {
        let monitor_off = || -> io::Result<()> {
            let foreground_window = WindowHandle::get_foreground_window().unwrap();
            foreground_window.set_monitor_power(MonitorPower::Off)
        };
        match Action::from(hotkey_id) {
            Action::MonitorOffPlusLock => {
                lock_workstation()?;
                monitor_off()?;
            }
            Action::MonitorOff => {
                monitor_off()?;
            }
            Action::VolumeUp => {
                KeyboardKey::VolumeUp.press()?;
                KeyboardKey::VolumeUp.release()?;
            }
            Action::VolumeDown => {
                KeyboardKey::VolumeDown.press()?;
                KeyboardKey::VolumeDown.release()?;
            }
            Action::Other(_) => unreachable!(),
        }
        Ok(())
    };
    let mut hotkeys = GlobalHotkeySet::new();
    hotkeys.add_hotkey(
        Action::MonitorOff.into(),
        Modifier::Ctrl + Modifier::Shift + KeyboardKey::Oem1,
    )?;
    hotkeys.add_hotkey(
        Action::MonitorOffPlusLock.into(),
        Modifier::Ctrl + Modifier::Alt + KeyboardKey::Oem1,
    )?;
    hotkeys.add_hotkey(Action::VolumeUp.into(), Modifier::Win + KeyboardKey::PgUp)?;
    hotkeys.add_hotkey(
        Action::VolumeDown.into(),
        Modifier::Win + KeyboardKey::PgDown,
    )?;
    hotkeys.listen_for_hotkeys(listener)?;
    Ok(())
}
