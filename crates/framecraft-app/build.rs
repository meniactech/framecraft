// SPDX-License-Identifier: AGPL-3.0-or-later
//! Windows: puts the app icon into FrameCraft.exe (Explorer, taskbar,
//! Alt+Tab). Other systems take it from their packages.

fn main() {
    println!("cargo:rerun-if-changed=framecraft.rc");
    println!("cargo:rerun-if-changed=../../packaging/icons/framecraft.ico");
    #[cfg(windows)]
    embed_resource::compile("framecraft.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("the app icon resource compiles");
}
