// ID extraction and reference detection utilities

// Detect simple references to an id within content: href="#id", xlink:href="#id", or url(#id)
pub(crate) fn references_id(content: &str, id: &str) -> bool {
    content.contains(&format!("href=\"#{id}\""))
        || content.contains(&format!("xlink:href=\"#{id}\""))
        || content.contains(&format!("href='#{id}'"))
        || content.contains(&format!("xlink:href='#{id}'"))
        || content.contains(&format!("url(#{id})"))
}

// Extract all id attribute values from a chunk of SVG/XML text.
// This is a lightweight scan that matches id="..." and id='...'
// and avoids matching names like data-id by checking the preceding char.
pub(crate) fn extract_ids(s: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 4 <= bytes.len() && &bytes[i..i + 3] == b"id=" {
            let prev = i.checked_sub(1).and_then(|j| bytes.get(j)).copied();
            if let Some(p) = prev {
                if is_name_char(p as char) {
                    i += 1;
                    continue;
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Generate a valid id for use in props
    fn arb_valid_id() -> impl Strategy<Value = String> {
        let alpha_lower = (b'a'..=b'z').prop_map(|b| b as char);
        let alpha_upper = (b'A'..=b'Z').prop_map(|b| b as char);
        let digit = (b'0'..=b'9').prop_map(|b| b as char);
        let start = prop_oneof![Just('_'), alpha_lower.clone(), alpha_upper.clone()];
        let cont_char = prop_oneof![
            alpha_lower,
            alpha_upper,
            digit,
            Just('.'),
            Just('_'),
            Just('-')
        ];
        (start, proptest::collection::vec(cont_char, 0..12)).prop_map(|(s, v)| {
            let mut id = String::new();
            id.push(s);
            for c in v {
                id.push(c);
            }
            while id.contains("--") {
                id = id.replace("--", "-");
            }
            id
        })
    }

    proptest! {
        #[test]
        fn prop_extract_ids_matches_inserted(ids in proptest::collection::vec(arb_valid_id(), 0..6)) {
            use std::collections::BTreeSet;
            let mut content = String::from("<svg>");
            for (i, id) in ids.iter().enumerate() {
                let tag = if i % 2 == 0 { "g" } else { "path" };
                content.push_str(&format!("<{tag} data-id=\"not{id}\" id='{id}' data_id=\"x\"/>"));
                content.push_str(&format!("<use data-id=\"{id}\" />"));
            }
            content.push_str("</svg>");
            let extracted = extract_ids(&content);
            let got: BTreeSet<_> = extracted.into_iter().collect();
            let want: BTreeSet<_> = ids.into_iter().collect();
            prop_assert_eq!(got, want);
        }
    }

    proptest! {
        #[test]
        fn prop_references_id_detects(needle in arb_valid_id(), other in arb_valid_id()) {
            prop_assume!(needle != other);
            let content = format!("<use href=\"#{needle}\"/><use xlink:href=\"#{needle}\"/><rect fill=\"url(#{needle})\"/>");
            prop_assert!(references_id(&content, &needle));
            prop_assert!(!references_id(&content, &other));
        }
    }
}

