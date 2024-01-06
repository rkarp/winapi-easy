use std::io;

use winapi_easy::input::hooking::{
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
        let mut callback = |message: LowLevelMouseMessage, _| -> HookReturnValue {
            match message.action {
                LowLevelMouseAction::Move => {}
                _ => {
                    dbg!(message);
                }
            }
            HookReturnValue::CallNextHook
        };
        LowLevelMouseHook::run_hook(&mut callback)
    });
    let keyboard_thread = std::thread::spawn(|| {
        let mut callback = |message: LowLevelKeyboardMessage, _| -> HookReturnValue {
            dbg!(message);
            HookReturnValue::CallNextHook
        };
        LowLevelKeyboardHook::run_hook(&mut callback)
    });
    mouse_thread.join().unwrap()?;
    keyboard_thread.join().unwrap()?;
    Ok(())
}
