use std::{path::Path, process::Command};

static BIN: &str = env!("CARGO_BIN_EXE_vasm");
static DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

fn snapshot(name: &str, h: u16) {
    let testname = Path::new(DATA_DIR).join(name);
    let testdata = testname.with_extension("asm");
    let output = Command::new(BIN)
        .arg("--h16")
        .arg(format!("{h:x}"))
        .arg(&testdata)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let output = String::from_utf8(output.stdout).unwrap();
    let mut lines = output.lines();
    let sample = std::fs::read_to_string(testname.with_extension("h16")).unwrap();

    for (n, sl) in sample.lines().enumerate() {
        assert_eq!(lines.next().unwrap(), sl, "line {}", n + 1);
    }

    assert_eq!(lines.next(), None);
}

#[test]
fn chal1() {
    snapshot("chal1", 0);
}

#[test]
fn hwrite() {
    snapshot("hwrite", 0);
}

#[test]
fn uninit() {
    snapshot("uninit", 0xfffe);
}
