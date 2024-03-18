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
        let mut callback = |message: LowLevelMouseMessage| -> HookReturnValue {
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
        let mut callback = |message: LowLevelKeyboardMessage| -> HookReturnValue {
            dbg!(message);
            HookReturnValue::CallNextHook
        };
        LowLevelKeyboardHook::run_hook(&mut callback)
    });
    mouse_thread.join().unwrap()?;
    keyboard_thread.join().unwrap()?;
    Ok(())
}
