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
//! contracts are in place. The ONC RPC / XDR layer ([`transport::rpc`]) is
//! implemented and tested (M1); the VXI-11, raw, discovery, screenshot and
//! benchmark layers are still stubbed with `todo!()`. See `docs/roadmap.md`
//! for the order they are intended to land in.
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
pub mod device;
pub mod discovery;
pub mod error;
pub mod screenshot;
pub mod transport;

pub use device::{Device, DiscoveryMethod, Target, Transport};
pub use error::{Error, Result};
pub use transport::{ScpiTransport, Session};

/// The crate version, surfaced by the CLI and the About panel.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
