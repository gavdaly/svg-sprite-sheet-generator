mod cli;
mod error;
mod svg;

use crate::error::AppError;
use cli::{Args, Commands, LogLevel, Shell};
use std::error::Error as _;

fn main() {
    let args = cli::parse();
    init_tracing(&args);

    let result: Result<(), AppError> = match &args.command {
        None => svg::process_with_opts(&args.directory, &args.file, to_run_opts(&args)),
        Some(Commands::Build) => {
            svg::process_with_opts(&args.directory, &args.file, to_run_opts(&args))
        }
        Some(Commands::Watch) => {
            svg::watch_with_opts(&args.directory, &args.file, to_run_opts(&args))
        }
        Some(Commands::Completions { shell, out_dir }) => {
            generate_completions(*shell, out_dir.clone())
        }
        Some(Commands::Man { out_dir }) => generate_man(out_dir.clone()),
    };

    match result {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("\x1b[1;31mError:\x1b[0m {e}");
            if let Some(source) = e.source() {
                eprintln!("Caused by: {source}");
            }
            std::process::exit(1)
        }
    }
}

fn to_run_opts(args: &Args) -> svg::RunOpts {
    svg::RunOpts {
        quiet: args.quiet,
        verbose: args.verbose,
        dry_run: args.dry_run,
        fail_on_warn: args.fail_on_warn,
        debounce_ms: args.debounce_ms,
        poll: args.poll,
    }
}

fn generate_completions(shell: Shell, out_dir: Option<std::path::PathBuf>) -> Result<(), AppError> {
    use clap_complete::{Shell as ClapShell, generate_to};
    let mut cmd = cli::command();
    let shell = match shell {
        Shell::Bash => ClapShell::Bash,
        Shell::Zsh => ClapShell::Zsh,
        Shell::Fish => ClapShell::Fish,
        Shell::Pwsh => ClapShell::PowerShell,
        Shell::Elvish => ClapShell::Elvish,
    };
    let out_dir = out_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    std::fs::create_dir_all(&out_dir).map_err(|e| AppError::WriteFile {
        path: out_dir.display().to_string(),
        source: e,
    })?;
    let bin_name = env!("CARGO_PKG_NAME");
    let _path =
        generate_to(shell, &mut cmd, bin_name, &out_dir).map_err(|e| AppError::WriteFile {
            path: out_dir.display().to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;
    println!(
        "Generated completions for {bin_name} in {}",
        out_dir.display()
    );
    Ok(())
}

fn generate_man(out_dir: Option<std::path::PathBuf>) -> Result<(), AppError> {
    let cmd = cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut out_path = out_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    std::fs::create_dir_all(&out_path).map_err(|e| AppError::WriteFile {
        path: out_path.display().to_string(),
        source: e,
    })?;
    out_path.push(format!("{}.1", env!("CARGO_PKG_NAME")));
    let mut buf = Vec::new();
    man.render(&mut buf).map_err(|e| AppError::WriteFile {
        path: out_path.display().to_string(),
        source: std::io::Error::other(e.to_string()),
    })?;
    std::fs::write(&out_path, buf).map_err(|e| AppError::WriteFile {
        path: out_path.display().to_string(),
        source: e,
    })?;
    println!("Wrote man page to {}", out_path.display());
    Ok(())
}

fn init_tracing(args: &Args) {
    use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

    // If RUST_LOG is set, prefer it; otherwise derive from CLI flags
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(f) => f,
        Err(_) => {
            let level = match args.log_level {
                Some(LogLevel::Error) => "error",
                Some(LogLevel::Warn) => "warn",
                Some(LogLevel::Info) => "info",
                Some(LogLevel::Debug) => "debug",
                Some(LogLevel::Trace) => "trace",
                None => {
                    if args.quiet {
                        "error"
                    } else if args.verbose {
                        "info"
                    } else {
                        "warn"
                    }
                }
            };
            EnvFilter::new(level)
        }
    };

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_file(false)
        .with_line_number(false)
        .with_timer(fmt::time::uptime())
        .json();

    let subscriber = Registry::default().with(env_filter).with(fmt_layer);
    let _ = tracing::subscriber::set_global_default(subscriber);
}
