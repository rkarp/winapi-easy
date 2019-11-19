# winapi-easy
An ergonomic and safe interface to some winapi functionality.

[![Latest version](https://img.shields.io/crates/v/winapi-easy)](https://crates.io/crates/winapi-easy)
[![Documentation](https://docs.rs/winapi-easy/badge.svg)](https://docs.rs/winapi-easy)
![License](https://img.shields.io/crates/l/winapi-easy)

## Features

* Add global hotkeys
* List threads
* Set CPU priority for any process / thread
* Set IO priority for any process
* Set current process / thread to background processing mode (sets very low IO priority, lowering disk access contention)
* Get toplevel windows
* Get windows associated with a thread
* Window actions like minimize, flash
* Set taskbar progress animation for the console window
* Turn monitor off
* Lock workstation
* COM initialization
* Get a file list from the Windows clipboard

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
