use std::io;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::input::KeyboardKey;
use winapi_easy::input::hotkey::{
    GlobalHotkeySet,
    Modifier,
};
use winapi_easy::process::{
    IoPriority,
    Process,
};
use winapi_easy::ui::window::WindowHandle;

fn main() -> io::Result<()> {
    #[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Debug)]
    #[repr(u8)]
    enum Action {
        VeryLowPrio,
        NormalPrio,
        #[num_enum(catch_all)]
        Other(u8),
    }

    let listener = |hotkey_id| {
        let prio_target: IoPriority = match Action::from(hotkey_id) {
            Action::VeryLowPrio => IoPriority::VeryLow,
            Action::NormalPrio => IoPriority::Normal,
            Action::Other(_) => unreachable!(),
        };
        let mut foreground_process: Process = WindowHandle::get_foreground_window()
            .unwrap()
            .get_creator_process_id()
            .try_into()?;
        foreground_process.set_io_priority(prio_target)?;
        Ok(())
    };
    let mut hotkeys = GlobalHotkeySet::new();
    hotkeys.add_hotkey(
        Action::VeryLowPrio.into(),
        Modifier::Ctrl + Modifier::Alt + KeyboardKey::PgDown,
    )?;
    hotkeys.add_hotkey(
        Action::NormalPrio.into(),
        Modifier::Ctrl + Modifier::Alt + KeyboardKey::PgUp,
    )?;
    hotkeys.listen_for_hotkeys(listener)?;
    Ok(())
}
