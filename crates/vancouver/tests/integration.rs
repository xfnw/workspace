// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    ffi::OsStr,
    process::{Command, Output},
};

static BIN: &str = env!("CARGO_BIN_EXE_vancouver");
static WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");

fn command_output(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

#[test]
fn workspace_check() {
    let output = command_output([
        "check",
        "--lock",
        &format!("{WORKSPACE}Cargo.lock"),
        "--config",
        &format!("{WORKSPACE}vancouver.toml"),
        "--audits",
        &format!("{WORKSPACE}audits.toml"),
    ]);
    dbg!(str::from_utf8(&output.stdout).unwrap());
    dbg!(str::from_utf8(&output.stderr).unwrap());
    assert!(output.status.success());
}
