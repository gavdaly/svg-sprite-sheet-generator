use assert_cmd::cargo::CommandCargoExt;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::fs;

// A comprehensive end-to-end test that exercises:
// - Preprocessing (BOM, XML prolog, leading comment)
// - Root width/height and viewBox normalization
// - Root id moved to data-id; id not present
// - Children preserved verbatim; nested <svg> untouched
// - Successful sprite generation with multiple files
// - Error handling: duplicate child id across files
// - Error handling: root id referenced internally
// - Error handling: invalid dimension and invalid viewBox
#[test]
fn end_to_end_sprite_generation_and_validations() {
    let temp = assert_fs::TempDir::new().expect("tempdir");
    let dir = temp.path();

    // a.svg includes BOM, XML prolog, comment, normalized dims, and a root id
    let a_svg = format!(
        "{}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- lead -->\n<svg id=\"RootA\" width=\"24px\" height=\"24.0\" viewBox=\"0,0,24,24\">\n  <g id=\"icon-a\"><path d=\"M0 0h24v24H0z\" fill=\"none\"/></g>\n</svg>\n",
        '\u{feff}'
    );
    temp.child("a.svg").write_str(&a_svg).unwrap();

    // b.svg has nested <svg> and no root width/height; nested attrs should remain untouched
    let b_svg = r#"
<svg viewBox="0 0 12 12">
  <g id="icon-b"><circle cx="6" cy="6" r="5"/></g>
  <svg id="nested" width="2px" height="2px"><rect width="2px" height="2px"/></svg>
</svg>
"#;
    temp.child("b.svg").write_str(b_svg).unwrap();

    let out_path = dir.join("sprite.svg");

    // Run: success case
    {
        let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
        cmd.args([
            "-d",
            dir.to_str().unwrap(),
            "-f",
            out_path.to_str().unwrap(),
            "build",
        ]);
        cmd.assert().success();

        // Validate output sprite
        let sprite = fs::read_to_string(&out_path).expect("read sprite");
        // a.svg becomes pattern id="a" with normalized attributes
        assert!(sprite.contains("<pattern id=\"a\""), "missing pattern for a");
        assert!(sprite.contains("width=\"24\""), "width not normalized to 24");
        assert!(sprite.contains("height=\"24\""), "height not normalized to 24");
        assert!(
            sprite.contains("viewBox=\"0 0 24 24\""),
            "viewBox not normalized to spaces"
        );
        // Root id moved to data-id; original id not present on output
        assert!(sprite.contains("data-id=\"RootA\""), "missing data-id=RootA");
        assert!(
            !sprite.contains(" id=\"RootA\""),
            "root id should not be preserved"
        );
        // Children ids preserved and referenceable
        assert!(sprite.contains("id=\"icon-a\""), "missing child id icon-a");
        assert!(sprite.contains("id=\"icon-b\""), "missing child id icon-b");
        // Nested <svg> width/height should remain with px, proving we didn't normalize nested elements
        assert!(
            sprite.contains("<svg id=\"nested\" width=\"2px\" height=\"2px\">"),
            "nested svg attributes should remain untouched"
        );
    }

    // Introduce a duplicate id across files (duplicate icon-b)
    let c_svg_dup = r#"
<svg >
  <g id="icon-b"><path d="M0 0h1v1H0z"/></g>
</svg>
"#;
    temp.child("c.svg").write_str(c_svg_dup).unwrap();
    {
        let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
        cmd.args([
            "-d",
            dir.to_str().unwrap(),
            "-f",
            out_path.to_str().unwrap(),
            "build",
        ]);
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("duplicate id"));
    }

    // Replace c.svg with a root id that is referenced internally (should fail)
    let c_svg_root_ref = r##"
<svg id="root">
  <use href="#root"/>
</svg>
"##;
    temp.child("c.svg").write_str(c_svg_root_ref).unwrap();
    {
        let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
        cmd.args([
            "-d",
            dir.to_str().unwrap(),
            "-f",
            out_path.to_str().unwrap(),
            "build",
        ]);
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("root <svg> id"));
    }

    // Replace a.svg with invalid dimension and invalid viewBox to assert both errors
    let a_svg_bad_dim = r#"
<svg width="0" height="10" viewBox="0 0 10 10"></svg>
"#;
    temp.child("a.svg").write_str(a_svg_bad_dim).unwrap();
    {
        let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
        cmd.args([
            "-d",
            dir.to_str().unwrap(),
            "-f",
            out_path.to_str().unwrap(),
            "build",
        ]);
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("invalid width"));
    }

    // Now bad viewBox (non-positive width)
    let a_svg_bad_vb = r#"
<svg viewBox="0 0 0 10"></svg>
"#;
    temp.child("a.svg").write_str(a_svg_bad_vb).unwrap();
    {
        let mut cmd = assert_cmd::Command::cargo_bin("svg_sheet").expect("binary");
        cmd.args([
            "-d",
            dir.to_str().unwrap(),
            "-f",
            out_path.to_str().unwrap(),
            "build",
        ]);
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("invalid viewBox"));
    }

    temp.close().unwrap();
}
