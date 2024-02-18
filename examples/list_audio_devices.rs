use std::io;

use winapi_easy::media::AudioOutputDevice;

fn main() -> io::Result<()> {
    let devices = AudioOutputDevice::get_active_devices()?;
    for device in &devices {
        println!(
            "Friendly name: '{}', ID: {}",
            device.get_friendly_name(),
            device.get_id().to_string_lossy()
        );
    }
    Ok(())
}
