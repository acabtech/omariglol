//! The `riglol` command-line interface.
//!
//! Subcommands mirror `lxi-tools` so that muscle memory transfers directly.
//!
//! ## Status
//!
//! **Implemented (M8).** `discover`, `scpi`, `screenshot`, `benchmark` and
//! `run` are all wired: the first four to `omariglol-core`, and `run` to the
//! embedded Python engine in `omariglol-script` (M9).

use std::io::{BufRead, Write};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use omariglol_core::benchmark::{self, BenchmarkOptions, BenchmarkReport};
use omariglol_core::device::{Target, Transport};
use omariglol_core::discovery_engine::{
    CachedInstrument, DiscoveryEngine, DiscoveryEvent, DiscoveryOptions,
};
use omariglol_core::driver::InstrumentDriver;
use omariglol_core::netconfig;
use omariglol_core::transport::{self, Session, parse_address, to_hex};
use omariglol_core::workspace::WorkspaceManager;
use omariglol_core::screenshot;
use omariglol_script::{ScriptEngine, ScriptEvent, ScriptOptions};

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

    /// Show the driver (command surface) for an instrument.
    Driver(DriverArgs),

    /// Manage saved workspaces.
    Workspace(WorkspaceArgs),
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

    /// Scan the local subnet with direct SCPI `*IDN?` probes over TCP.
    ///
    /// This is the default; the flag is kept for explicitness.
    #[arg(long)]
    pub scan: bool,

    /// Skip the subnet scan and only use broadcast and mDNS.
    #[arg(long)]
    pub no_scan: bool,

    /// Additional hosts to probe, repeatable (e.g. --host 192.168.1.50).
    #[arg(long = "host", value_name = "ADDRESS")]
    pub hosts: Vec<String>,

    /// Cache TTL in seconds.
    #[arg(long, default_value_t = 300, value_name = "SECONDS")]
    pub ttl: u64,

    /// Probe every host, ignoring the cache.
    #[arg(long)]
    pub no_incremental: bool,
}

#[derive(Debug, Args)]
pub struct DriverArgs {
    /// Instrument address.
    #[arg(value_name = "ADDRESS")]
    pub address: String,

    /// Use the raw TCP socket instead of VXI-11.
    #[arg(long)]
    pub raw: bool,

    /// Override the port.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Socket timeout in seconds.
    #[arg(long, default_value_t = 3, value_name = "SECONDS")]
    pub timeout: u64,

    /// Emit JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// List saved workspaces.
    List,

    /// Create a new workspace.
    Create {
        /// The workspace name.
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// Show a workspace's instruments and artifacts.
    Show {
        /// The workspace name or id.
        #[arg(value_name = "NAME")]
        name: String,
    },
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

    /// Use the raw TCP socket (the default; kept for explicitness).
    #[arg(long, conflicts_with = "vxi11")]
    pub raw: bool,

    /// Use VXI-11 instead of the default raw TCP socket.
    #[arg(long)]
    pub vxi11: bool,

    /// Override the port (e.g. 5555 for Rigol's raw SCPI socket).
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

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
    #[arg(short, long, default_value_t = 25, value_name = "N")]
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
        Command::Driver(args) => cmd_driver(args),
        Command::Workspace(args) => cmd_workspace(args),
    }
}

// ---------------------------------------------------------------------------
// discover
// ---------------------------------------------------------------------------

fn cmd_discover(args: DiscoverArgs) -> anyhow::Result<()> {
    // Discovery always runs the full engine: broadcast and mDNS find
    // self-advertising instruments, and the subnet scan finds the rest. The
    // strategies run concurrently, so a full scan stays within budget.
    cmd_discover_scan(args)
}

/// Run the multi-strategy discovery engine, including a subnet scan.
fn cmd_discover_scan(args: DiscoverArgs) -> anyhow::Result<()> {
    let config = netconfig::detect();
    match &config.primary {
        Some(interface) => vlog!(
            1,
            "network: {}/{} on {}{}",
            interface.address,
            interface.prefix_len,
            interface.name,
            interface
                .vlan
                .map(|vlan| format!(" (vlan {vlan})"))
                .unwrap_or_default()
        ),
        None => vlog!(1, "network: offline"),
    }

    let extra_hosts: Vec<IpAddr> = args
        .hosts
        .iter()
        .filter_map(|host| host.parse().ok())
        .collect();
    let options = DiscoveryOptions {
        use_broadcast: true,
        use_mdns: args.mdns,
        scan_subnet: !args.no_scan,
        extra_hosts,
        ttl_secs: args.ttl,
        incremental: !args.no_incremental,
        listen_timeout: Duration::from_secs(args.timeout),
        ..Default::default()
    };

    let engine = DiscoveryEngine::new(args.ttl);
    let report = engine.run(&options, |event| {
        if verbosity() >= 1
            && let DiscoveryEvent::InstrumentFound { instrument } = event
        {
            eprintln!("  found {} at {}", instrument.model, instrument.address());
        }
    });

    vlog!(
        1,
        "probed {} host(s), skipped {}, found {} in {:.0} ms",
        report.probed_hosts,
        report.skipped_hosts,
        report.found.len(),
        report.elapsed_ms
    );

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_cached_table(&report.found);
    }
    Ok(())
}

/// Render cached discovery results as an aligned table.
fn print_cached_table(instruments: &[CachedInstrument]) {
    if instruments.is_empty() {
        println!("No instruments found.");
        return;
    }
    let headers = ["ADDRESS", "MODEL", "SERIAL", "VLAN", "METHOD"];
    let rows: Vec<Vec<String>> = instruments
        .iter()
        .map(|instrument| {
            vec![
                instrument.address().to_string(),
                dash_if_empty(&instrument.model),
                dash_if_empty(&instrument.serial),
                instrument
                    .vlan
                    .map(|vlan| vlan.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                instrument.device.method.to_string(),
            ]
        })
        .collect();
    print_table(&headers, &rows);
}

/// Render a string, or `-` when it is empty.
fn dash_if_empty(value: &str) -> String {
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_string()
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
    let timeout = Duration::from_secs(args.timeout);

    // Raw TCP is the default image-acquisition protocol: it streams the image
    // at TCP speed, whereas VXI-11 caps each read at the instrument's
    // `maxRecvSize` (often a single Ethernet MTU), which makes large captures
    // crawl. `--raw` is accepted for explicitness; `--vxi11` opts back in.
    let transport_kind = match (args.raw, args.vxi11) {
        (_, true) => Transport::Vxi11,
        _ => Transport::Raw,
    };

    let mut transport = if transport_kind == Transport::Vxi11 {
        let target = Target::new(ip, Transport::Vxi11).with_timeout(timeout);
        transport::open(&target)?
    } else if let Some(port) = args.port {
        let target = Target::new(ip, Transport::Raw)
            .with_port(port)
            .with_timeout(timeout);
        transport::open(&target)?
    } else {
        // No port given: probe the known vendor raw ports (Rigol 5555, the
        // LXI-standard 5025) and use the first that answers.
        screenshot::open_default_transport(ip, timeout)?
    };
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
    let mut engine = ScriptEngine::new()?;
    let options = ScriptOptions {
        args: args.args.clone(),
        ..Default::default()
    };

    vlog!(1, "running {}", args.script.display());
    let result = engine.run_file(&args.script, &options, |event| match event {
        ScriptEvent::Stdout { text } => {
            print!("{text}");
            let _ = std::io::stdout().flush();
        }
        ScriptEvent::Stderr { text } => {
            eprint!("{text}");
            let _ = std::io::stderr().flush();
        }
        ScriptEvent::Sample { series, value } => {
            vlog!(1, "[sample] {series} = {value}");
        }
        ScriptEvent::Status { message } => {
            vlog!(1, "[status] {message}");
        }
    })?;

    vlog!(1, "finished in {:.1} ms", result.duration_ms);

    // Mirror CPython: a non-zero `sys.exit()` code becomes the process status.
    if let Some(code) = result.exit_code {
        if code != 0 {
            std::process::exit(code);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// driver
// ---------------------------------------------------------------------------

fn cmd_driver(args: DriverArgs) -> anyhow::Result<()> {
    let mut session = open_session(&args.address, args.raw, args.port, args.timeout)?;
    let idn = session.identify()?;
    let driver = InstrumentDriver::from_idn(&idn);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&driver)?);
        return Ok(());
    }

    println!("instrument: {idn}");
    println!("type:       {}", driver.instrument_type.label());
    if let Some(profile) = &driver.profile {
        println!("profile:    {profile}");
    }
    if let Some(screenshot) = &driver.screenshot_profile {
        println!("screenshot: {screenshot}");
    }
    println!();

    let rows: Vec<Vec<String>> = driver
        .commands
        .iter()
        .map(|command| {
            vec![
                command.group.clone(),
                command.name.clone(),
                command.scpi.clone(),
                if command.query { "query" } else { "write" }.to_string(),
                command.description.clone(),
            ]
        })
        .collect();
    print_table(&["GROUP", "NAME", "SCPI", "KIND", "DESCRIPTION"], &rows);
    Ok(())
}

// ---------------------------------------------------------------------------
// workspace
// ---------------------------------------------------------------------------

/// The directory workspaces are persisted to, matching the desktop app.
fn workspace_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("omariglol")
            .join("workspaces");
    }
    PathBuf::from("omariglol-workspaces")
}

fn cmd_workspace(args: WorkspaceArgs) -> anyhow::Result<()> {
    let dir = workspace_dir();
    let mut manager = WorkspaceManager::with_dir(&dir);
    manager.load_all()?;

    match args.command {
        WorkspaceCommand::List => {
            if manager.list().is_empty() {
                println!("No workspaces in {}.", dir.display());
                return Ok(());
            }
            let rows: Vec<Vec<String>> = manager
                .list()
                .iter()
                .map(|workspace| {
                    vec![
                        workspace.id.to_string(),
                        workspace.name.clone(),
                        workspace.instruments.len().to_string(),
                        workspace.connected_count().to_string(),
                    ]
                })
                .collect();
            print_table(&["ID", "NAME", "INSTRUMENTS", "CONNECTED"], &rows);
        }
        WorkspaceCommand::Create { name } => {
            let id = manager.create(name);
            manager.save(&id)?;
            println!("created {id} in {}", dir.display());
        }
        WorkspaceCommand::Show { name } => {
            let workspace = manager
                .list()
                .iter()
                .find(|workspace| workspace.name == name || workspace.id.as_str() == name)
                .ok_or_else(|| anyhow::anyhow!("no workspace '{name}' in {}", dir.display()))?;
            println!("workspace: {} ({})", workspace.name, workspace.id);
            println!("instruments: {}", workspace.instruments.len());
            if !workspace.instruments.is_empty() {
                let rows: Vec<Vec<String>> = workspace
                    .instruments
                    .iter()
                    .map(|instrument| {
                        vec![
                            instrument.id.to_string(),
                            instrument.label(),
                            instrument.state.to_string(),
                            instrument.idn.clone().unwrap_or_else(|| "-".to_string()),
                        ]
                    })
                    .collect();
                print_table(&["ID", "LABEL", "STATE", "IDN"], &rows);
            }
            println!("presets: {}", workspace.presets.len());
            println!("rules: {}", workspace.rules.len());
            println!("triggers: {}", workspace.triggers.len());
            println!("timeline events: {}", workspace.timeline.len());
        }
    }
    Ok(())
}
