// SPDX-License-Identifier: AGPL-3.0-or-later
//! FrameCraft desktop app. For now only prints its version; the Dioxus UI
//! arrives in step 4 of the port.

fn main() {
    println!(
        "{} {}",
        framecraft_core::GENERATOR,
        framecraft_core::VERSION
    );
}
