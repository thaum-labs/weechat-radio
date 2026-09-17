//! SPDX-License-Identifier: Apache-2.0
//! Desktop launcher — no console window on Windows.

#![windows_subsystem = "windows"]

fn main() {
    if let Err(e) = wcr::gui::run() {
        let _ = std::fs::write(
            wcr::config::default_data_dir().join("gui.log"),
            format!("{e}\n"),
        );
        std::process::exit(1);
    }
}
