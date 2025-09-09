use winnow::{
    ascii::multispace1,
    combinator::{preceded, separated, separated_pair, terminated},
    token::{take_until, take_while},
    PResult, Parser,
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
pub fn process(directory: &str, file: &str) -> Result<(), ()> {
    let Ok(svgs) = load_svgs(directory) else {
        return Err(());
    };
    let sprite = transform(svgs);
    write_sprite(&sprite, file)?;
    Ok(())
}

/// Loads all the svg files in the directory
fn load_svgs<'b>(directory: &'b str) -> Result<Vec<SvgSprite>, String> {
    let Ok(files) = std::fs::read_dir(directory) else {
        return Err("Error Reading Directory".to_string());
    };

    let files = files.filter_map(|f| {
        let file = f.expect("to have the existing file");
        let path = file.path();
        let name = file
            .file_name()
            .into_string()
            .expect("to have an existing filename");
        if !name.ends_with(".svg") {
            return None;
        }
        let name = name.strip_suffix(".svg").unwrap().to_string();
        Some((name, path))
    });

    let files = files
        .map(|(name, path)| {
            let Ok(a) = std::fs::read_to_string(path) else {
                return Err("Error reading file".to_string());
            };
            let Ok((attributes, children)) = parse_svg.parse(&a) else {
                return Err("Error parsing".to_string());
            };
            Ok(SvgSprite::new(name, attributes, children.to_string()))
        })
        .flatten()
        .collect();
    Ok(files)
}

/// Write the sprite to a file
fn write_sprite(sprite: &str, file: &str) -> Result<(), ()> {
    std::fs::write(file, sprite).unwrap();
    Ok(())
}

/// Transfrom a group of svgs into a single svg as a string
fn transform<'b>(svgs: Vec<SvgSprite>) -> String {
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
    let val = take_while(1.., ('a'..='z', 'A'..='Z', '-')).parse_next(input);
    val
}

fn entry_tag<'s>(input: &mut &'s str) -> PResult<&'s str> {
    terminated("<svg", multispace1).parse_next(input)
}

fn attributes<'s>(input: &mut &'s str) -> PResult<Vec<(&'s str, &'s str)>> {
    separated(0.., parse_attribute, multispace1).parse_next(input)
}

fn parse_svg<'s>(input: &mut &'s str) -> PResult<(Vec<(&'s str, &'s str)>, &'s str)> {
    separated_pair(
        preceded(entry_tag, attributes),
        '>',
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
