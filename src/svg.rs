use crate::error::AppError;
use std::collections::hash_map::DefaultHasher;
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
    let entries = std::fs::read_dir(directory)
        .map_err(|e| AppError::ReadDir { path: directory.to_string(), source: e })?;
    let mut hasher = DefaultHasher::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let _path = entry.path();
        let file_name = entry.file_name();
        let Ok(name_str) = file_name.into_string() else { continue };
        if !name_str.ends_with(".svg") { continue }
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
        let mut s = content.as_str();
        match parse_svg.parse_next(&mut s) {
            Ok((attributes, children)) => {
                sprites.push(SvgSprite::new(name, attributes, children.to_string()));
            }
            Err(e) => {
                let p = path.display().to_string();
                return Err(AppError::ParseSvg { path: p, message: format!("{e:?}") });
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
        return preceded('\'', terminated(take_until(0.., '\''), '\''))
            .parse_next(input);
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
                Ok(attr) => { out.push(attr); },
                Err(_) => { *input = checkpoint; break; }
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
fn parse_children(input: &mut &str) -> PResult<&str> {
    terminated(take_until(0.., "</svg>"), "</svg>").parse_next(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use winnow::Parser;
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
        fn path(&self) -> &std::path::Path { &self.0 }
    }
    impl Drop for TempDir {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
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
            SvgSprite::new("one".into(), vec![("width", "24"), ("height", "24")], "<g/>".into()),
            SvgSprite::new("two".into(), vec![("fill", "#000")], "<circle/>".into()),
        ];
        let out = transform(svgs);
        assert!(out.starts_with("<svg"));
        assert!(out.contains("<defs>"));
        assert!(out.contains("<pattern id=\"one\" width=\"24\" height=\"24\"><g/>") );
        assert!(out.contains("<pattern id=\"two\" fill=\"#000\"><circle/>") );
        assert!(out.ends_with("</defs></svg>"));
    }

    #[test]
    fn process_empty_directory_yields_error() {
        let tmp = TempDir::new("svg_sheet_empty");
        let err = process(tmp.path().to_str().unwrap(), &tmp.path().join("out.svg").display().to_string())
            .expect_err("expected error");
        match err { AppError::NoSvgFiles { .. } => {}, _ => panic!("wrong error: {err}") }
    }

    #[test]
    fn load_and_build_from_real_files() {
        let tmp = TempDir::new("svg_sheet_build");
        let dir = tmp.path();
        // Create a couple of minimal SVGs
        fs::write(dir.join("a.svg"), "<svg width=\"10\" height=\"10\"><rect/></svg>").unwrap();
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
}
