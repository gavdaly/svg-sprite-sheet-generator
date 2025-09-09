use crate::error::AppError;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use winnow::{
    PResult, Parser,
    ascii::{multispace0, multispace1},
    combinator::{preceded, separated, separated_pair, terminated},
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
    separated_pair(kebab_alpha1, '=', parse_value).parse_next(input)
}

fn parse_value<'s>(input: &mut &'s str) -> PResult<&'s str> {
    preceded('"', terminated(take_until(0.., '"'), '"')).parse_next(input)
}

fn kebab_alpha1<'s>(input: &mut &'s str) -> PResult<&'s str> {
    take_while(1.., ('a'..='z', 'A'..='Z', '-')).parse_next(input)
}

fn entry_tag<'s>(input: &mut &'s str) -> PResult<&'s str> {
    terminated("<svg", multispace1).parse_next(input)
}

fn attributes<'s>(input: &mut &'s str) -> PResult<Vec<(&'s str, &'s str)>> {
    preceded(multispace0, separated(0.., parse_attribute, multispace1)).parse_next(input)
}

fn parse_svg<'s>(input: &mut &'s str) -> PResult<(Vec<(&'s str, &'s str)>, &'s str)> {
    separated_pair(
        preceded(entry_tag, attributes),
        preceded(multispace0, '>'),
        terminated(take_until(0.., "</svg>"), "</svg>"),
    )
    .parse_next(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use winnow::Parser;
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
    fn parse_svg() {
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
}
