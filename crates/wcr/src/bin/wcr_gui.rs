//! SPDX-License-Identifier: Apache-2.0
//! Desktop launcher — no console window on Windows.

#![windows_subsystem = "windows"]

fn main() {
    // Before an update can rename this file (see wcr::restart).
    wcr::restart::remember_install_exe();
    // `wcr-gui -c FILE` (also what headless `wcr -c FILE gui` runs): load,
    // save and start the station on FILE instead of the default wcr.toml.
    if let Some(path) = config_arg(std::env::args_os().skip(1)) {
        wcr::config::Config::set_active_path(&path);
    }
    // `--ui-layout modem73` picks the window layout for this run only.
    if let Some(layout) = flag_value(std::env::args_os().skip(1), "--ui-layout") {
        wcr::gui::set_layout_override(&layout);
    }
    if let Err(e) = wcr::gui::run() {
        let _ = std::fs::write(
            wcr::config::default_data_dir().join("gui.log"),
            format!("{e}\n"),
        );
        std::process::exit(1);
    }
}

/// The value in `NAME VALUE` or `NAME=VALUE`.
fn flag_value(mut args: impl Iterator<Item = std::ffi::OsString>, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        if text == name {
            return args.next().map(|v| v.to_string_lossy().into_owned());
        }
        if let Some(v) = text.strip_prefix(&prefix) {
            return Some(v.to_string());
        }
    }
    None
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
