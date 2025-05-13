use std::io;

use winapi_easy::hooking::{
    HookReturnValue,
    LowLevelInputHookType,
    LowLevelKeyboardHook,
    LowLevelKeyboardMessage,
    LowLevelMouseAction,
    LowLevelMouseHook,
    LowLevelMouseMessage,
};
use winapi_easy::messaging::ThreadMessageLoop;

fn main() -> io::Result<()> {
    let mouse_callback = |message: LowLevelMouseMessage| -> HookReturnValue {
        match message.action {
            LowLevelMouseAction::Move => {}
            _ => {
                dbg!(message);
            }
        }
        HookReturnValue::CallNextHook
    };
    let _mouse_hook = LowLevelMouseHook::add_hook::<0, _>(mouse_callback)?;

    let keyboard_callback = |message: LowLevelKeyboardMessage| -> HookReturnValue {
        dbg!(message);
        HookReturnValue::CallNextHook
    };
    let _keyboard_hook = LowLevelKeyboardHook::add_hook::<1, _>(keyboard_callback)?;

    ThreadMessageLoop::run_thread_message_loop(|| Ok(()))?;
    Ok(())
}
