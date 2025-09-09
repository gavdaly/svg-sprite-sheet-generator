use crate::error::AppError;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use winnow::{
    PResult, Parser,
    ascii::{multispace0, multispace1},
    combinator::{preceded, terminated},
    token::{take_until, take_while},
};

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
    let sprite = transform(svgs);
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
        match parse_svg.parse_next(&mut s) {
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
                        match normalize_length(v) {
                            Some(nv) => out_attrs.push(((*k).to_string(), nv)),
                            None => {
                                return Err(AppError::InvalidDimension {
                                    path: path.display().to_string(),
                                    attr: (*k).to_string(),
                                    value: (*v).to_string(),
                                })
                            }
                        }
                    } else if *k == "viewBox" {
                        match normalize_viewbox(v) {
                            Some(vb) => pending_viewbox = Some(vb),
                            None => {
                                return Err(AppError::InvalidViewBox {
                                    path: path.display().to_string(),
                                    value: (*v).to_string(),
                                })
                            }
                        }
                    } else {
                        out_attrs.push(((*k).to_string(), (*v).to_string()));
                    }
                }

                if let Some(idv) = root_id_raw {
                    let sanitized = sanitize_id(idv);
                    if sanitized.is_empty() {
                        return Err(AppError::InvalidIdAfterSanitize {
                            path: path.display().to_string(),
                            original: idv.to_string(),
                        });
                    }
                    // Check if root id is referenced internally
                    if references_id(children, idv) {
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
                let child_ids = extract_ids(children);
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
fn sanitize_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    // Drop leading invalid until first valid start char
    while let Some(&ch) = it.peek() {
        if is_valid_id_start(ch) {
            break;
        }
        it.next();
    }
    // Process the rest
    let mut prev_dash = false;
    while let Some(ch) = it.next() {
        if is_valid_id_continue(ch) || is_valid_id_start(ch) {
            out.push(ch);
            prev_dash = false;
        } else {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
    }
    // Trim leading/trailing '-'
    while out.starts_with('-') {
        out.remove(0);
    }
    while out.ends_with('-') {
        out.pop();
    }
    // Collapse any "--" sequences that might remain (defensive)
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out
}

fn is_valid_id_start(ch: char) -> bool {
    (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z') || ch == '_'
}
fn is_valid_id_continue(ch: char) -> bool {
    (ch >= 'A' && ch <= 'Z')
        || (ch >= 'a' && ch <= 'z')
        || (ch >= '0' && ch <= '9')
        || ch == '.'
        || ch == '_'
        || ch == '-'
}

// Detect simple references to an id within content: href="#id", xlink:href="#id", or url(#id)
fn references_id(content: &str, id: &str) -> bool {
    content.contains(&format!("href=\"#{id}\""))
        || content.contains(&format!("xlink:href=\"#{id}\""))
        || content.contains(&format!("href='#{id}'"))
        || content.contains(&format!("xlink:href='#{id}'"))
        || content.contains(&format!("url(#{})", format!("#{id}")))
}

// Extract all id attribute values from a chunk of SVG/XML text.
// This is a lightweight scan that matches id="..." and id='...'
// and avoids matching names like data-id by checking the preceding char.
fn extract_ids(s: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Look for id=" or id='
        if i + 4 <= bytes.len() && &bytes[i..i + 3] == b"id=" {
            let prev = i.checked_sub(1).and_then(|j| bytes.get(j)).copied();
            if let Some(p) = prev {
                // If prev is a name char, it's likely part of a longer attr (e.g., data-id)
                if is_name_char(p as char) {
                    i += 1;
                    continue;
                }
            }
            // Determine quote
            if i + 4 <= bytes.len() {
                let quote = bytes[i + 3] as char;
                if quote == '"' || quote == '\'' {
                    let start = i + 4;
                    let mut j = start;
                    while j < bytes.len() {
                        if bytes[j] as char == quote {
                            if let Ok(val) = std::str::from_utf8(&bytes[start..j]) {
                                ids.push(val.to_string());
                            }
                            i = j + 1;
                            break;
                        }
                        j += 1;
                    }
                    if j >= bytes.len() {
                        // Unclosed quote; abort scan
                        break;
                    }
                    continue;
                }
            }
        }
        i += 1;
    }
    ids
}

fn is_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ':'
}

// Parse and normalize positive length values for width/height.
// Accepts unitless or 'px' suffix. Returns normalized string (e.g., "24").
fn normalize_length(v: &str) -> Option<String> {
    let t = v.trim();
    let num = if let Some(stripped) = t.strip_suffix("px") {
        stripped.trim()
    } else {
        t
    };
    // Reject percentages or other units
    if num.ends_with('%') || num.ends_with("em") || num.ends_with("rem") {
        return None;
    }
    let val: f64 = num.parse().ok()?;
    if !(val.is_finite() && val > 0.0) {
        return None;
    }
    Some(normalize_number(val))
}

fn normalize_number(n: f64) -> String {
    if (n.fract()).abs() < f64::EPSILON {
        format!("{:.0}", n)
    } else {
        // Default formatter gives a concise representation
        format!("{}", n)
    }
}

// Normalize viewBox into four numbers separated by single spaces.
// Accept commas and/or whitespace as separators. Require width/height > 0.
fn normalize_viewbox(v: &str) -> Option<String> {
    let replaced = v.replace(',', " ");
    let parts: Vec<&str> = replaced.split_whitespace().collect();
    if parts.len() != 4 {
        return None;
    }
    let min_x: f64 = parts[0].parse().ok()?;
    let min_y: f64 = parts[1].parse().ok()?;
    let width: f64 = parts[2].parse().ok()?;
    let height: f64 = parts[3].parse().ok()?;
    if !(width.is_finite() && width > 0.0 && height.is_finite() && height > 0.0) {
        return None;
    }
    Some(format!(
        "{} {} {} {}",
        normalize_number(min_x),
        normalize_number(min_y),
        normalize_number(width),
        normalize_number(height)
    ))
}

/// Transfrom a group of svgs into a single svg as a string
fn transform(svgs: Vec<SvgSprite>) -> String {
    let mut result = svgs.iter().fold(
        String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><defs>"#),
        |mut acc, svg| {
            let name = &svg.name;
            let children = &svg.children;
            let attributes = &svg
                .attributes
                .iter()
                .map(|(key, value)| format!(r#" {key}="{value}""#))
                .collect::<String>();
            acc.push_str(&format!(
                r#"<pattern id="{name}"{attributes}>{children}</pattern>"#
            ));
            acc
        },
    );
    result.push_str("</defs></svg>");
    result
}

fn parse_attribute<'s>(input: &mut &'s str) -> PResult<(&'s str, &'s str)> {
    // Parse an attribute in one of two forms:
    // - key[ws]?=[ws]?value    (value can be single or double quoted)
    // - key                    (boolean attribute; value mirrors key)
    let key = kebab_alpha1.parse_next(input)?;
    // Try to detect an '=' possibly surrounded by whitespace.
    let mut lookahead = *input;
    if parse_eq_ws.parse_next(&mut lookahead).is_ok() {
        // There is a value. Parse it from the advanced cursor.
        let val = parse_value.parse_next(&mut lookahead)?;
        *input = lookahead;
        Ok((key, val))
    } else {
        // Boolean attribute: use key as value to avoid empty string outputs.
        Ok((key, key))
    }
}

fn parse_value<'s>(input: &mut &'s str) -> PResult<&'s str> {
    // Support both double- and single-quoted values.
    if input.starts_with('"') {
        return preceded('"', terminated(take_until(0.., '"'), '"')).parse_next(input);
    }
    if input.starts_with('\'') {
        return preceded('\'', terminated(take_until(0.., '\''), '\'')).parse_next(input);
    }
    // Fall back to the double-quoted parser to emit a consistent error
    preceded('"', terminated(take_until(0.., '"'), '"')).parse_next(input)
}

fn parse_eq_ws(input: &mut &str) -> PResult<char> {
    // Consume optional whitespace, '=', optional whitespace
    multispace0.parse_next(input)?;
    let eq = '='.parse_next(input)?;
    multispace0.parse_next(input)?;
    Ok(eq)
}

fn kebab_alpha1<'s>(input: &mut &'s str) -> PResult<&'s str> {
    // Allow letters, digits, hyphen, underscore, and colon (for namespaced attributes like xmlns:xlink)
    take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '-', '_', ':')).parse_next(input)
}

fn entry_tag<'s>(input: &mut &'s str) -> PResult<&'s str> {
    terminated("<svg", multispace1).parse_next(input)
}

fn attributes<'s>(input: &mut &'s str) -> PResult<Vec<(&'s str, &'s str)>> {
    // Accept zero or more attributes separated by whitespace, allowing
    // arbitrary whitespace before the closing '>' without failing.
    // Strategy: parse an optional first attribute, then loop on (ws + attr).
    multispace0.parse_next(input)?;
    let mut out: Vec<(&'s str, &'s str)> = Vec::new();
    if let Ok(first) = parse_attribute.parse_next(input) {
        out.push(first);
        loop {
            let checkpoint = *input;
            match preceded(multispace1, parse_attribute).parse_next(input) {
                Ok(attr) => {
                    out.push(attr);
                }
                Err(_) => {
                    *input = checkpoint;
                    break;
                }
            }
        }
    }
    Ok(out)
}

fn parse_svg<'s>(input: &mut &'s str) -> PResult<(Vec<(&'s str, &'s str)>, &'s str)> {
    entry_tag.parse_next(input)?;
    let attrs = attributes.parse_next(input)?;
    preceded(multispace0, '>').parse_next(input)?;
    let children = terminated(take_until(0.., "</svg>"), "</svg>").parse_next(input)?;
    Ok((attrs, children))
}

#[cfg(test)]
fn parse_gt(input: &mut &str) -> PResult<char> {
    preceded(multispace0, '>').parse_next(input)
}

#[cfg(test)]
fn parse_children<'a>(input: &'a mut &'a str) -> PResult<&'a str> {
    terminated(take_until(0.., "</svg>"), "</svg>").parse_next(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};
    use winnow::Parser;
    use proptest::prelude::*;

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
    fn parse_attribute_test() {
        let input = &mut r##"fill="#000000""##;
        let result = parse_attribute.parse_next(input).unwrap();
        let answer = ("fill", "#000000");
        assert_eq!(result, answer)
    }
    #[test]
    fn parse_attribute_in_kebab_case_test() {
        let input = &mut r#"color-interpolation-filters="sRGB""#;
        let result = parse_attribute.parse_next(input);
        let answer = ("color-interpolation-filters", "sRGB");
        assert_eq!(result, Ok(answer))
    }
    #[test]
    fn parse_attribute_key_in_kebab_case_test() {
        let input = &mut "color-interpolation-filters";
        let result = kebab_alpha1.parse_next(input);
        let answer = "color-interpolation-filters";
        assert_eq!(result, Ok(answer))
    }
    #[test]
    fn parse_attributes_test() {
        let input = &mut r##"fill="#000000" stroke="red""##;
        let result = attributes.parse_next(input).unwrap();
        let answer = vec![("fill", "#000000"), ("stroke", "red")];
        assert_eq!(result, answer);
    }
    #[test]
    fn parse_attribute_single_quoted() {
        let input = &mut "width='24'";
        let result = parse_attribute.parse_next(input).unwrap();
        assert_eq!(result, ("width", "24"));
    }
    #[test]
    fn parse_attribute_colon_underscore_digits_in_key() {
        let input = &mut "data_2d:mode=\"on\"";
        let result = parse_attribute.parse_next(input).unwrap();
        assert_eq!(result, ("data_2d:mode", "on"));
    }
    #[test]
    fn parse_boolean_attribute() {
        let input = &mut "focusable";
        let result = parse_attribute.parse_next(input).unwrap();
        assert_eq!(result, ("focusable", "focusable"));
    }
    #[test]
    fn parse_svg_simple() {
        use super::parse_svg;
        let input = r##"<svg id="test" fill="#000000">Something</svg>"##;
        match parse_svg.parse(input) {
            Ok((_vec, children)) => assert_eq!(children, "Something"),
            Err(e) => {
                dbg!(e);
                assert!(false)
            }
        };
    }

    #[test]
    fn parse_svg_multiline_opening_tag() {
        let input = r#"<svg
  id="icon-arrow" width="24" height="24"
  viewBox="0 0 24 24"
>
  <path d="M 0 0 L 10 10"/>
</svg>
"#;
        let mut s = input;
        let (attrs, children) = super::parse_svg.parse_next(&mut s).expect("parse svg");
        assert!(attrs.iter().any(|(k, v)| *k == "id" && *v == "icon-arrow"));
        assert!(children.contains("<path"));
    }

    #[test]
    fn attributes_parse_multiline_block() {
        let input = r#"<svg
  id="icon-arrow" width="24" height="24"
  viewBox="0 0 24 24"
>
  <path d="M 0 0 L 10 10"/>
</svg>
"#;
        let mut s = input;
        entry_tag.parse_next(&mut s).expect("entry tag");
        let attrs = attributes.parse_next(&mut s).expect("attributes");
        assert_eq!(attrs.len(), 4);
        assert!(attrs.iter().any(|(k, _)| *k == "id"));
        // Ensure we can consume the '>' after optional whitespace
        parse_gt(&mut s).expect("gt");
        // And we can read children until closing tag
        let children = parse_children(&mut s).expect("children");
        assert!(children.contains("<path"));
    }

    #[test]
    fn attributes_with_extra_whitespace_and_newlines() {
        let mut input = "  fill=\"#333\"\n   stroke=\"red\"  ";
        let parsed = attributes.parse_next(&mut input).expect("attrs");
        assert_eq!(parsed, vec![("fill", "#333"), ("stroke", "red")]);
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
        let out = transform(svgs);
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
        assert_eq!(sanitize_id("123abc"), "abc");
        assert_eq!(sanitize_id("-foo"), "foo");
        assert_eq!(sanitize_id("💥x"), "x");
        assert_eq!(sanitize_id("data icon@1.5x"), "data-icon-1.5x");
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
        fs::write(
            dir.join("v.svg"),
            "<svg viewBox=\"0 0 0 24\"><g/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        let err = process(dir.to_str().unwrap(), out.to_str().unwrap()).expect_err("should err");
        match err {
            AppError::InvalidViewBox { .. } => {}
            other => panic!("unexpected error: {other}"),
        }
    }

    // Property: sanitize_id outputs only allowed chars, trims dashes, removes duplicates,
    // and is idempotent. It may return empty if no valid start char exists.
    proptest! {
        #[test]
        fn prop_sanitize_id_valid_and_idempotent(input in ".*") {
            let out = sanitize_id(&input);
            if !out.is_empty() {
                let mut chars = out.chars();
                let first = chars.next().unwrap();
                prop_assert!(is_valid_id_start(first));
                prop_assert!(!out.starts_with('-'));
                prop_assert!(!out.ends_with('-'));
                prop_assert!(!out.contains("--"));
                prop_assert!(out.chars().skip(1).all(is_valid_id_continue));
                prop_assert!(out.chars().all(|c| !c.is_whitespace()));
                prop_assert_eq!(sanitize_id(&out), out);
            }
        }
    }

    // Property: normalize_length accepts positive numbers (with optional px and whitespace),
    // returns a canonical representation that is idempotent and parsable > 0.
    proptest! {
        #[test]
        fn prop_normalize_length_positive_idempotent(
            n in 0.0000001f64..1.0e12f64,
            suffix_px in proptest::bool::ANY,
            pad_left in 0usize..3,
            pad_right in 0usize..3
        ) {
            // Avoid pathological float strings by formatting via to_string
            let mut s = n.to_string();
            if suffix_px { s.push_str("px"); }
            let input = format!("{left}{s}{right}", left = " ".repeat(pad_left), right = " ".repeat(pad_right));
            let out = normalize_length(&input).expect("should accept positive length");
            // out must parse and be > 0
            let parsed: f64 = out.parse().expect("normalized parses");
            prop_assert!(parsed.is_finite() && parsed > 0.0);
            // idempotent
            prop_assert_eq!(normalize_length(&out), Some(out.clone()));
        }
    }

    // Property: normalize_length rejects non-positive values and non-finite
    proptest! {
        #[test]
        fn prop_normalize_length_rejects_non_positive(n in -1.0e6f64..=0.0f64) {
            // Exclude NaN/inf via range; still guard just in case
            prop_assume!(n.is_finite());
            let input = n.to_string();
            prop_assert!(normalize_length(&input).is_none());
            let input_px = format!("{input}px");
            prop_assert!(normalize_length(&input_px).is_none());
        }
    }

    // Strategy to format numbers with optional comma/space separators
    fn fmt_viewbox(min_x: f64, min_y: f64, width: f64, height: f64, use_commas: bool, extra_ws: bool) -> String {
        let sep = if use_commas { "," } else { " " };
        let mut s = format!("{}{}{}{}{}{}{}",
            min_x, sep,
            if extra_ws { " " } else { "" }, min_y, sep,
            if extra_ws { "  " } else { "" }, width);
        if use_commas && extra_ws { s.push(' '); }
        s.push_str(sep);
        if !use_commas && extra_ws { s.push_str("   "); }
        s.push_str(&height.to_string());
        s
    }

    // Property: normalize_viewbox accepts 4-tuple with width/height > 0,
    // emits single-space-separated canonical string without commas and is idempotent.
    proptest! {
        #[test]
        fn prop_normalize_viewbox_idempotent(
            min_x in -1.0e6f64..1.0e6f64,
            min_y in -1.0e6f64..1.0e6f64,
            width in 0.000001f64..1.0e6f64,
            height in 0.000001f64..1.0e6f64,
            use_commas in proptest::bool::ANY,
            extra_ws in proptest::bool::ANY
        ) {
            prop_assume!(min_x.is_finite() && min_y.is_finite() && width.is_finite() && height.is_finite());
            let raw = fmt_viewbox(min_x, min_y, width, height, use_commas, extra_ws);
            let out = normalize_viewbox(&raw).expect("should normalize valid viewBox");
            // No commas, single-space separated 4 parts
            prop_assert!(!out.contains(','));
            let parts: Vec<&str> = out.split(' ').collect();
            prop_assert_eq!(parts.len(), 4);
            // Parse back and compare roughly
            let rx: f64 = parts[0].parse().unwrap();
            let ry: f64 = parts[1].parse().unwrap();
            let rw: f64 = parts[2].parse().unwrap();
            let rh: f64 = parts[3].parse().unwrap();
            prop_assert!((rx - min_x).abs() <= 1e-9 || (min_x.is_sign_negative() == rx.is_sign_negative()));
            prop_assert!((ry - min_y).abs() <= 1e-9 || (min_y.is_sign_negative() == ry.is_sign_negative()));
            prop_assert!(rw > 0.0 && rh > 0.0);
            // idempotent
            prop_assert_eq!(normalize_viewbox(&out), Some(out.clone()));
        }
    }

    // Property: invalid width/height in viewBox are rejected
    proptest! {
        #[test]
        fn prop_normalize_viewbox_rejects_bad_dims(width in -1.0e6f64..=0.0f64, height in -1.0e6f64..=0.0f64) {
            let raw = format!("0 0 {} {}", width, height);
            prop_assert!(normalize_viewbox(&raw).is_none());
        }
    }

    // Generate a valid id for use in other props
    fn arb_valid_id() -> impl Strategy<Value = String> {
        let alpha_lower = (b'a'..=b'z').prop_map(|b| b as char);
        let alpha_upper = (b'A'..=b'Z').prop_map(|b| b as char);
        let digit = (b'0'..=b'9').prop_map(|b| b as char);
        let start = prop_oneof![Just('_'), alpha_lower.clone(), alpha_upper.clone()];
        let cont_char = prop_oneof![alpha_lower, alpha_upper, digit, Just('.'), Just('_'), Just('-')];
        (start, proptest::collection::vec(cont_char, 0..12)).prop_map(|(s, v)| {
            let mut id = String::new();
            id.push(s);
            for c in v { id.push(c); }
            // Ensure no consecutive dashes to align with sanitize_id invariants where needed
            while id.contains("--") { id = id.replace("--", "-"); }
            id
        })
    }

    // Property: extract_ids captures only explicit id attributes, not data-id or other suffixes/prefixes.
    proptest! {
        #[test]
        fn prop_extract_ids_matches_inserted(ids in proptest::collection::vec(arb_valid_id(), 0..6)) {
            use std::collections::BTreeSet;
            // Build an svg-like content including both id and decoy attributes
            let mut content = String::from("<svg>");
            for (i, id) in ids.iter().enumerate() {
                let tag = if i % 2 == 0 { "g" } else { "path" };
                // include decoys around
                content.push_str(&format!("<{} data-id=\"not{}\" id='{}' data_id=\"x\"/>", tag, id, id));
                content.push_str(&format!("<use data-id=\"{}\" />", id));
            }
            content.push_str("</svg>");
            let extracted = extract_ids(&content);
            let got: BTreeSet<_> = extracted.into_iter().collect();
            let want: BTreeSet<_> = ids.into_iter().collect();
            prop_assert_eq!(got, want);
        }
    }

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
            for i in 0..n_comments { s.push_str(&format!("<!-- c{} -->", i)); }
            s.push_str("<svg width=\"1\"></svg>");
            let pre = preprocess_svg_content(&s);
            prop_assert!(pre.starts_with("<svg"));
        }
    }

    // Property: references_id detects specific references and does not trigger on other ids
    proptest! {
        #[test]
        fn prop_references_id_detects(needle in arb_valid_id(), other in arb_valid_id()) {
            prop_assume!(needle != other);
            let content = format!("<use href=\"#{}\"/><use xlink:href=\"#{}\"/><rect fill=\"url(#{})\"/>", needle, needle, needle);
            prop_assert!(references_id(&content, &needle));
            prop_assert!(!references_id(&content, &other));
        }
    }
}
