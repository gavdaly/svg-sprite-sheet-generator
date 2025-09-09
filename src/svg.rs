use crate::error::AppError;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use winnow::Parser;

mod parsing;
mod ids;
mod sanitize;
mod normalize;
mod transform;

/// A struct to represent a SVG file
struct SvgSprite {
    /// The name of the SVG file
    name: String,
    /// The attributes of the svg tag
    attributes: Vec<(String, String)>,
    /// The children of the svg tag
    children: String,
}

impl SvgSprite {
    #[cfg(test)]
    pub fn new(name: String, attributes: Vec<(&str, &str)>, children: String) -> Self {
        let attributes = attributes
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        SvgSprite {
            name,
            attributes,
            children,
        }
    }
}

/// Parse SVG file and return a SvgSprite struct
pub fn process(directory: &str, file: &str) -> Result<(), AppError> {
    let svgs = load_svgs(directory)?;
    if svgs.is_empty() {
        return Err(AppError::NoSvgFiles {
            path: directory.to_string(),
        });
    }
    let sprite = transform::transform(svgs);
    write_sprite(&sprite, file)?;
    Ok(())
}

/// Watch a directory for changes and rebuild the sprite when inputs change.
pub fn watch(directory: &str, file: &str) -> Result<(), AppError> {
    println!("Watching '{directory}' -> '{file}' (Ctrl+C to stop)");
    // Initial build
    if let Err(e) = process(directory, file) {
        eprintln!("Initial build failed: {e}");
        if let Some(src) = std::error::Error::source(&e) {
            eprintln!("Caused by: {src}");
        }
    } else {
        println!("Initial build completed");
    }

    let mut last: Option<u64> = None;
    loop {
        let state = dir_state_hash(directory)?;
        if last.as_ref().is_none_or(|l| *l != state) {
            match process(directory, file) {
                Ok(()) => println!("Rebuilt sprite at {:?}", SystemTime::now()),
                Err(e) => {
                    eprintln!("Rebuild failed: {e}");
                    if let Some(src) = std::error::Error::source(&e) {
                        eprintln!("Caused by: {src}");
                    }
                }
            }
            last = Some(state);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn dir_state_hash(directory: &str) -> Result<u64, AppError> {
    let entries = std::fs::read_dir(directory).map_err(|e| AppError::ReadDir {
        path: directory.to_string(),
        source: e,
    })?;
    let mut hasher = DefaultHasher::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let _path = entry.path();
        let file_name = entry.file_name();
        let Ok(name_str) = file_name.into_string() else {
            continue;
        };
        if !name_str.ends_with(".svg") {
            continue;
        }
        name_str.hash(&mut hasher);
        if let Ok(md) = entry.metadata() {
            md.len().hash(&mut hasher);
            if let Ok(modified) = md.modified() {
                hash_time(&modified, &mut hasher);
            }
        }
    }
    Ok(hasher.finish())
}

fn hash_time(t: &SystemTime, hasher: &mut DefaultHasher) {
    if let Ok(dur) = t.duration_since(UNIX_EPOCH) {
        dur.as_secs().hash(hasher);
        dur.subsec_nanos().hash(hasher);
    }
}
/// Loads all the svg files in the directory
fn load_svgs(directory: &str) -> Result<Vec<SvgSprite>, AppError> {
    let entries = std::fs::read_dir(directory).map_err(|e| AppError::ReadDir {
        path: directory.to_string(),
        source: e,
    })?;

    let mut sprites = Vec::new();
    // Global registry of ids to detect duplicates across all inputs
    let mut id_registry: HashMap<String, String> = HashMap::new(); // id -> first_path
    for entry in entries {
        let entry = entry.map_err(|e| AppError::ReadDir {
            path: directory.to_string(),
            source: e,
        })?;
        let path = entry.path();
        let file_name = entry.file_name();
        let Ok(name_str) = file_name.into_string() else {
            continue;
        };
        if !name_str.ends_with(".svg") {
            continue;
        }
        let name = name_str.trim_end_matches(".svg").to_string();
        let content = std::fs::read_to_string(&path).map_err(|e| AppError::ReadFile {
            path: path.display().to_string(),
            source: e,
        })?;
        let pre = preprocess_svg_content(&content);
        let mut s = pre.as_str();
        match parsing::parse_svg.parse_next(&mut s) {
            Ok((attributes, children)) => {
                // Convert attributes and handle root <svg id> policy: move id -> data-id after sanitization
                let mut out_attrs: Vec<(String, String)> = Vec::new();
                let mut root_id_raw: Option<&str> = None;
                let mut pending_viewbox: Option<String> = None;
                for (k, v) in &attributes {
                    if *k == "id" {
                        root_id_raw = Some(v);
                    } else if *k == "width" || *k == "height" {
                        // Validate and normalize positive numeric width/height, allow optional 'px'
                        match normalize::normalize_length(v) {
                            Some(nv) => out_attrs.push(((*k).to_string(), nv)),
                            None => {
                                return Err(AppError::InvalidDimension {
                                    path: path.display().to_string(),
                                    attr: (*k).to_string(),
                                    value: (*v).to_string(),
                                });
                            }
                        }
                    } else if *k == "viewBox" {
                        match normalize::normalize_viewbox(v) {
                            Some(vb) => pending_viewbox = Some(vb),
                            None => {
                                return Err(AppError::InvalidViewBox {
                                    path: path.display().to_string(),
                                    value: (*v).to_string(),
                                });
                            }
                        }
                    } else {
                        out_attrs.push(((*k).to_string(), (*v).to_string()));
                    }
                }

                if let Some(idv) = root_id_raw {
                    let sanitized = sanitize::sanitize_id(idv);
                    if sanitized.is_empty() {
                        return Err(AppError::InvalidIdAfterSanitize {
                            path: path.display().to_string(),
                            original: idv.to_string(),
                        });
                    }
                    // Check if root id is referenced internally
                    if ids::references_id(children, idv) {
                        return Err(AppError::RootIdReferenced {
                            path: path.display().to_string(),
                            id: idv.to_string(),
                        });
                    }
                    out_attrs.push(("data-id".to_string(), sanitized));
                }

                if let Some(vb) = pending_viewbox {
                    out_attrs.push(("viewBox".to_string(), vb));
                }

                // Scan children for element ids and detect collisions across files
                let child_ids = ids::extract_ids(children);
                for cid in child_ids {
                    if let Some(first) = id_registry.get(&cid) {
                        return Err(AppError::IdCollision {
                            id: cid,
                            first_path: first.clone(),
                            second_path: path.display().to_string(),
                        });
                    } else {
                        id_registry.insert(cid, path.display().to_string());
                    }
                }

                sprites.push(SvgSprite {
                    name,
                    attributes: out_attrs,
                    children: children.to_string(),
                });
            }
            Err(e) => {
                let p = path.display().to_string();
                return Err(AppError::ParseSvg {
                    path: p,
                    message: format!("{e:?}"),
                });
            }
        }
    }
    Ok(sprites)
}

/// Write the sprite to a file
fn write_sprite(sprite: &str, file: &str) -> Result<(), AppError> {
    std::fs::write(file, sprite).map_err(|e| AppError::WriteFile {
        path: file.to_string(),
        source: e,
    })
}

// Strip BOM, leading XML prolog, and comments before the root <svg> tag
fn preprocess_svg_content(input: &str) -> String {
    let mut s = input.trim_start_matches('\u{feff}');
    // Iteratively skip whitespace + XML declarations or comments before <svg
    loop {
        let trimmed = s.trim_start();
        if trimmed.starts_with("<?") {
            // Skip until '?>'
            if let Some(end) = trimmed.find("?>") {
                s = &trimmed[end + 2..];
                continue;
            }
        } else if trimmed.starts_with("<!--") {
            if let Some(end) = trimmed.find("-->") {
                s = &trimmed[end + 3..];
                continue;
            }
        }
        // If we see neither, stop
        s = trimmed;
        break;
    }
    s.to_string()
}

// Sanitize an id by dropping leading invalid chars and replacing internal
// invalid chars with '-'. Collapse multiple '-' and trim them at ends.
// Allowed pattern: [A-Za-z_][A-Za-z0-9._-]*
// moved: sanitize_id, references_id, extract_ids in submodules

// Parse and normalize positive length values for width/height.
// Accepts unitless or 'px' suffix. Returns normalized string (e.g., "24").
// moved: normalize_* functions in svg::normalize

// sprite rendering moved to svg::transform

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::{fs, path::PathBuf};

    // Simple temp dir guard to keep tests isolated
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(prefix: &str) -> Self {
            let mut p = std::env::temp_dir();
            let unique = format!("{}_{}", prefix, std::process::id());
            p.push(unique);
            // Best-effort cleanup if it already exists
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    

    

    #[test]
    fn transform_emits_pattern_per_file() {
        let svgs = vec![
            SvgSprite::new(
                "one".into(),
                vec![("width", "24"), ("height", "24")],
                "<g/>".into(),
            ),
            SvgSprite::new("two".into(), vec![("fill", "#000")], "<circle/>".into()),
        ];
        let out = crate::svg::transform::transform(svgs);
        assert!(out.starts_with("<svg"));
        assert!(out.contains("<defs>"));
        assert!(out.contains("<pattern id=\"one\" width=\"24\" height=\"24\"><g/>"));
        assert!(out.contains("<pattern id=\"two\" fill=\"#000\"><circle/>"));
        assert!(out.ends_with("</defs></svg>"));
    }

    #[test]
    fn process_empty_directory_yields_error() {
        let tmp = TempDir::new("svg_sheet_empty");
        let err = process(
            tmp.path().to_str().unwrap(),
            &tmp.path().join("out.svg").display().to_string(),
        )
        .expect_err("expected error");
        match err {
            AppError::NoSvgFiles { .. } => {}
            _ => panic!("wrong error: {err}"),
        }
    }

    #[test]
    fn load_and_build_from_real_files() {
        let tmp = TempDir::new("svg_sheet_build");
        let dir = tmp.path();
        // Create a couple of minimal SVGs
        fs::write(
            dir.join("a.svg"),
            "<svg width=\"10\" height=\"10\"><rect/></svg>",
        )
        .unwrap();
        fs::write(dir.join("b.svg"), "<svg id=\"b\"><g/></svg>").unwrap();
        let out = dir.join("sprite.svg");
        process(dir.to_str().unwrap(), out.to_str().unwrap()).expect("build ok");
        let sprite = fs::read_to_string(&out).expect("read sprite");
        assert!(sprite.contains("pattern id=\"a\""));
        assert!(sprite.contains("pattern id=\"b\""));
    }

    #[test]
    fn dir_state_hash_changes_on_update() {
        let tmp = TempDir::new("svg_sheet_hash");
        let dir = tmp.path();
        fs::write(dir.join("c.svg"), "<svg id=\"c\"></svg>").unwrap();
        let h1 = dir_state_hash(dir.to_str().unwrap()).expect("hash1");
        // Touch file update
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(dir.join("c.svg"), "<svg id=\"c2\"></svg>").unwrap();
        let h2 = dir_state_hash(dir.to_str().unwrap()).expect("hash2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn sanitize_id_drops_leading_and_replaces_invalids() {
        assert_eq!(crate::svg::sanitize::sanitize_id("123abc"), "abc");
        assert_eq!(crate::svg::sanitize::sanitize_id("-foo"), "foo");
        assert_eq!(crate::svg::sanitize::sanitize_id("💥x"), "x");
        assert_eq!(crate::svg::sanitize::sanitize_id("data icon@1.5x"), "data-icon-1.5x");
    }

    #[test]
    fn root_svg_id_is_moved_to_data_id_and_sanitized() {
        let tmp = TempDir::new("root_id_move");
        let dir = tmp.path();
        fs::write(
            dir.join("logo.svg"),
            "<svg id=\"123Logo\" width=\"1\" height=\"1\"><g/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        process(dir.to_str().unwrap(), out.to_str().unwrap()).expect("build ok");
        let sprite = fs::read_to_string(&out).expect("read sprite");
        // id removed, data-id present with sanitized value "Logo"
        assert!(sprite.contains("data-id=\"Logo\""));
        assert!(!sprite.contains(" id=\"123Logo\""));
    }

    #[test]
    fn root_svg_id_reference_causes_error() {
        let tmp = TempDir::new("root_id_ref");
        let dir = tmp.path();
        fs::write(
            dir.join("r.svg"),
            "<svg id=\"root\"><use href=\"#root\"/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        let err = process(dir.to_str().unwrap(), out.to_str().unwrap()).expect_err("should err");
        match err {
            AppError::RootIdReferenced { .. } => {}
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn id_collision_across_files_errors() {
        let tmp = TempDir::new("id_collision");
        let dir = tmp.path();
        fs::write(dir.join("a.svg"), "<svg width='1'><g id=\"dup\"/></svg>").unwrap();
        fs::write(dir.join("b.svg"), "<svg width='1'><g id=\"dup\"/></svg>").unwrap();
        let out = dir.join("sprite.svg");
        let err = process(dir.to_str().unwrap(), out.to_str().unwrap()).expect_err("should err");
        match err {
            AppError::IdCollision { id, .. } => assert_eq!(id, "dup"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn handles_bom_xml_prolog_and_leading_comment() {
        let tmp = TempDir::new("svg_preamble");
        let dir = tmp.path();
        let content = format!(
            "{}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- comment -->\n<svg width=\"10\" height=\"10\"><rect/></svg>",
            '\u{feff}'
        );
        fs::write(dir.join("p.svg"), content).unwrap();
        let out = dir.join("sprite.svg");
        process(dir.to_str().unwrap(), out.to_str().unwrap()).expect("build ok");
        let sprite = fs::read_to_string(&out).unwrap();
        assert!(sprite.contains("pattern id=\"p\""));
    }

    #[test]
    fn normalizes_width_height_values() {
        let tmp = TempDir::new("svg_dims_norm");
        let dir = tmp.path();
        fs::write(
            dir.join("s.svg"),
            "<svg width=\"24px\" height=\"24.0\"><g/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        process(dir.to_str().unwrap(), out.to_str().unwrap()).expect("build ok");
        let sprite = fs::read_to_string(&out).unwrap();
        assert!(sprite.contains("width=\"24\""));
        assert!(sprite.contains("height=\"24\""));
    }

    #[test]
    fn rejects_invalid_dimension_values() {
        let tmp = TempDir::new("svg_dims_reject");
        let dir = tmp.path();
        fs::write(dir.join("s.svg"), "<svg width=\"0\" height=\"1\"></svg>").unwrap();
        let out = dir.join("sprite.svg");
        let err = process(dir.to_str().unwrap(), out.to_str().unwrap()).expect_err("should err");
        match err {
            AppError::InvalidDimension { attr, .. } => assert_eq!(attr, "width"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn normalizes_viewbox() {
        let tmp = TempDir::new("svg_viewbox_norm");
        let dir = tmp.path();
        fs::write(
            dir.join("v.svg"),
            "<svg viewBox=\"0,0,24,24\" width=\"1\"><g/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        process(dir.to_str().unwrap(), out.to_str().unwrap()).expect("build ok");
        let sprite = fs::read_to_string(&out).unwrap();
        assert!(sprite.contains("viewBox=\"0 0 24 24\""));
    }

    #[test]
    fn rejects_invalid_viewbox_dims() {
        let tmp = TempDir::new("svg_viewbox_reject");
        let dir = tmp.path();
        fs::write(dir.join("v.svg"), "<svg viewBox=\"0 0 0 24\"><g/></svg>").unwrap();
        let out = dir.join("sprite.svg");
        let err = process(dir.to_str().unwrap(), out.to_str().unwrap()).expect_err("should err");
        match err {
            AppError::InvalidViewBox { .. } => {}
            other => panic!("unexpected error: {other}"),
        }
    }

    // Property tests for sanitize_id live in svg::sanitize

    // Normalization property tests live in svg::normalize

    // ID generators moved to svg::ids tests

    // Property tests for ids::extract_ids live in svg::ids

    // Property: preprocess_svg_content removes BOM, xml prolog, and leading comments before <svg>
    proptest! {
        #[test]
        fn prop_preprocess_svg_preamble_stripped(
            n_comments in 0usize..3,
            include_bom in proptest::bool::ANY,
            include_prolog in proptest::bool::ANY
        ) {
            let mut s = String::new();
            if include_bom { s.push('\u{feff}'); }
            if include_prolog { s.push_str("<?xml version=\"1.0\"?>"); }
            for i in 0..n_comments { s.push_str(&format!("<!-- c{i} -->")); }
            s.push_str("<svg width=\"1\"></svg>");
            let pre = preprocess_svg_content(&s);
            prop_assert!(pre.starts_with("<svg"));
        }
    }

    // Property tests for ids::references_id live in svg::ids
}
