use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum, value_parser};
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Pwsh,
    Elvish,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Watch,
    Build,
    /// Generate shell completions for a given shell
    Completions {
        #[arg(value_enum)]
        shell: Shell,
        /// Output directory to write the completion file(s)
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
    },
    /// Generate a man page
    Man {
        /// Output directory to write the man page
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value = "sprite.svg")]
    pub file: String,
    #[arg(short, long, default_value = "svgs")]
    pub directory: String,

    /// Use filesystem polling instead of event-based watching
    #[arg(long, action = ArgAction::SetTrue)]
    pub poll: bool,
    /// Debounce interval in milliseconds for event-based watch
    #[arg(long, default_value_t = 300, value_parser = value_parser!(u64))]
    pub debounce_ms: u64,

    /// Suppress non-error output
    #[arg(long, action = ArgAction::SetTrue)]
    pub quiet: bool,
    /// Increase verbosity (info-level messages)
    #[arg(long, action = ArgAction::SetTrue)]
    pub verbose: bool,
    /// Parse/validate but do not write output files
    #[arg(long, action = ArgAction::SetTrue)]
    pub dry_run: bool,
    /// Treat warnings as errors
    #[arg(long, action = ArgAction::SetTrue)]
    pub fail_on_warn: bool,

    /// Global log level when RUST_LOG is not set
    /// One of: error, warn, info, debug, trace
    #[arg(long, value_enum)]
    pub log_level: Option<LogLevel>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Parse CLI arguments from the process arguments using clap.
///
/// Example (constructing `Args` directly for testing):
/// ```
/// use svg_sheet::cli::Args;
/// use clap::Parser;
/// let args = Args::try_parse_from([
///     "svg_sheet", "-d", "svgs", "-f", "sprite.svg", "build"
/// ]).unwrap();
/// assert_eq!(args.directory, "svgs");
/// assert_eq!(args.file, "sprite.svg");
/// ```
pub fn parse() -> Args {
    Args::parse()
}

/// Build the clap `Command` for this CLI (useful for completions/manpages).
///
/// Example: render short help text
/// ```
/// let mut cmd = svg_sheet::cli::command();
/// let help = cmd.render_help().to_string();
/// assert!(!help.is_empty());
/// ```
pub fn command() -> clap::Command {
    Args::command()
}
