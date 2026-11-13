// SPDX-FileCopyrightText: 2026 Antonio De Gaetano <antux912012@users.noreply.github.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Build script that compiles the bundled Papirus icon GResource for Mithrandir.
//!
//! Upstream caller: Cargo build pipeline when the `gui` feature is enabled.
//! Downstream consumers: `src/main.rs` embeds the compiled `mithrandir.gresource` file.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Compiles `data/mithrandir.gresource.xml` into `$OUT_DIR/mithrandir.gresource`.
///
/// # Panics
///
/// Panics if `OUT_DIR` is unset or if `glib-compile-resources` fails when `gui` is enabled.
fn main() {
    println!("cargo:rerun-if-changed=data/mithrandir.gresource.xml");
    println!("cargo:rerun-if-changed=data/icons/papirus");

    if env::var_os("CARGO_FEATURE_GUI").is_none() {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Cargo must set OUT_DIR"));
    let target_file = out_dir.join("mithrandir.gresource");

    let status = Command::new("glib-compile-resources")
        .arg("--sourcedir=data")
        .arg(format!("--target={}", target_file.display()))
        .arg("data/mithrandir.gresource.xml")
        .status()
        .expect("Failed to execute glib-compile-resources");

    assert!(
        status.success(),
        "glib-compile-resources exited with status {status}"
    );
}
