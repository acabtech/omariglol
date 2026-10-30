//! # omariglol-core
//!
//! The LXI / SCPI protocol core behind every omariglol frontend — the CLI, the
//! Tauri desktop app and the Omarchy shell plugin all sit on top of this crate.
//!
//! This crate is a from-scratch Rust implementation of the same
//! instrument-facing surface that `liblxi` provides to `lxi-tools`:
//!
//! * [`transport::vxi11`] — VXI-11 over ONC RPC, including the portmapper
//! * [`transport::raw`] — the LXI raw socket on TCP 5025
//! * [`discovery`] — VXI-11 broadcast and mDNS/DNS-SD browsing
//! * [`screenshot`] — IEEE-488.2 block capture with per-vendor profiles
//! * [`benchmark`] — SCPI round-trip measurement
//!
//! ## Status
//!
//! **In progress.** The module layout, the public types and the trait
//! contracts are in place. The ONC RPC / XDR layer ([`transport::rpc`], M1),
//! the VXI-11 transport ([`transport::vxi11`], M2), the raw TCP transport
//! ([`transport::raw`], M3), the shared device model plus [`Session`] wrapper
//! ([`device`], [`transport`], M4), discovery ([`discovery`], M5), screenshot
//! capture ([`screenshot`], M6) and benchmarking ([`benchmark`], M7) are
//! implemented and tested. The screenshot profile table still needs captured
//! samples from real hardware. See `docs/roadmap.md` for the order the
//! milestones are intended to land in.
//!
//! ## Design constraints
//!
//! The crate is **blocking on purpose**: instruments are slow, the protocol is
//! strictly request/response, and every caller already has somewhere to run
//! blocking work. It is also **UI-agnostic** — no GUI, CLI or Python types
//! leak in here.
//!
//! ```no_run
//! use omariglol_core::{device::{Target, Transport}, transport::Session};
//!
//! # fn main() -> omariglol_core::Result<()> {
//! let target = Target::new("192.168.1.50".parse().unwrap(), Transport::Vxi11);
//! let mut session = Session::open(target)?;
//! println!("{}", session.identify()?);
//! # Ok(())
//! # }
//! ```

// Scaffold: the modules below declare the fields, constants and variants their
// implementations will use. Drop this allow once they are implemented.
#![allow(dead_code)]

pub mod benchmark;
pub mod connection;
pub mod device;
pub mod discovery;
pub mod discovery_engine;
pub mod driver;
pub mod error;
pub mod instrument;
pub mod measure;
pub mod netconfig;
pub mod preset;
pub mod rule;
pub mod screenshot;
pub mod timeline;
pub mod transport;
pub mod trigger;
pub mod workspace;

pub use device::{Device, DiscoveryMethod, Target, Transport};
pub use error::{Error, Result};
pub use transport::{ScpiTransport, Session};

// Multi-instrument platform entities (concepts.md §4).
pub use connection::Connection;
pub use driver::{InstrumentDriver, InstrumentType};
pub use instrument::{Instrument, InstrumentConfig, InstrumentId, InstrumentState};
pub use measure::{MeasureReport, MeasurementResult, measure_workspace};
pub use preset::{Preset, PresetPlan, PresetStep};
pub use rule::{Comparison, EvaluationMode, Rule, RuleDecision};
pub use timeline::{Timeline, TimelineEvent, TimelineEventKind};
pub use trigger::{Trigger, TriggerError, TriggerState};
pub use workspace::{Workspace, WorkspaceId, WorkspaceManager, WorkspaceMetadata};

// Discovery subsystem.
pub use discovery::{ProbeResult, SCPI_PROBE_PORTS};
pub use discovery_engine::{
    CachedInstrument, DiscoveryCache, DiscoveryEngine, DiscoveryEvent, DiscoveryOptions,
    DiscoveryReport,
};
pub use netconfig::{InterfaceInfo, NetworkConfig};

/// The crate version, surfaced by the CLI and the About panel.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
