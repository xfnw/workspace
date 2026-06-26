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

fn test_exitcode(name: &str, output: &str, code: i32) -> String {
    let output = command_output([
        "check",
        "--manifest",
        &format!("{name}Cargo.toml"),
        "--config",
        &format!("{name}vancouver.toml"),
        "--audits",
        &format!("{name}audits.toml"),
        "--output",
        output,
    ]);
    let stdout = dbg!(str::from_utf8(&output.stdout).unwrap());
    dbg!(str::from_utf8(&output.stderr).unwrap());
    assert_eq!(output.status.code().unwrap(), code);
    stdout.to_string()
}

#[test]
fn workspace_check() {
    assert_eq!(test_exitcode(WORKSPACE, "human", 0), "");
}

#[test]
fn violation() {
    let stdout = test_exitcode(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/violation/"),
        "json",
        1,
    );
    assert_eq!(
        stdout,
        r#"{"dependencies":[{"fails":[{"needed":"meow","prev_version":null,"reason":"Violation"}],"name":"equivalent","status":"failed","version":"1.0.2"},{"fails":[{"needed":"meow","prev_version":"0.2.1","reason":"Violation"}],"name":"try-lock","status":"failed","version":"0.2.4"}],"total":2,"total_failed":2,"total_passed":0,"unused_exempts":[]}
"#
    );
}
