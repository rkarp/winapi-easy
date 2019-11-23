use std::io;

use winapi_easy::keyboard::{
    GlobalHotkeySet,
    Key,
    Modifier,
};
use winapi_easy::ui::{
    lock_workstation,
    MonitorPower,
    Window,
    WindowAction,
};

#[derive(Copy, Clone)]
enum Action {
    MonitorOff,
    MonitorOffPlusLock,
}

fn main() -> io::Result<()> {
    if let Some(mut console_window) = Window::get_console_window() {
        console_window.perform_action(WindowAction::Minimize)?;
    }
    let hotkey_def = GlobalHotkeySet::new()
        .add_global_hotkey(
            Action::MonitorOff,
            Modifier::Ctrl + Modifier::Shift + Key::Oem3,
        )
        .add_global_hotkey(
            Action::MonitorOffPlusLock,
            Modifier::Ctrl + Modifier::Alt + Key::Oem3,
        );
    for event in hotkey_def.listen_for_hotkeys()? {
        match event? {
            Action::MonitorOffPlusLock => lock_workstation()?,
            Action::MonitorOff => (),
        }
        let mut foreground_window = Window::get_foreground_window().unwrap();
        foreground_window.set_monitor_power(MonitorPower::Off)?;
    }
    Ok(())
}
