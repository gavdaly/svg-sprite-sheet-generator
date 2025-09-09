// ID extraction and reference detection utilities

// Detect simple references to an id within content: href="#id", xlink:href="#id", or url(#id)
pub(crate) fn references_id(content: &str, id: &str) -> bool {
    content.contains(&format!("href=\"#{id}\""))
        || content.contains(&format!("xlink:href=\"#{id}\""))
        || content.contains(&format!("href='#{id}'"))
        || content.contains(&format!("xlink:href='#{id}'"))
        || content.contains(&format!("url(#{id})"))
}

fn is_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ':'
}

// Rewrite all internal id attributes to data-id attributes.
// Ensures there are no duplicate data-id values within the same content by
// appending a numeric suffix (-2, -3, ...) to subsequent duplicates.
// Returns the rewritten content and the list of resulting data-id values.
pub(crate) fn rewrite_ids_to_data_ids(s: &str) -> (String, Vec<String>) {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut data_ids = Vec::new();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        // Match id="..." or id='...'
        if i + 4 <= bytes.len() && &bytes[i..i + 3] == b"id=" {
            // Ensure it's a standalone id attribute (not data-id)
            let prev = i.checked_sub(1).and_then(|j| bytes.get(j)).copied();
            if let Some(p) = prev {
                if is_name_char(p as char) {
                    // Part of a larger name, copy one byte and continue
                    out.push(bytes[i] as char);
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
                            // Extract value
                            if let Ok(val) =
                                std::str::from_utf8(&bytes[start..j]).map(|v| v.to_string())
                            {
                                // Sanitize and disambiguate
                                let mut sanitized = crate::svg::sanitize::sanitize_id(&val);
                                if sanitized.is_empty() {
                                    // Fall back to original if sanitation removes all; keep stable
                                    sanitized = "x".into();
                                }
                                let entry = seen.entry(sanitized.clone()).or_insert(0);
                                *entry += 1;
                                let final_id = if *entry == 1 {
                                    sanitized
                                } else {
                                    format!("{}-{}", sanitized, *entry)
                                };
                                data_ids.push(final_id.clone());
                                // Write rewritten attribute
                                out.push_str("data-id=");
                                out.push(quote);
                                out.push_str(&final_id);
                                out.push(quote);
                                i = j + 1;
                                break;
                            } else {
                                // If invalid utf-8 segment, just copy raw and continue
                                out.push(bytes[i] as char);
                                i += 1;
                                break;
                            }
                        }
                        j += 1;
                    }
                    if j >= bytes.len() {
                        // Unclosed attribute; copy remainder and break
                        out.push_str(&s[i..]);
                        break;
                    }
                    continue;
                }
            }
        }
        // Default copy
        out.push(bytes[i] as char);
        i += 1;
    }
    (out, data_ids)
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

    // No property tests for extract_ids since internal ids are rewritten

    proptest! {
        #[test]
        fn prop_references_id_detects(needle in arb_valid_id(), other in arb_valid_id()) {
            prop_assume!(needle != other);
            let content = format!("<use href=\"#{needle}\"/><use xlink:href=\"#{needle}\"/><rect fill=\"url(#{needle})\"/>");
            prop_assert!(references_id(&content, &needle));
            prop_assert!(!references_id(&content, &other));
        }
    }

    #[test]
    fn rewrite_ids_simple() {
        let input = "<g id=\"a\"/><g id='a'/><g id=\"b\"/>";
        let (out, ids) = rewrite_ids_to_data_ids(input);
        assert!(out.contains("data-id=\"a\""));
        assert!(out.contains("data-id='a-2'"));
        assert!(out.contains("data-id=\"b\""));
        assert_eq!(
            ids,
            vec!["a".to_string(), "a-2".to_string(), "b".to_string()]
        );
        assert!(!out.contains(" id=\""));
        assert!(!out.contains(" id='"));
    }
}
