# TCP-Novus

_Memetic network transfer tool._

Completely transparent networking tool for transferring PoD.
Designed to be very very very easy to redistribute and use.

Does not have any form of:
- Encryption
- Compression
- Integrity validation

It doesn't even have a progress bar.




## Building

1. Install the Rust toolchain from https://rustup.rs/
2. With `cargo` now in your path, run `cargo build --release` in this directory
3. Find your newly compiled `tcp-novus` executable in `target/release/`

## Usage

Run `tcp-novus --help` for instructions.

# License

This project is licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or
  https://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `tcp-novus` by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
