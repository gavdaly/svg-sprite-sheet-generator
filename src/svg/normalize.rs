// Normalization utilities for lengths and viewBox values

// Parse and normalize positive length values for width/height.
// Accepts unitless or 'px' suffix. Returns normalized string (e.g., "24").
pub(crate) fn normalize_length(v: &str) -> Option<String> {
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

pub(crate) fn normalize_number(n: f64) -> String {
    if (n.fract()).abs() < f64::EPSILON {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

// Normalize viewBox into four numbers separated by single spaces.
// Accept commas and/or whitespace as separators. Require width/height > 0.
pub(crate) fn normalize_viewbox(v: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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
            let mut s = n.to_string();
            if suffix_px { s.push_str("px"); }
            let input = format!("{left}{s}{right}", left = " ".repeat(pad_left), right = " ".repeat(pad_right));
            let out = normalize_length(&input).expect("should accept positive length");
            let parsed: f64 = out.parse().expect("normalized parses");
            prop_assert!(parsed.is_finite() && parsed > 0.0);
            prop_assert_eq!(normalize_length(&out), Some(out.clone()));
        }
    }

    // Property: normalize_length rejects non-positive values and non-finite
    proptest! {
        #[test]
        fn prop_normalize_length_rejects_non_positive(n in -1.0e6f64..=0.0f64) {
            prop_assume!(n.is_finite());
            let input = n.to_string();
            prop_assert!(normalize_length(&input).is_none());
            let input_px = format!("{input}px");
            prop_assert!(normalize_length(&input_px).is_none());
        }
    }

    // Strategy to format numbers with optional comma/space separators
    fn fmt_viewbox(
        min_x: f64,
        min_y: f64,
        width: f64,
        height: f64,
        use_commas: bool,
        extra_ws: bool,
    ) -> String {
        let sep = if use_commas { "," } else { " " };
        let mut s = format!(
            "{}{}{}{}{}{}{}",
            min_x,
            sep,
            if extra_ws { " " } else { "" },
            min_y,
            sep,
            if extra_ws { "  " } else { "" },
            width
        );
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
            prop_assert!(!out.contains(','));
            let parts: Vec<&str> = out.split(' ').collect();
            prop_assert_eq!(parts.len(), 4);
            let rx: f64 = parts[0].parse().unwrap();
            let ry: f64 = parts[1].parse().unwrap();
            let rw: f64 = parts[2].parse().unwrap();
            let rh: f64 = parts[3].parse().unwrap();
            prop_assert!((rx - min_x).abs() <= 1e-9 || (min_x.is_sign_negative() == rx.is_sign_negative()));
            prop_assert!((ry - min_y).abs() <= 1e-9 || (min_y.is_sign_negative() == ry.is_sign_negative()));
            prop_assert!(rw > 0.0 && rh > 0.0);
            prop_assert_eq!(normalize_viewbox(&out), Some(out.clone()));
        }
    }

    // Property: invalid width/height in viewBox are rejected
    proptest! {
        #[test]
        fn prop_normalize_viewbox_rejects_bad_dims(width in -1.0e6f64..=0.0f64, height in -1.0e6f64..=0.0f64) {
            let raw = format!("0 0 {width} {height}");
            prop_assert!(normalize_viewbox(&raw).is_none());
        }
    }
}

