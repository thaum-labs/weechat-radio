//! SPDX-License-Identifier: Apache-2.0
//! Embed the accent app icon into Windows binaries.
//! Copy map page assets when `web/` is in the checkout (hub Docker omits it).

fn main() {
    embed_map_assets();
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

fn embed_map_assets() {
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let web = manifest.join("../../web");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for name in ["index.html", "app.js", "shell.js", "styles.css"] {
        let src = web.join(name);
        println!("cargo:rerun-if-changed={}", src.display());
        let dst = out.join(name);
        if src.is_file() {
            std::fs::copy(&src, &dst).expect("copy map asset into OUT_DIR");
        } else {
            std::fs::write(&dst, []).expect("stub missing map asset");
        }
    }
}
