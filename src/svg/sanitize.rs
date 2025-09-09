//! ID sanitization utilities.

/// Sanitize an id by dropping leading invalid chars and replacing internal
/// invalid chars with '-' (collapsing repeats and trimming ends).
/// Allowed pattern: `[A-Za-z_][A-Za-z0-9._-]*`.
///
/// Example:
/// ```
/// assert_eq!(svg_sheet::svg::sanitize::sanitize_id(" 123 bad"), "bad");
/// ```
pub fn sanitize_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    while let Some(&ch) = it.peek() {
        if is_valid_id_start(ch) {
            break;
        }
        it.next();
    }
    let mut prev_dash = false;
    for ch in it {
        if is_valid_id_continue(ch) || is_valid_id_start(ch) {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.starts_with('-') {
        out.remove(0);
    }
    while out.ends_with('-') {
        out.pop();
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out
}

/// Return whether a char is a valid starting character for an id.
fn is_valid_id_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}
/// Return whether a char is a valid continuing character for an id.
fn is_valid_id_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-'
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn sanitize_id_drops_leading_and_replaces_invalids() {
        assert_eq!(sanitize_id("123abc"), "abc");
        assert_eq!(sanitize_id("-foo"), "foo");
        assert_eq!(sanitize_id("💥x"), "x");
        assert_eq!(sanitize_id("data icon@1.5x"), "data-icon-1.5x");
    }

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
}
