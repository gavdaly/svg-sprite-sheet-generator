use super::SvgSprite;

// Render the final sprite XML from a list of parsed SvgSprite entries
pub(crate) fn transform(svgs: Vec<SvgSprite>) -> String {
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

