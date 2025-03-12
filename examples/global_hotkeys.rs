use std::io;

use winapi_easy::input::{
    GenericKey,
    KeyboardKey,
};
use winapi_easy::input::hotkeys::{
    GlobalHotkeySet,
    Modifier,
};
use winapi_easy::ui::{
    MonitorPower,
    WindowHandle,
    WindowShowState,
    lock_workstation,
};

#[derive(Copy, Clone, Debug)]
enum Action {
    MonitorOff,
    MonitorOffPlusLock,
    VolumeUp,
    VolumeDown,
}

fn main() -> io::Result<()> {
    if let Some(console_window) = WindowHandle::get_console_window() {
        console_window.set_show_state(WindowShowState::Minimize)?;
    }
    let hotkey_def = GlobalHotkeySet::new()
        .add_hotkey(
            Action::MonitorOff,
            Modifier::Ctrl + Modifier::Shift + KeyboardKey::Oem1,
        )
        .add_hotkey(
            Action::MonitorOffPlusLock,
            Modifier::Ctrl + Modifier::Alt + KeyboardKey::Oem1,
        )
        .add_hotkey(Action::VolumeUp, Modifier::Win + KeyboardKey::PgUp)
        .add_hotkey(Action::VolumeDown, Modifier::Win + KeyboardKey::PgDown);
    for event in hotkey_def.listen_for_hotkeys()? {
        let monitor_off = || -> io::Result<()> {
            let foreground_window = WindowHandle::get_foreground_window().unwrap();
            foreground_window.set_monitor_power(MonitorPower::Off)
        };
        match event? {
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
        }
    }
    Ok(())
}
