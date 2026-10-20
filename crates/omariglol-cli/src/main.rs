//! The `omariglol` command-line interface.
//!
//! Subcommands mirror `lxi-tools` so that muscle memory transfers directly.
//!
//! ## Status
//!
//! **Scaffold.** The command surface, arguments and help text are defined;
//! every handler is stubbed.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

/// Talk to LXI / SCPI test instruments over VXI-11 or raw TCP.
#[derive(Debug, Parser)]
#[command(
    name = "omariglol",
    version,
    about = "Talk to LXI / SCPI test instruments",
    long_about = "A native reimplementation of lxi-tools with Python scripting \
                  and Omarchy theming.\n\n\
                  Instruments are addressed by IPv4 address or hostname.",
    propagate_version = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Increase log verbosity (-v, -vv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Find LXI instruments on the network.
    Discover(DiscoverArgs),

    /// Send a SCPI command, or start an interactive session.
    Scpi(ScpiArgs),

    /// Capture an instrument's screen to an image file.
    Screenshot(ScreenshotArgs),

    /// Measure SCPI round-trip performance.
    Benchmark(BenchmarkArgs),

    /// Run a Python measurement script.
    Run(RunArgs),
}

#[derive(Debug, Args)]
pub struct DiscoverArgs {
    /// Also browse mDNS/DNS-SD, finding instruments the broadcast misses.
    #[arg(long)]
    pub mdns: bool,

    /// How long to listen, in seconds.
    #[arg(long, default_value_t = 3, value_name = "SECONDS")]
    pub timeout: u64,

    /// Emit JSON instead of a table, for scripting.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ScpiArgs {
    /// Instrument address, e.g. 192.168.1.50 or scope.local.
    #[arg(value_name = "ADDRESS")]
    pub address: String,

    /// SCPI commands to send. Omit to start an interactive session.
    #[arg(value_name = "COMMAND", trailing_var_arg = true)]
    pub commands: Vec<String>,

    /// Use the raw TCP socket instead of VXI-11.
    #[arg(long)]
    pub raw: bool,

    /// Override the port.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Print responses as hex, which matters for binary blocks.
    #[arg(long)]
    pub hex: bool,

    /// Socket timeout in seconds.
    #[arg(long, default_value_t = 3, value_name = "SECONDS")]
    pub timeout: u64,
}

#[derive(Debug, Args)]
pub struct ScreenshotArgs {
    /// Instrument address.
    #[arg(value_name = "ADDRESS")]
    pub address: String,

    /// Where to write the image.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Use the raw TCP socket instead of VXI-11.
    #[arg(long)]
    pub raw: bool,

    /// Override the capture query, for instruments without a built-in profile.
    #[arg(long, value_name = "COMMAND")]
    pub command: Option<String>,

    /// Socket timeout in seconds.
    #[arg(long, default_value_t = 3, value_name = "SECONDS")]
    pub timeout: u64,
}

#[derive(Debug, Args)]
pub struct BenchmarkArgs {
    /// Instrument address.
    #[arg(value_name = "ADDRESS")]
    pub address: String,

    /// Number of round trips.
    #[arg(short, long, default_value_t = 100, value_name = "N")]
    pub count: usize,

    /// The query to repeat.
    #[arg(long, default_value = "*IDN?", value_name = "COMMAND")]
    pub command: String,

    /// Use the raw TCP socket instead of VXI-11.
    #[arg(long)]
    pub raw: bool,

    /// Emit JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// The Python script to execute.
    #[arg(value_name = "SCRIPT")]
    pub script: PathBuf,

    /// Arguments passed through to the script as sys.argv.
    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    pub args: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Discover(args) => cmd_discover(args),
        Command::Scpi(args) => cmd_scpi(args),
        Command::Screenshot(args) => cmd_screenshot(args),
        Command::Benchmark(args) => cmd_benchmark(args),
        Command::Run(args) => cmd_run(args),
    }
}

fn cmd_discover(args: DiscoverArgs) -> anyhow::Result<()> {
    let _ = (args.mdns, Duration::from_secs(args.timeout), args.json);
    anyhow::bail!("`discover` is not implemented yet — see docs/roadmap.md")
}

fn cmd_scpi(args: ScpiArgs) -> anyhow::Result<()> {
    let _ = args;
    anyhow::bail!("`scpi` is not implemented yet — see docs/roadmap.md")
}

fn cmd_screenshot(args: ScreenshotArgs) -> anyhow::Result<()> {
    let _ = args;
    anyhow::bail!("`screenshot` is not implemented yet — see docs/roadmap.md")
}

fn cmd_benchmark(args: BenchmarkArgs) -> anyhow::Result<()> {
    let _ = args;
    anyhow::bail!("`benchmark` is not implemented yet — see docs/roadmap.md")
}

fn cmd_run(args: RunArgs) -> anyhow::Result<()> {
    let _ = args;
    anyhow::bail!("`run` is not implemented yet — see docs/roadmap.md")
}
