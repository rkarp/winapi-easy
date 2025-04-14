use std::io;

use winapi_easy::hooking::{
    WinEventHook,
    WinEventMessage,
};

fn main() -> io::Result<()> {
    let hook_thread = std::thread::spawn(|| {
        let callback = |message: WinEventMessage| {
            match message.event_kind {
                winapi_easy::hooking::WinEventKind::ForegroundWindowChanged => {
                    println!(
                        "Foreground window changed to: '{}'",
                        message.window_handle.unwrap().get_caption_text()
                    )
                }
                winapi_easy::hooking::WinEventKind::WindowUnminimized => {
                    println!(
                        "Window unminimized: '{}'",
                        message.window_handle.unwrap().get_caption_text()
                    )
                }
                _ => (),
            };
        };
        WinEventHook::run_hook(callback)
    });
    hook_thread.join().unwrap()?;
    Ok(())
}
