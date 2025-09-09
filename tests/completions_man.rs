use std::fs;

#[test]
fn generates_bash_completions_into_out_dir() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    let out_dir = temp.path();

    let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
    cmd.args(["completions", "bash", "-o", out_dir.to_str().unwrap()]);
    cmd.assert().success();

    let bash_path = out_dir.join("svg_sheet.bash");
    assert!(bash_path.exists(), "expected {:?} to exist", bash_path);
    let contents = fs::read(&bash_path).expect("read completion file");
    assert!(!contents.is_empty(), "completion file should be non-empty");

    temp.close().unwrap();
}

#[test]
fn generates_man_page_into_out_dir() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    let out_dir = temp.path();

    let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
    cmd.args(["man", "-o", out_dir.to_str().unwrap()]);
    cmd.assert().success();

    let man_path = out_dir.join("svg_sheet.1");
    assert!(man_path.exists(), "expected {:?} to exist", man_path);
    let contents = fs::read(&man_path).expect("read man file");
    assert!(!contents.is_empty(), "man file should be non-empty");

    temp.close().unwrap();
}
