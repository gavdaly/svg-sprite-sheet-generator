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
    WarningsPresent { count: usize },
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
