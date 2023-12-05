# winapi-easy
An ergonomic and safe interface to some Windows API functionality.

[![Latest version](https://img.shields.io/crates/v/winapi-easy)](https://crates.io/crates/winapi-easy)
[![Documentation](https://docs.rs/winapi-easy/badge.svg)](https://docs.rs/winapi-easy)
![License](https://img.shields.io/crates/l/winapi-easy)

## Design

This is an **experimental** library designed to explore how the Windows API could look like if it had these properties:

* Properly typed parameters, making wrong usage of the API difficult
* No unsafe functionality exposed to the user outside of 'escape hatches'
* Consistent error handling without special numerical return values

Expect breaking changes between versions. Any kind of feature completeness is also unrealistic given the huge size
of the Windows API.

## Features

* Add global hotkeys
* Send keystroke combinations
* List threads
* Set CPU priority for any process / thread
* Set IO priority for any process
* Set current process / thread to background processing mode (sets very low IO priority, lowering disk access contention)
* Get toplevel windows
* Get windows associated with a thread
* Window actions like minimize, flash
* Set taskbar progress animation for the console window
* Turn monitor off
* COM initialization
* Get a file list from the Windows clipboard
* Refresh icon cache
* Set default audio output device

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
