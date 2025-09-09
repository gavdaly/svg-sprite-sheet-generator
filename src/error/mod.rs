use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    ReadDir {
        path: String,
        source: std::io::Error,
    },
    ReadFile {
        path: String,
        source: std::io::Error,
    },
    WriteFile {
        path: String,
        source: std::io::Error,
    },
    ParseSvg {
        path: String,
        message: String,
    },
    NoSvgFiles {
        path: String,
    },
    /// Duplicate id detected across inputs
    IdCollision {
        id: String,
        first_path: String,
        second_path: String,
    },
    /// Root <svg> had an id that is referenced inside the document
    RootIdReferenced {
        path: String,
        id: String,
    },
    /// An id became empty after sanitization
    InvalidIdAfterSanitize {
        path: String,
        original: String,
    },
    /// width/height attribute has an invalid or unsupported value
    InvalidDimension {
        path: String,
        attr: String,
        value: String,
    },
    /// viewBox attribute is malformed or has non-positive dimensions
    InvalidViewBox {
        path: String,
        value: String,
    },
    /// Warnings were emitted and --fail-on-warn was set
    WarningsPresent {
        count: usize,
    },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::ReadDir { path, .. } => write!(f, "failed to read directory: {path}"),
            AppError::ReadFile { path, .. } => write!(f, "failed to read file: {path}"),
            AppError::WriteFile { path, .. } => write!(f, "failed to write file: {path}"),
            AppError::ParseSvg { path, message } => {
                write!(f, "failed to parse svg ({path}): {message}")
            }
            AppError::NoSvgFiles { path } => write!(f, "no SVG files found in directory: {path}"),
            AppError::IdCollision {
                id,
                first_path,
                second_path,
            } => write!(
                f,
                "duplicate id '{id}' found in {second_path}; already defined in {first_path}"
            ),
            AppError::RootIdReferenced { path, id } => write!(
                f,
                "root <svg> id '{id}' in {path} is referenced inside the document; root ids are moved to data-id"
            ),
            AppError::InvalidIdAfterSanitize { path, original } => {
                write!(f, "id '{original}' in {path} is empty after sanitization")
            }
            AppError::InvalidDimension { path, attr, value } => write!(
                f,
                "invalid {attr}='{value}' in {path}; expected positive number (optionally 'px')"
            ),
            AppError::InvalidViewBox { path, value } => write!(
                f,
                "invalid viewBox='{value}' in {path}; expected four numbers with positive width/height"
            ),
            AppError::WarningsPresent { count } => write!(
                f,
                "aborting due to {count} warning(s) (use --no-fail-on-warn to ignore)"
            ),
        }
    }
}

impl StdError for AppError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            AppError::ReadDir { source, .. } => Some(source),
            AppError::ReadFile { source, .. } => Some(source),
            AppError::WriteFile { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_read_write_errors() {
        let e = AppError::ReadDir {
            path: "/x".into(),
            source: std::io::Error::other("boom"),
        };
        let s = e.to_string();
        assert!(s.contains("failed to read directory"));

        let e = AppError::ReadFile {
            path: "f.svg".into(),
            source: std::io::Error::other("boom"),
        };
        let s = e.to_string();
        assert!(s.contains("failed to read file"));

        let e = AppError::WriteFile {
            path: "out.svg".into(),
            source: std::io::Error::other("boom"),
        };
        let s = e.to_string();
        assert!(s.contains("failed to write file"));
    }

    #[test]
    fn display_parse_and_no_svg() {
        let e = AppError::ParseSvg {
            path: "p.svg".into(),
            message: "bad".into(),
        };
        let s = e.to_string();
        assert!(s.contains("failed to parse svg"));

        let e = AppError::NoSvgFiles { path: "dir".into() };
        let s = e.to_string();
        assert!(s.contains("no SVG files"));
    }

    #[test]
    fn display_id_related() {
        let e = AppError::IdCollision {
            id: "dup".into(),
            first_path: "a".into(),
            second_path: "b".into(),
        };
        assert!(e.to_string().contains("duplicate id"));

        let e = AppError::RootIdReferenced {
            path: "p.svg".into(),
            id: "root".into(),
        };
        assert!(e.to_string().contains("root <svg> id"));

        let e = AppError::InvalidIdAfterSanitize {
            path: "p.svg".into(),
            original: "💥".into(),
        };
        assert!(e.to_string().contains("empty after sanitization"));
    }

    #[test]
    fn display_invalid_attrs_and_warnings() {
        let e = AppError::InvalidDimension {
            path: "p.svg".into(),
            attr: "width".into(),
            value: "0".into(),
        };
        assert!(e.to_string().contains("invalid width='0'"));

        let e = AppError::InvalidViewBox {
            path: "p.svg".into(),
            value: "0 0 0 0".into(),
        };
        assert!(e.to_string().contains("invalid viewBox"));

        let e = AppError::WarningsPresent { count: 3 };
        assert!(e.to_string().contains("aborting due to 3 warning(s)"));
    }
}
