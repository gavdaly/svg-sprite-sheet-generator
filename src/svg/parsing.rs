use winnow::{
    PResult, Parser,
    ascii::{multispace0, multispace1},
    combinator::{preceded, terminated},
    token::{take_until, take_while},
};

// Public within crate: used by svg::load_svgs
pub(crate) fn parse_svg<'s>(input: &mut &'s str) -> PResult<(Vec<(&'s str, &'s str)>, &'s str)> {
    entry_tag.parse_next(input)?;
    let attrs = attributes.parse_next(input)?;
    preceded(multispace0, '>').parse_next(input)?;
    let children = terminated(take_until(0.., "</svg>"), "</svg>").parse_next(input)?;
    Ok((attrs, children))
}

// Attribute list: zero or more attributes separated by whitespace
fn attributes<'s>(input: &mut &'s str) -> PResult<Vec<(&'s str, &'s str)>> {
    multispace0.parse_next(input)?;
    let mut out: Vec<(&'s str, &'s str)> = Vec::new();
    if let Ok(first) = parse_attribute.parse_next(input) {
        out.push(first);
        loop {
            let checkpoint = *input;
            match preceded(multispace1, parse_attribute).parse_next(input) {
                Ok(attr) => out.push(attr),
                Err(_) => {
                    *input = checkpoint;
                    break;
                }
            }
        }
    }
    Ok(out)
}

// Parse an attribute in two forms: key[ws]?=[ws]?value or boolean key
fn parse_attribute<'s>(input: &mut &'s str) -> PResult<(&'s str, &'s str)> {
    let key = kebab_alpha1.parse_next(input)?;
    let mut lookahead = *input;
    if parse_eq_ws.parse_next(&mut lookahead).is_ok() {
        let val = parse_value.parse_next(&mut lookahead)?;
        *input = lookahead;
        Ok((key, val))
    } else {
        Ok((key, key))
    }
}

fn parse_value<'s>(input: &mut &'s str) -> PResult<&'s str> {
    if input.starts_with('"') {
        return preceded('"', terminated(take_until(0.., '"'), '"')).parse_next(input);
    }
    if input.starts_with('\'') {
        return preceded('\'', terminated(take_until(0.., '\''), '\'')).parse_next(input);
    }
    preceded('"', terminated(take_until(0.., '"'), '"')).parse_next(input)
}

fn parse_eq_ws(input: &mut &str) -> PResult<char> {
    multispace0.parse_next(input)?;
    let eq = '='.parse_next(input)?;
    multispace0.parse_next(input)?;
    Ok(eq)
}

fn kebab_alpha1<'s>(input: &mut &'s str) -> PResult<&'s str> {
    take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '-', '_', ':')).parse_next(input)
}

fn entry_tag<'s>(input: &mut &'s str) -> PResult<&'s str> {
    terminated("<svg", multispace1).parse_next(input)
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
        let input = r##"<svg id="test" fill="#000000">Something</svg>"##;
        match parse_svg.parse(input) {
            Ok((_vec, children)) => assert_eq!(children, "Something"),
            Err(e) => panic!("parse_svg error: {e:?}"),
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
        assert!(attrs.iter().any(|(k, v)| *k == "width" && *v == "24"));
        parse_gt(&mut s).expect("gt");
        let children = parse_children(&mut s).expect("children");
        assert!(children.contains("<path"));
    }
}
