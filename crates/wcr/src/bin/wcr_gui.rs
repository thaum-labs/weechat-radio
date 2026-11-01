//! SPDX-License-Identifier: Apache-2.0
//! Desktop launcher — no console window on Windows.

#![windows_subsystem = "windows"]

fn main() {
    // `wcr-gui -c FILE` (also what headless `wcr -c FILE gui` runs): load,
    // save and start the station on FILE instead of the default wcr.toml.
    if let Some(path) = config_arg(std::env::args_os().skip(1)) {
        wcr::config::Config::set_active_path(&path);
    }
    if let Err(e) = wcr::gui::run() {
        let _ = std::fs::write(
            wcr::config::default_data_dir().join("gui.log"),
            format!("{e}\n"),
        );
        std::process::exit(1);
    }
}

/// The FILE in `-c FILE`, `--config FILE` or `--config=FILE`.
fn config_arg(mut args: impl Iterator<Item = std::ffi::OsString>) -> Option<std::path::PathBuf> {
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        if text == "-c" || text == "--config" {
            return args.next().map(Into::into);
        }
        if let Some(path) = text.strip_prefix("--config=") {
            return Some(path.into());
        }
    }
    None
}
