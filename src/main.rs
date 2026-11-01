//! midiboard: run commands from a MIDI controller, with LED feedback.

mod beatstep;
mod config;
mod devices;
mod encoder;
mod engine;
mod exec;
mod midi;
mod replay;
mod runtime;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "midiboard",
    version,
    author,
    about = "Run commands from any MIDI controller, with encoder handling and LED feedback.",
    long_about = "Run commands from any MIDI 1.0 controller: change the volume with a knob, \
                  toggle mute with a pad, launch scripts with a button, and light up the \
                  controller's LEDs to show what's going on.\n\n\
                  The config file lives at $XDG_CONFIG_HOME/midiboard/config.json \
                  (~/.config/midiboard/config.json) unless --path is given.\n\n\
                  Logging defaults to the config's `log_level` for `run` and to `info` \
                  otherwise; set RUST_LOG (e.g. RUST_LOG=debug) to override."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Args, Debug, Clone)]
struct PathArg {
    /// Config file to use instead of the default location.
    #[arg(short, long, value_name = "FILE")]
    path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the service: listen to the configured devices and run commands.
    Run(PathArg),

    /// List MIDI input and output ports.
    #[command(visible_alias = "list")]
    Devices,

    /// Print every message a device sends, decoded as `type`, `channel` and `key`.
    ///
    /// Use it to find the values to put in the config. With --record, also
    /// saves a trace that `midiboard replay` can run through the engine.
    Listen {
        /// Input port (full name, client name, or a unique part of it).
        device: String,
        /// Save a trace (`t_us,bytes` per line) to FILE.
        #[arg(short, long, value_name = "FILE")]
        record: Option<PathBuf>,
    },

    /// Create or check the config file.
    Config {
        #[command(subcommand)]
        action: ConfigCmd,
    },

    /// Run a recorded trace through the engine and print what would execute.
    ///
    /// Output is CSV: `t_us,event,raw,position,value`, where `event` is
    /// `input` (a message for the control) or `exec` (a command run). Plot
    /// position and value against time to see latency and smoothing.
    Replay {
        /// Trace recorded with `midiboard listen --record`.
        trace: PathBuf,
        /// Control to report on.
        #[arg(short, long)]
        control: String,
        /// Simulated time each command takes to finish, in milliseconds.
        #[arg(long, default_value_t = 5)]
        exec_ms: u64,
        /// Write the CSV to FILE instead of stdout.
        #[arg(short, long, value_name = "FILE")]
        out: Option<PathBuf>,
        #[command(flatten)]
        path: PathArg,
    },

    /// Send raw MIDI bytes to an output port, e.g. "90 24 7F" or a SysEx.
    Send {
        /// Output port (full name, client name, or a unique part of it).
        device: String,
        /// Hex bytes, separated by spaces.
        bytes: String,
    },

    /// Set one LED on an Arturia BeatStep.
    Led {
        /// Output port of the BeatStep.
        device: String,
        /// Pad number, 1-16.
        #[arg(long, conflicts_with = "button", required_unless_present = "button")]
        pad: Option<u8>,
        /// Function button: play, seq, sync, recall, store, shift or chan.
        #[arg(long)]
        button: Option<String>,
        /// off, red, blue or magenta.
        #[arg(long, default_value = "red")]
        color: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Write a starter config file.
    #[command(visible_alias = "new")]
    Generate {
        #[command(flatten)]
        path: PathArg,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Check a config file and report every problem found.
    #[command(visible_alias = "check")]
    Validate(PathArg),
    /// Print the default config location.
    Path,
}

fn init_logging(default_level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ =
        tracing_subscriber::fmt().with_env_filter(filter).with_target(false).with_writer(std::io::stderr).try_init();
}

fn generate(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        anyhow::bail!("{} already exists; use --force to overwrite it", path.display());
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    }
    std::fs::write(path, config::SKELETON).with_context(|| format!("could not write {}", path.display()))?;
    println!("Wrote a starter config to {}", path.display());
    Ok(())
}

fn validate(path: &Path) -> Result<()> {
    let (file, devices) = config::load_compiled(path)?;
    let controls: usize = devices.iter().map(|d| d.controls.len()).sum();
    println!(
        "{} is valid: {} device(s), {} control(s), log level `{}`.",
        path.display(),
        devices.len(),
        controls,
        file.log_level.as_filter()
    );
    Ok(())
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Run(p) => {
            let path = config::resolve_path(p.path.as_deref())?;
            let (file, devices) = config::load_compiled(&path)?;
            init_logging(file.log_level.as_filter());
            tracing::info!(config = %path.display(), devices = devices.len(), "starting");
            runtime::run(devices)
        }
        Cmd::Devices => devices::list(),
        Cmd::Listen { device, record } => {
            init_logging("info");
            devices::listen(&device, record.as_deref())
        }
        Cmd::Config { action } => match action {
            ConfigCmd::Generate { path, force } => generate(&config::resolve_path(path.path.as_deref())?, force),
            ConfigCmd::Validate(p) => validate(&config::resolve_path(p.path.as_deref())?),
            ConfigCmd::Path => {
                println!("{}", config::default_path()?.display());
                Ok(())
            }
        },
        Cmd::Replay { trace, control, exec_ms, out, path } => {
            init_logging("warn");
            let path = config::resolve_path(path.path.as_deref())?;
            let (_, devices) = config::load_compiled(&path)?;
            replay::run(&devices, &trace, &control, exec_ms, out.as_deref())
        }
        Cmd::Send { device, bytes } => devices::send(&device, &bytes),
        Cmd::Led { device, pad, button, color } => devices::led(&device, pad, button, &color),
    }
}

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// docs/cli.md is generated from the clap definitions. Run
    /// `UPDATE_DOCS=1 cargo test cli_docs` after changing the CLI.
    #[test]
    fn cli_docs_are_up_to_date() {
        let mut root = Cli::command().term_width(100);
        root.build();
        let mut md = String::from(
            "# CLI reference\n\n<!-- Generated from the clap definitions by `UPDATE_DOCS=1 cargo test cli_docs`. Don't edit by hand. -->\n",
        );
        let mut section = |title: &str, cmd: &mut clap::Command| {
            md.push_str(&format!("\n## {title}\n\n```text\n{}\n```\n", cmd.render_long_help().to_string().trim_end()));
        };
        section("midiboard", &mut root.clone());
        for sub in root.get_subcommands_mut() {
            let name = sub.get_name().to_string();
            if name == "help" {
                continue;
            }
            section(&format!("midiboard {name}"), &mut sub.clone());
            for subsub in sub.get_subcommands_mut() {
                let n2 = subsub.get_name().to_string();
                if n2 != "help" {
                    section(&format!("midiboard {name} {n2}"), &mut subsub.clone());
                }
            }
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/cli.md");
        if std::env::var_os("UPDATE_DOCS").is_some() {
            std::fs::write(path, &md).unwrap();
        }
        let current = std::fs::read_to_string(path).unwrap_or_default();
        assert!(current == md, "docs/cli.md is out of date; run `UPDATE_DOCS=1 cargo test cli_docs`");
    }

    #[test]
    fn generate_then_validate() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/config.json");
        generate(&p, false).unwrap();
        assert!(generate(&p, false).is_err(), "refuses to overwrite");
        generate(&p, true).unwrap();
        validate(&p).unwrap();
    }
}
