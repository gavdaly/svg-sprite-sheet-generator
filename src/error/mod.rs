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
            AppError::Unimplemented(feature) => write!(f, "not yet implemented: {feature}"),
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
