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

fn test_success(name: &str, should_succeed: bool) {
    let output = command_output([
        "check",
        "--lock",
        &format!("{name}Cargo.lock"),
        "--config",
        &format!("{name}vancouver.toml"),
        "--audits",
        &format!("{name}audits.toml"),
    ]);
    dbg!(str::from_utf8(&output.stdout).unwrap());
    dbg!(str::from_utf8(&output.stderr).unwrap());
    assert_eq!(output.status.success(), should_succeed);
}

#[test]
fn workspace_check() {
    test_success(WORKSPACE, true);
}
