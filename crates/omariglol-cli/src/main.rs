//! The `riglol` command-line interface.
//!
//! Subcommands mirror `lxi-tools` so that muscle memory transfers directly.
//!
//! ## Status
//!
//! **Implemented (M8).** `discover`, `scpi`, `screenshot` and `benchmark` are
//! wired to `omariglol-core`. `run` is defined but still stubbed: it needs the
//! embedded Python engine from M9, which has not landed yet.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use omariglol_core::benchmark::{self, BenchmarkOptions, BenchmarkReport};
use omariglol_core::device::{Device, Target, Transport};
use omariglol_core::transport::{self, Session, parse_address, to_hex};
use omariglol_core::{discovery, screenshot};

/// Process-wide verbosity, set once from the global `-v` flag.
static VERBOSITY: AtomicU8 = AtomicU8::new(0);

/// Current verbosity level.
fn verbosity() -> u8 {
    VERBOSITY.load(Ordering::Relaxed)
}

/// Print a diagnostic to stderr when the verbosity level is high enough.
macro_rules! vlog {
    ($level:expr, $($arg:tt)*) => {
        if verbosity() >= $level {
            eprintln!($($arg)*);
        }
    };
}

/// Talk to LXI / SCPI test instruments over VXI-11 or raw TCP.
#[derive(Debug, Parser)]
#[command(
    name = "riglol",
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
    VERBOSITY.store(cli.verbose, Ordering::Relaxed);
    match cli.command {
        Command::Discover(args) => cmd_discover(args),
        Command::Scpi(args) => cmd_scpi(args),
        Command::Screenshot(args) => cmd_screenshot(args),
        Command::Benchmark(args) => cmd_benchmark(args),
        Command::Run(args) => cmd_run(args),
    }
}

// ---------------------------------------------------------------------------
// discover
// ---------------------------------------------------------------------------

fn cmd_discover(args: DiscoverArgs) -> anyhow::Result<()> {
    let timeout = Duration::from_secs(args.timeout);
    vlog!(
        1,
        "discovering instruments (mdns={}, timeout={}s)",
        args.mdns,
        args.timeout
    );

    let devices = discovery::discover(timeout, args.mdns)?;
    vlog!(1, "found {} instrument(s)", devices.len());

    if args.json {
        println!("{}", serde_json::to_string_pretty(&devices)?);
    } else {
        print_device_table(&devices);
    }
    Ok(())
}

/// Render discovered instruments as an aligned table.
fn print_device_table(devices: &[Device]) {
    if devices.is_empty() {
        println!("No instruments found.");
        return;
    }

    let headers = ["ADDRESS", "NAME", "VXI11", "RAW", "METHOD"];
    let rows: Vec<Vec<String>> = devices
        .iter()
        .map(|device| {
            vec![
                device.address.to_string(),
                device.label(),
                port_or_dash(device.vxi11_port),
                port_or_dash(device.raw_port),
                device.method.to_string(),
            ]
        })
        .collect();

    print_table(&headers, &rows);
}

/// Render an optional port as a string, or `-` when absent.
fn port_or_dash(port: Option<u16>) -> String {
    match port {
        Some(port) => port.to_string(),
        None => "-".to_string(),
    }
}

/// Print a simple left-aligned table with a header and a rule.
fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let render = |cells: &[String]| -> String {
        let mut line = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            line.push_str(&format!("{:<width$}", cell, width = widths[i]));
        }
        line.trim_end().to_string()
    };

    let header_cells: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    println!("{}", render(&header_cells));
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    println!("{}", render(&rule));
    for row in rows {
        println!("{}", render(row));
    }
}

// ---------------------------------------------------------------------------
// scpi
// ---------------------------------------------------------------------------

fn cmd_scpi(args: ScpiArgs) -> anyhow::Result<()> {
    let mut session = open_session(&args.address, args.raw, args.port, args.timeout)?;
    vlog!(1, "connected: {}", session.description());

    if args.commands.is_empty() {
        interactive(&mut session, args.hex)?;
    } else {
        for command in &args.commands {
            run_command(&mut session, command, args.hex)?;
        }
    }
    Ok(())
}

/// Open a session for an address, applying the transport, port and timeout.
fn open_session(
    address: &str,
    raw: bool,
    port: Option<u16>,
    timeout_secs: u64,
) -> anyhow::Result<Session> {
    let ip = parse_address(address)?;
    let transport = if raw {
        Transport::Raw
    } else {
        Transport::Vxi11
    };
    let mut target = Target::new(ip, transport).with_timeout(Duration::from_secs(timeout_secs));
    if let Some(port) = port {
        target = target.with_port(port);
    }
    vlog!(
        1,
        "opening {} to {}:{}",
        transport,
        target.address,
        target.effective_port()
    );
    Ok(Session::open(target)?)
}

/// Send one command. A query (ending in `?`) prints its response; a write does
/// not. `--hex` renders the raw response bytes.
fn run_command(session: &mut Session, command: &str, hex: bool) -> anyhow::Result<()> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(());
    }

    if command.ends_with('?') {
        let bytes = session.query_raw(command)?;
        if hex {
            println!("{}", to_hex(&bytes));
        } else {
            println!("{}", String::from_utf8_lossy(&bytes).trim());
        }
    } else {
        session.send(command)?;
    }
    Ok(())
}

/// Read commands from stdin until EOF or `quit`/`exit`.
fn interactive(session: &mut Session, hex: bool) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut stdout = std::io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            // EOF (Ctrl-D).
            println!();
            break;
        }

        let command = line.trim();
        if command.is_empty() {
            continue;
        }
        if command == "quit" || command == "exit" {
            break;
        }

        if let Err(err) = run_command(session, command, hex) {
            eprintln!("error: {err}");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// screenshot
// ---------------------------------------------------------------------------

fn cmd_screenshot(args: ScreenshotArgs) -> anyhow::Result<()> {
    let ip = parse_address(&args.address)?;
    let transport_kind = if args.raw {
        Transport::Raw
    } else {
        Transport::Vxi11
    };
    let target = Target::new(ip, transport_kind).with_timeout(Duration::from_secs(args.timeout));

    let mut transport = transport::open(&target)?;
    vlog!(1, "connected: {}", transport.description());

    let idn_bytes = transport.query("*IDN?")?;
    let idn = String::from_utf8_lossy(&idn_bytes).trim().to_string();
    vlog!(1, "instrument: {idn}");

    let shot = screenshot::capture(transport.as_mut(), &idn, args.command.as_deref())?;
    std::fs::write(&args.file, &shot.data)?;

    println!(
        "wrote {} ({} bytes, {}, profile {})",
        args.file.display(),
        shot.data.len(),
        shot.format.extension(),
        shot.profile
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// benchmark
// ---------------------------------------------------------------------------

fn cmd_benchmark(args: BenchmarkArgs) -> anyhow::Result<()> {
    let ip = parse_address(&args.address)?;
    let transport_kind = if args.raw {
        Transport::Raw
    } else {
        Transport::Vxi11
    };
    let target = Target::new(ip, transport_kind).with_timeout(Duration::from_secs(3));

    let mut transport = transport::open(&target)?;
    vlog!(1, "connected: {}", transport.description());

    let options = BenchmarkOptions {
        command: args.command.clone(),
        iterations: args.count,
        ..Default::default()
    };

    let json = args.json;
    let total = args.count;
    let report = benchmark::run(transport.as_mut(), &options, |iteration, sample_ms| {
        if !json {
            eprint!("\r  {iteration}/{total} ({sample_ms:.2} ms)");
        }
    })?;
    if !json {
        eprintln!();
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_benchmark_table(&report);
    }
    Ok(())
}

/// Render a benchmark report as a two-column table.
fn print_benchmark_table(report: &BenchmarkReport) {
    let rows: Vec<Vec<String>> = vec![
        vec!["command".to_string(), report.command.clone()],
        vec!["iterations".to_string(), report.iterations.to_string()],
        vec!["failures".to_string(), report.failures.to_string()],
        vec![
            "elapsed".to_string(),
            format!("{:.2} ms", report.elapsed_ms),
        ],
        vec!["min".to_string(), format!("{:.3} ms", report.min_ms)],
        vec!["max".to_string(), format!("{:.3} ms", report.max_ms)],
        vec!["mean".to_string(), format!("{:.3} ms", report.mean_ms)],
        vec!["median".to_string(), format!("{:.3} ms", report.median_ms)],
        vec!["p95".to_string(), format!("{:.3} ms", report.p95_ms)],
        vec!["stddev".to_string(), format!("{:.3} ms", report.stddev_ms)],
        vec![
            "throughput".to_string(),
            format!("{:.1} msg/s", report.messages_per_second),
        ],
        vec![
            "bandwidth".to_string(),
            format!("{:.2} KiB/s", report.kbytes_per_second()),
        ],
        vec!["bytes".to_string(), report.bytes_received.to_string()],
    ];
    print_table(&["METRIC", "VALUE"], &rows);
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn cmd_run(args: RunArgs) -> anyhow::Result<()> {
    let _ = args;
    anyhow::bail!(
        "`run` needs the embedded Python engine (M9), which is not implemented yet — \
         see plans/roadmap.md"
    )
}
