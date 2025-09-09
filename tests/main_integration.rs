use assert_fs::prelude::*;
use predicates::prelude::*;
use std::fs;

#[test]
fn default_run_uses_defaults_and_writes_sprite() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    let svgs = temp.child("svgs");
    svgs.create_dir_all().unwrap();
    svgs.child("a.svg")
        .write_str("<svg width=\"1\" height=\"1\"><g/></svg>")
        .unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
    cmd.current_dir(temp.path());
    cmd.assert().success();

    let sprite = temp.path().join("sprite.svg");
    assert!(sprite.exists(), "sprite.svg should be created at CWD");
    let contents = fs::read_to_string(sprite).expect("read sprite");
    assert!(contents.contains("pattern id=\"a\""));

    temp.close().unwrap();
}

#[test]
fn no_svgs_in_default_dir_exits_nonzero() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    temp.child("svgs").create_dir_all().unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
    cmd.current_dir(temp.path());
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("no SVG files"));

    temp.close().unwrap();
}

#[test]
fn dry_run_does_not_write_output() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    let svgs = temp.child("svgs");
    svgs.create_dir_all().unwrap();
    svgs.child("a.svg")
        .write_str("<svg width=\"1\" height=\"1\"><g/></svg>")
        .unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
    cmd.current_dir(temp.path());
    cmd.arg("--dry-run");
    cmd.assert().success();

    assert!(
        !temp.path().join("sprite.svg").exists(),
        "dry run should not write sprite.svg"
    );
    temp.close().unwrap();
}

#[test]
fn nonexistent_input_dir_error() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
    cmd.current_dir(temp.path());
    cmd.args(["-d", "nope"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("failed to read directory"));
    temp.close().unwrap();
}

#[test]
fn fail_on_warn_exits_nonzero() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    let svgs = temp.child("svgs");
    svgs.create_dir_all().unwrap();
    // Missing width/height/viewBox will produce warnings
    svgs.child("w.svg").write_str("<svg ><g/></svg>").unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
    cmd.current_dir(temp.path());
    cmd.arg("--fail-on-warn");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("aborting due to"));

    temp.close().unwrap();
}
