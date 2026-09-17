//! SPDX-License-Identifier: Apache-2.0
//! Embed the accent app icon into Windows binaries.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "windows" {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("FileDescription", "WeeChat Radio");
    res.set("ProductName", "WeeChat Radio");
    res.set("LegalCopyright", "Copyright 2026 Thaum Labs");
    res.compile()
        .expect("embed WeeChat Radio icon into Windows exe");
}
