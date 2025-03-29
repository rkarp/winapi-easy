use std::io;

use winapi_easy::hooking::{
    HookReturnValue,
    LowLevelInputHook,
    LowLevelKeyboardHook,
    LowLevelKeyboardMessage,
    LowLevelMouseAction,
    LowLevelMouseHook,
    LowLevelMouseMessage,
};

fn main() -> io::Result<()> {
    let mouse_thread = std::thread::spawn(|| {
        let callback = |message: LowLevelMouseMessage| -> HookReturnValue {
            match message.action {
                LowLevelMouseAction::Move => {}
                _ => {
                    dbg!(message);
                }
            }
            HookReturnValue::CallNextHook
        };
        LowLevelMouseHook::run_hook(callback)
    });
    let keyboard_thread = std::thread::spawn(|| {
        let callback = |message: LowLevelKeyboardMessage| -> HookReturnValue {
            dbg!(message);
            HookReturnValue::CallNextHook
        };
        LowLevelKeyboardHook::run_hook(callback)
    });
    mouse_thread.join().unwrap()?;
    keyboard_thread.join().unwrap()?;
    Ok(())
}
