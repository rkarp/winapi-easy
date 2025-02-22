use std::io;

use winapi_easy::input::KeyboardKey;
use winapi_easy::input::hotkeys::{
    GlobalHotkeySet,
    Modifier,
};
use winapi_easy::process::{
    IoPriority,
    Process,
};
use winapi_easy::ui::WindowHandle;

fn main() -> io::Result<()> {
    #[derive(Copy, Clone, Debug)]
    enum Action {
        VeryLowPrio,
        NormalPrio,
    }
    let hotkey_events = GlobalHotkeySet::new()
        .add_hotkey(
            Action::VeryLowPrio,
            Modifier::Ctrl + Modifier::Alt + KeyboardKey::PgDown,
        )
        .add_hotkey(
            Action::NormalPrio,
            Modifier::Ctrl + Modifier::Alt + KeyboardKey::PgUp,
        )
        .listen_for_hotkeys();
    for event in hotkey_events {
        let prio_target: IoPriority = match event? {
            Action::VeryLowPrio => IoPriority::VeryLow,
            Action::NormalPrio => IoPriority::Normal,
        };
        let mut foreground_process: Process = WindowHandle::get_foreground_window()
            .unwrap()
            .get_creator_process_id()
            .try_into()?;
        foreground_process.set_io_priority(prio_target)?;
    }
    Ok(())
}
