//! Per-instrument driver abstraction (concepts.md §4.1, §6.2).
//!
//! The platform is vendor-neutral: anything that speaks SCPI is an instrument.
//! A [`InstrumentDriver`] is a *description* of an instrument family — its type,
//! vendor, and the named operations it supports — derived from the `*IDN?`
//! string and, where available, the screenshot profile table.
//!
//! Drivers are data, not code (concepts.md §8.1): the UI renders them and the
//! scripting layer addresses them by name. Nothing here performs I/O.
//!
//! ## Generalisation vs. depth
//!
//! Every instrument gets a *generic* command set derived from its
//! [`InstrumentType`]. Instruments we have first-hand knowledge of — currently
//! the Rigol DS1000Z family (the DS1054Z) — additionally get a *rich* profile
//! that exposes the full operation surface (run/stop/single/auto, trigger
//! configuration, per-channel setup, timebase, measurements, acquisition and
//! waveform transfer). The rich profile is selected by `*IDN?` prefix, so
//! adding another family is a matter of adding another table, never of changing
//! the generic path.

use serde::{Deserialize, Serialize};

use crate::screenshot::profile_for_idn;

/// The broad class of an instrument, inferred from its `*IDN?` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstrumentType {
    Oscilloscope,
    PowerSupply,
    SignalGenerator,
    Multimeter,
    SpectrumAnalyzer,
    ElectronicLoad,
    SwitchModule,
    Unknown,
}

impl InstrumentType {
    /// A human label for the type.
    pub fn label(&self) -> &'static str {
        match self {
            InstrumentType::Oscilloscope => "oscilloscope",
            InstrumentType::PowerSupply => "power supply",
            InstrumentType::SignalGenerator => "signal generator",
            InstrumentType::Multimeter => "multimeter",
            InstrumentType::SpectrumAnalyzer => "spectrum analyzer",
            InstrumentType::ElectronicLoad => "electronic load",
            InstrumentType::SwitchModule => "switch module",
            InstrumentType::Unknown => "instrument",
        }
    }

    /// A short glyph used by the topology view.
    pub fn glyph(&self) -> &'static str {
        match self {
            InstrumentType::Oscilloscope => "scope",
            InstrumentType::PowerSupply => "psu",
            InstrumentType::SignalGenerator => "gen",
            InstrumentType::Multimeter => "dmm",
            InstrumentType::SpectrumAnalyzer => "sa",
            InstrumentType::ElectronicLoad => "load",
            InstrumentType::SwitchModule => "sw",
            InstrumentType::Unknown => "inst",
        }
    }
}

/// Classify an `*IDN?` string into an [`InstrumentType`].
///
/// The heuristic is deliberately conservative: it matches on well-known model
/// prefixes and falls back to [`InstrumentType::Unknown`] rather than guessing.
pub fn classify_idn(idn: &str) -> InstrumentType {
    let upper = idn.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return InstrumentType::Unknown;
    }

    const OSCILLOSCOPE: &[&str] = &[
        "DSO",
        "MSO",
        "DS1",
        "DS2",
        "DS4",
        "DS6",
        "DSOX",
        "WAVESURFER",
    ];
    const POWER_SUPPLY: &[&str] = &["N6705", "E363", "DP8", "DP7", "DP9", "PSU", "PWR"];
    const GENERATOR: &[&str] = &["33500", "33600", "AFG", "DG1", "DG4", "DG8", "SMB", "SMW"];
    const MULTIMETER: &[&str] = &["34401", "34410", "3446", "DMM", "DM3", "DM8"];
    const SPECTRUM: &[&str] = &[
        "N9020", "N9010", "FSW", "FSU", "FSP", "RSA", "DSA8", "SPECTRUM",
    ];
    const LOAD: &[&str] = &["N3300", "DL3", "EL3", "LOAD"];
    const SWITCH: &[&str] = &["34970", "34980", "SWITCH", "MUX"];

    let model = idn_model(&upper);
    let haystack = if model.is_empty() {
        upper.as_str()
    } else {
        model.as_str()
    };

    let matches = |prefixes: &[&str]| prefixes.iter().any(|p| haystack.contains(p));

    if matches(OSCILLOSCOPE) {
        InstrumentType::Oscilloscope
    } else if matches(POWER_SUPPLY) {
        InstrumentType::PowerSupply
    } else if matches(GENERATOR) {
        InstrumentType::SignalGenerator
    } else if matches(MULTIMETER) {
        InstrumentType::Multimeter
    } else if matches(SPECTRUM) {
        InstrumentType::SpectrumAnalyzer
    } else if matches(LOAD) {
        InstrumentType::ElectronicLoad
    } else if matches(SWITCH) {
        InstrumentType::SwitchModule
    } else {
        InstrumentType::Unknown
    }
}

/// Extract the model field (the second comma-separated field) from an IDN.
fn idn_model(upper: &str) -> String {
    upper
        .split(',')
        .nth(1)
        .map(|model| model.trim().to_string())
        .unwrap_or_default()
}

/// The vendor field (the first comma-separated field) from an IDN.
pub fn idn_vendor(idn: &str) -> String {
    idn.split(',')
        .next()
        .map(|vendor| vendor.trim().to_string())
        .unwrap_or_default()
}

/// The model field (the second comma-separated field) from an IDN.
pub fn idn_model_name(idn: &str) -> String {
    idn.split(',')
        .nth(1)
        .map(|model| model.trim().to_string())
        .unwrap_or_default()
}

/// A named operation a driver exposes, mapping to a SCPI command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverCommand {
    /// The operation name, e.g. `measure_frequency`.
    pub name: String,
    /// The SCPI command or query it maps to.
    pub scpi: String,
    /// True when the command is a query (returns a response).
    pub query: bool,
    /// A short human description, for the UI.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// A command group, e.g. `trigger`, `channel`, `measure`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
}

impl DriverCommand {
    /// Build a command with no description or group.
    pub fn new(name: &str, scpi: &str, query: bool) -> Self {
        DriverCommand {
            name: name.to_string(),
            scpi: scpi.to_string(),
            query,
            description: String::new(),
            group: String::new(),
        }
    }

    /// Attach a description and group.
    pub fn described(mut self, group: &str, description: &str) -> Self {
        self.group = group.to_string();
        self.description = description.to_string();
        self
    }
}

/// A description of an instrument family's capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentDriver {
    /// The inferred instrument type.
    pub instrument_type: InstrumentType,
    /// The vendor, from the IDN.
    pub vendor: String,
    /// The model, from the IDN.
    pub model: String,
    /// The name of the rich profile in use, when one matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The screenshot profile name, when one matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_profile: Option<String>,
    /// The named operations this driver exposes.
    pub commands: Vec<DriverCommand>,
}

impl InstrumentDriver {
    /// Build a driver description from an `*IDN?` string.
    ///
    /// A known family's rich profile is preferred; otherwise the generic
    /// type-based command set is used.
    pub fn from_idn(idn: &str) -> Self {
        let instrument_type = classify_idn(idn);
        let profile = profile_for_idn(idn).map(|p| p.name.to_string());
        let (profile_name, commands) = match vendor_commands(idn) {
            Some((name, commands)) => (Some(name.to_string()), commands),
            None => (None, generic_commands(instrument_type)),
        };
        InstrumentDriver {
            instrument_type,
            vendor: idn_vendor(idn),
            model: idn_model_name(idn),
            profile: profile_name,
            screenshot_profile: profile,
            commands,
        }
    }

    /// Look up a named operation.
    pub fn command(&self, name: &str) -> Option<&DriverCommand> {
        self.commands.iter().find(|c| c.name == name)
    }

    /// The distinct command groups, in first-seen order.
    pub fn groups(&self) -> Vec<String> {
        let mut groups = Vec::new();
        for command in &self.commands {
            if !command.group.is_empty() && !groups.contains(&command.group) {
                groups.push(command.group.clone());
            }
        }
        groups
    }

    /// The commands in a group.
    pub fn commands_in_group(&self, group: &str) -> Vec<&DriverCommand> {
        self.commands.iter().filter(|c| c.group == group).collect()
    }
}

/// The standard operations every instrument supports, plus type-specific ones.
fn generic_commands(instrument_type: InstrumentType) -> Vec<DriverCommand> {
    let mut commands = standard_commands();

    match instrument_type {
        InstrumentType::Oscilloscope => {
            commands.extend([
                DriverCommand::new("run", ":RUN", false).described("acquire", "Start acquisition"),
                DriverCommand::new("stop", ":STOP", false).described("acquire", "Stop acquisition"),
                DriverCommand::new("single", ":SING", false).described("acquire", "Single sweep"),
                DriverCommand::new("autoscale", ":AUT", false).described("acquire", "Auto-scale"),
                DriverCommand::new("measure_frequency", ":MEAS:FREQ?", true)
                    .described("measure", "Measure frequency"),
                DriverCommand::new("measure_vpp", ":MEAS:VPP?", true)
                    .described("measure", "Measure peak-to-peak"),
                DriverCommand::new("initiate", ":INIT", false).described("acquire", "Initiate"),
            ]);
        }
        InstrumentType::PowerSupply => {
            commands.extend([
                DriverCommand::new("set_voltage", ":VOLT", false)
                    .described("output", "Set voltage"),
                DriverCommand::new("read_voltage", ":VOLT?", true)
                    .described("output", "Read voltage"),
                DriverCommand::new("set_current", ":CURR", false)
                    .described("output", "Set current"),
                DriverCommand::new("output_on", ":OUTP ON", false).described("output", "Output on"),
                DriverCommand::new("output_off", ":OUTP OFF", false)
                    .described("output", "Output off"),
            ]);
        }
        InstrumentType::SignalGenerator => {
            commands.extend([
                DriverCommand::new("set_frequency", ":FREQ", false)
                    .described("output", "Set frequency"),
                DriverCommand::new("set_amplitude", ":VOLT", false)
                    .described("output", "Set amplitude"),
                DriverCommand::new("output_on", ":OUTP ON", false).described("output", "Output on"),
                DriverCommand::new("output_off", ":OUTP OFF", false)
                    .described("output", "Output off"),
            ]);
        }
        InstrumentType::Multimeter => {
            commands.extend([
                DriverCommand::new("measure_voltage", ":MEAS:VOLT?", true)
                    .described("measure", "Measure voltage"),
                DriverCommand::new("measure_current", ":MEAS:CURR?", true)
                    .described("measure", "Measure current"),
                DriverCommand::new("measure_resistance", ":MEAS:RES?", true)
                    .described("measure", "Measure resistance"),
            ]);
        }
        InstrumentType::SpectrumAnalyzer => {
            commands.extend([
                DriverCommand::new("set_center_frequency", ":FREQ:CENT", false)
                    .described("sweep", "Set center frequency"),
                DriverCommand::new("set_span", ":FREQ:SPAN", false).described("sweep", "Set span"),
                DriverCommand::new("sweep", ":INIT", false).described("sweep", "Start sweep"),
            ]);
        }
        InstrumentType::ElectronicLoad => {
            commands.extend([
                DriverCommand::new("set_current", ":CURR", false).described("input", "Set current"),
                DriverCommand::new("input_on", ":INP ON", false).described("input", "Input on"),
                DriverCommand::new("input_off", ":INP OFF", false).described("input", "Input off"),
            ]);
        }
        InstrumentType::SwitchModule => {
            commands.extend([
                DriverCommand::new("close_channel", ":ROUT:CLOS", false)
                    .described("route", "Close a channel"),
                DriverCommand::new("open_channel", ":ROUT:OPEN", false)
                    .described("route", "Open a channel"),
            ]);
        }
        InstrumentType::Unknown => {}
    }

    commands
}

/// The IEEE-488.2 common commands every instrument supports.
fn standard_commands() -> Vec<DriverCommand> {
    vec![
        DriverCommand::new("identify", "*IDN?", true)
            .described("system", "Identify the instrument"),
        DriverCommand::new("reset", "*RST", false).described("system", "Reset to defaults"),
        DriverCommand::new("clear_status", "*CLS", false).described("system", "Clear status"),
        DriverCommand::new("operation_complete", "*OPC?", true)
            .described("system", "Wait for operation complete"),
        DriverCommand::new("self_test", "*TST?", true).described("system", "Run self-test"),
        DriverCommand::new("trigger", "*TRG", false).described("trigger", "Bus trigger"),
    ]
}

/// A rich, family-specific command set selected by `*IDN?` prefix.
///
/// This is the extension point for new instruments: a supported family is one
/// entry in [`DRIVER_PROFILES`] plus a `fn() -> Vec<DriverCommand>` builder. The
/// generic, type-based path is never changed when a family is added.
pub struct DriverProfile {
    /// Friendly family name, e.g. `"Rigol DS1000Z"`.
    pub name: &'static str,
    /// Upper-cased `*IDN?` prefixes that select this profile.
    pub idn_prefixes: &'static [&'static str],
    /// Builds the family's command surface.
    pub build: fn() -> Vec<DriverCommand>,
}

/// Registered rich profiles, checked in order.
///
/// ## Adding a future scope
///
/// 1. Write a `fn <family>_commands() -> Vec<DriverCommand>` next to
///    [`ds1000z_commands`], starting from [`standard_commands`].
/// 2. Add a [`DriverProfile`] entry below with the family's `*IDN?` prefixes.
///
/// No other code needs to change: discovery, `riglol driver`, measurement and
/// the scripting API all read the resulting command table.
pub static DRIVER_PROFILES: &[DriverProfile] = &[DriverProfile {
    name: "Rigol DS1000Z",
    idn_prefixes: DS1000Z_PREFIXES,
    build: ds1000z_commands,
}];

/// The rich profile matching an `*IDN?` string, if any.
pub fn driver_profile_for_idn(idn: &str) -> Option<&'static DriverProfile> {
    let upper = idn.trim().to_ascii_uppercase();
    DRIVER_PROFILES
        .iter()
        .find(|profile| profile.idn_prefixes.iter().any(|p| upper.starts_with(p)))
}

/// A rich, family-specific command set, selected by `*IDN?` prefix.
///
/// Returns the profile name and its commands, or `None` when no rich profile
/// matches (in which case the generic set applies).
fn vendor_commands(idn: &str) -> Option<(&'static str, Vec<DriverCommand>)> {
    let profile = driver_profile_for_idn(idn)?;
    Some((profile.name, (profile.build)()))
}

/// `*IDN?` prefixes that select the Rigol DS1000Z rich profile.
const DS1000Z_PREFIXES: &[&str] = &[
    "RIGOL TECHNOLOGIES,DS1",
    "RIGOL TECHNOLOGIES,MSO1",
    "RIGOL TECHNOLOGIES,DS1000Z",
];

/// The full operation surface of the Rigol DS1000Z family (the DS1054Z).
///
/// This is the instrument we have first-hand knowledge of, so its profile is
/// exhaustive: acquisition control, trigger configuration, per-channel setup,
/// timebase, measurements, acquisition depth and waveform transfer. Every
/// command is a standard DS1000Z SCPI mnemonic, so the same profile drives the
/// DS1054Z, DS1074Z, MSO1104Z and their siblings.
fn ds1000z_commands() -> Vec<DriverCommand> {
    let mut commands = standard_commands();

    // --- Acquisition control -------------------------------------------------
    commands.extend([
        DriverCommand::new("run", ":RUN", false).described("acquire", "Start acquisition"),
        DriverCommand::new("stop", ":STOP", false).described("acquire", "Stop acquisition"),
        DriverCommand::new("single", ":SING", false).described("acquire", "Single sweep"),
        DriverCommand::new("autoscale", ":AUT", false)
            .described("acquire", "Auto-scale all channels"),
        DriverCommand::new("force_trigger", ":TFOR", false).described("acquire", "Force a trigger"),
        DriverCommand::new("acquire_type", ":ACQ:TYPE", false)
            .described("acquire", "Set acquisition type (NORM/AVER/PEAK/HRES)"),
        DriverCommand::new("acquire_averages", ":ACQ:AVER", false)
            .described("acquire", "Set the average count"),
        DriverCommand::new("acquire_depth", ":ACQ:MDEP", false)
            .described("acquire", "Set memory depth (AUTO/12K/120K/1.2M/12M/24M)"),
        DriverCommand::new("acquire_srate", ":ACQ:SRAT?", true)
            .described("acquire", "Read the sample rate"),
    ]);

    // --- Trigger configuration ----------------------------------------------
    commands.extend([
        DriverCommand::new("trigger_mode", ":TRIG:MODE", false).described(
            "trigger",
            "Set trigger mode (EDGE/PULS/RUNT/WIND/NEDG/SLOP/VIDEO/PATT/DUR)",
        ),
        DriverCommand::new("trigger_status", ":TRIG:STAT?", true)
            .described("trigger", "Read trigger status (TD/RUN/WAIT/AUTO/STOP)"),
        DriverCommand::new("trigger_sweep", ":TRIG:SWE", false)
            .described("trigger", "Set trigger sweep (AUTO/NORM/SING)"),
        DriverCommand::new("trigger_edge_source", ":TRIG:EDGE:SOUR", false)
            .described("trigger", "Set edge trigger source (CHAN1..4/AC)"),
        DriverCommand::new("trigger_edge_level", ":TRIG:EDGE:LEV", false)
            .described("trigger", "Set edge trigger level"),
        DriverCommand::new("trigger_edge_slope", ":TRIG:EDGE:SLOP", false)
            .described("trigger", "Set edge slope (POS/NEG/RFAL)"),
        DriverCommand::new("trigger_edge_coupling", ":TRIG:EDGE:COUP", false)
            .described("trigger", "Set edge coupling (AC/DC/LFRE/HFRE)"),
        DriverCommand::new("trigger_pulse_source", ":TRIG:PULS:SOUR", false)
            .described("trigger", "Set pulse trigger source"),
        DriverCommand::new("trigger_pulse_condition", ":TRIG:PULS:COND", false)
            .described("trigger", "Set pulse condition (POS/NEG)"),
        DriverCommand::new("trigger_pulse_width", ":TRIG:PULS:WIDT", false)
            .described("trigger", "Set pulse width"),
        DriverCommand::new("trigger_video_standard", ":TRIG:VID:STAN", false)
            .described("trigger", "Set video standard (PAL/NTSC)"),
        DriverCommand::new("trigger_video_source", ":TRIG:VID:SOUR", false)
            .described("trigger", "Set video trigger source"),
        DriverCommand::new("trigger_video_line", ":TRIG:VID:LINE", false)
            .described("trigger", "Set video trigger line"),
    ]);

    // --- Per-channel setup (CHAN1..CHAN4) -----------------------------------
    for channel in 1..=4 {
        let group = format!("channel{channel}");
        commands.extend([
            DriverCommand::new(
                &format!("channel{channel}_display"),
                &format!(":CHAN{channel}:DISP"),
                false,
            )
            .described(&group, "Show or hide the channel"),
            DriverCommand::new(
                &format!("channel{channel}_scale"),
                &format!(":CHAN{channel}:SCAL"),
                false,
            )
            .described(&group, "Set the vertical scale (V/div)"),
            DriverCommand::new(
                &format!("channel{channel}_offset"),
                &format!(":CHAN{channel}:OFFS"),
                false,
            )
            .described(&group, "Set the vertical offset"),
            DriverCommand::new(
                &format!("channel{channel}_coupling"),
                &format!(":CHAN{channel}:COUP"),
                false,
            )
            .described(&group, "Set coupling (AC/DC/GND)"),
            DriverCommand::new(
                &format!("channel{channel}_probe"),
                &format!(":CHAN{channel}:PROB"),
                false,
            )
            .described(&group, "Set the probe attenuation"),
            DriverCommand::new(
                &format!("channel{channel}_bandwidth_limit"),
                &format!(":CHAN{channel}:BWL"),
                false,
            )
            .described(&group, "Set the bandwidth limit (20M/OFF)"),
            DriverCommand::new(
                &format!("channel{channel}_invert"),
                &format!(":CHAN{channel}:INV"),
                false,
            )
            .described(&group, "Invert the channel"),
        ]);
    }

    // --- Timebase ------------------------------------------------------------
    commands.extend([
        DriverCommand::new("timebase_mode", ":TIM:MODE", false)
            .described("timebase", "Set timebase mode (MAIN/XY/ROLL)"),
        DriverCommand::new("timebase_scale", ":TIM:MAIN:SCAL", false)
            .described("timebase", "Set the horizontal scale (s/div)"),
        DriverCommand::new("timebase_offset", ":TIM:MAIN:OFFS", false)
            .described("timebase", "Set the horizontal offset"),
        DriverCommand::new("timebase_scale_query", ":TIM:MAIN:SCAL?", true)
            .described("timebase", "Read the horizontal scale"),
    ]);

    // --- Measurements --------------------------------------------------------
    commands.extend([
        DriverCommand::new("measure_source", ":MEAS:SOUR", false)
            .described("measure", "Set the measurement source channel"),
        DriverCommand::new("measure_vpp", ":MEAS:VPP?", true)
            .described("measure", "Measure peak-to-peak voltage"),
        DriverCommand::new("measure_vmax", ":MEAS:VMAX?", true)
            .described("measure", "Measure maximum voltage"),
        DriverCommand::new("measure_vmin", ":MEAS:VMIN?", true)
            .described("measure", "Measure minimum voltage"),
        DriverCommand::new("measure_vavg", ":MEAS:VAVG?", true)
            .described("measure", "Measure average voltage"),
        DriverCommand::new("measure_vrms", ":MEAS:VRMS?", true)
            .described("measure", "Measure RMS voltage"),
        DriverCommand::new("measure_vtop", ":MEAS:VTOP?", true)
            .described("measure", "Measure top voltage"),
        DriverCommand::new("measure_vbase", ":MEAS:VBAS?", true)
            .described("measure", "Measure base voltage"),
        DriverCommand::new("measure_frequency", ":MEAS:FREQ?", true)
            .described("measure", "Measure frequency"),
        DriverCommand::new("measure_period", ":MEAS:PER?", true)
            .described("measure", "Measure period"),
        DriverCommand::new("measure_rise_time", ":MEAS:RTIM?", true)
            .described("measure", "Measure rise time"),
        DriverCommand::new("measure_fall_time", ":MEAS:FTIM?", true)
            .described("measure", "Measure fall time"),
        DriverCommand::new("measure_positive_width", ":MEAS:PWID?", true)
            .described("measure", "Measure positive pulse width"),
        DriverCommand::new("measure_negative_width", ":MEAS:NWID?", true)
            .described("measure", "Measure negative pulse width"),
        DriverCommand::new("measure_positive_duty", ":MEAS:PDUT?", true)
            .described("measure", "Measure positive duty cycle"),
        DriverCommand::new("measure_negative_duty", ":MEAS:NDUT?", true)
            .described("measure", "Measure negative duty cycle"),
        DriverCommand::new("measure_overshoot", ":MEAS:OVER?", true)
            .described("measure", "Measure overshoot"),
        DriverCommand::new("measure_preshoot", ":MEAS:PRES?", true)
            .described("measure", "Measure preshoot"),
        DriverCommand::new("measure_clear", ":MEAS:CLE", false)
            .described("measure", "Clear all measurements"),
    ]);

    // --- Waveform transfer ---------------------------------------------------
    commands.extend([
        DriverCommand::new("waveform_source", ":WAV:SOUR", false)
            .described("waveform", "Set the waveform source channel"),
        DriverCommand::new("waveform_mode", ":WAV:MODE", false)
            .described("waveform", "Set the waveform mode (NORM/MAX/RAW)"),
        DriverCommand::new("waveform_format", ":WAV:FORM", false)
            .described("waveform", "Set the waveform format (WORD/BYTE/ASC)"),
        DriverCommand::new("waveform_points", ":WAV:POIN", false)
            .described("waveform", "Set the number of waveform points"),
        DriverCommand::new("waveform_preamble", ":WAV:PRE?", true)
            .described("waveform", "Read the waveform preamble"),
        DriverCommand::new("waveform_data", ":WAV:DATA?", true)
            .described("waveform", "Read the waveform data block"),
    ]);

    // --- Display, math and system -------------------------------------------
    commands.extend([
        DriverCommand::new("display_data", ":DISP:DATA?", true)
            .described("display", "Capture the screen as an image block"),
        DriverCommand::new("display_type", ":DISP:TYPE", false)
            .described("display", "Set the display type (VECT/DOTS)"),
        DriverCommand::new("math_display", ":MATH:DISP", false)
            .described("math", "Show or hide the math waveform"),
        DriverCommand::new("math_operator", ":MATH:OPER", false)
            .described("math", "Set the math operator (ADD/SUBT/MULT/DIV/FFT/...)"),
        DriverCommand::new("system_error", ":SYST:ERR?", true)
            .described("system", "Read the next error from the queue"),
        DriverCommand::new("system_beeper", ":SYST:BEEP", false)
            .described("system", "Sound the beeper"),
    ]);

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_oscilloscope() {
        assert_eq!(
            classify_idn("RIGOL TECHNOLOGIES,DS1054Z,DS1ZA123,1.0"),
            InstrumentType::Oscilloscope
        );
        assert_eq!(
            classify_idn("KEYSIGHT TECHNOLOGIES,DSOX1204G,CN123,1.0"),
            InstrumentType::Oscilloscope
        );
    }

    #[test]
    fn test_classify_power_supply() {
        assert_eq!(
            classify_idn("RIGOL TECHNOLOGIES,N6705C,SN,1.0"),
            InstrumentType::PowerSupply
        );
        assert_eq!(
            classify_idn("KEYSIGHT TECHNOLOGIES,E36300A,SN,1.0"),
            InstrumentType::PowerSupply
        );
    }

    #[test]
    fn test_classify_generator() {
        assert_eq!(
            classify_idn("Agilent Technologies,33500B,SN,1.0"),
            InstrumentType::SignalGenerator
        );
    }

    #[test]
    fn test_classify_multimeter() {
        assert_eq!(
            classify_idn("KEYSIGHT TECHNOLOGIES,34401A,SN,1.0"),
            InstrumentType::Multimeter
        );
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(classify_idn("ACME,WIDGET,SN,1.0"), InstrumentType::Unknown);
        assert_eq!(classify_idn(""), InstrumentType::Unknown);
    }

    #[test]
    fn test_driver_from_idn_generic() {
        // A Keysight scope has no rich profile, so it gets the generic set.
        let driver = InstrumentDriver::from_idn("KEYSIGHT TECHNOLOGIES,DSOX1204G,CN,1.0");
        assert_eq!(driver.instrument_type, InstrumentType::Oscilloscope);
        assert!(driver.profile.is_none());
        assert!(driver.command("measure_frequency").is_some());
        assert!(driver.command("identify").is_some());
        assert!(driver.command("nope").is_none());
    }

    #[test]
    fn test_driver_profile_registry_is_well_formed() {
        assert!(!DRIVER_PROFILES.is_empty());
        for profile in DRIVER_PROFILES {
            assert!(!profile.name.is_empty());
            assert!(!profile.idn_prefixes.is_empty());
            assert!(!(profile.build)().is_empty());
        }
        assert!(driver_profile_for_idn("RIGOL TECHNOLOGIES,DS1054Z,SN,1.0").is_some());
        assert!(driver_profile_for_idn("KEYSIGHT TECHNOLOGIES,DSOX,SN,1.0").is_none());
    }

    #[test]
    fn test_ds1000z_rich_profile_selected() {
        let driver = InstrumentDriver::from_idn("RIGOL TECHNOLOGIES,DS1054Z,DS1ZA123,1.0");
        assert_eq!(driver.profile.as_deref(), Some("Rigol DS1000Z"));
        assert_eq!(driver.screenshot_profile.as_deref(), Some("Rigol DS1000Z"));
    }

    #[test]
    fn test_ds1000z_full_operation_surface() {
        let driver = InstrumentDriver::from_idn("RIGOL TECHNOLOGIES,DS1054Z,DS1ZA123,1.0");
        // Acquisition control.
        for name in ["run", "stop", "single", "autoscale", "force_trigger"] {
            assert!(driver.command(name).is_some(), "missing {name}");
        }
        // Trigger configuration.
        for name in [
            "trigger_mode",
            "trigger_status",
            "trigger_sweep",
            "trigger_edge_source",
            "trigger_edge_level",
            "trigger_edge_slope",
        ] {
            assert!(driver.command(name).is_some(), "missing {name}");
        }
        // Per-channel setup for all four channels.
        for channel in 1..=4 {
            for suffix in ["display", "scale", "offset", "coupling", "probe"] {
                let name = format!("channel{channel}_{suffix}");
                assert!(driver.command(&name).is_some(), "missing {name}");
            }
        }
        // Timebase, measurements and waveform.
        assert!(driver.command("timebase_scale").is_some());
        assert!(driver.command("measure_vpp").is_some());
        assert!(driver.command("measure_frequency").is_some());
        assert!(driver.command("waveform_data").is_some());
        assert!(driver.command("display_data").is_some());
    }

    #[test]
    fn test_ds1000z_scpi_mnemonics() {
        let driver = InstrumentDriver::from_idn("RIGOL TECHNOLOGIES,DS1054Z,SN,1.0");
        assert_eq!(driver.command("run").unwrap().scpi, ":RUN");
        assert_eq!(driver.command("stop").unwrap().scpi, ":STOP");
        assert_eq!(driver.command("single").unwrap().scpi, ":SING");
        assert_eq!(driver.command("autoscale").unwrap().scpi, ":AUT");
        assert_eq!(
            driver.command("channel1_scale").unwrap().scpi,
            ":CHAN1:SCAL"
        );
        assert_eq!(
            driver.command("channel4_offset").unwrap().scpi,
            ":CHAN4:OFFS"
        );
        assert_eq!(
            driver.command("trigger_edge_level").unwrap().scpi,
            ":TRIG:EDGE:LEV"
        );
        assert_eq!(driver.command("measure_vpp").unwrap().scpi, ":MEAS:VPP?");
        assert!(driver.command("measure_vpp").unwrap().query);
        assert!(!driver.command("run").unwrap().query);
        // The DS1000Z mnemonics verified against hardware: average is VAVG (not
        // VAV), and rise/fall time are RTIM/FTIM (not RIS/FALL). There is no
        // "read every measurement" query, so `measure_all` is not exposed.
        assert_eq!(driver.command("measure_vavg").unwrap().scpi, ":MEAS:VAVG?");
        assert_eq!(
            driver.command("measure_rise_time").unwrap().scpi,
            ":MEAS:RTIM?"
        );
        assert_eq!(
            driver.command("measure_fall_time").unwrap().scpi,
            ":MEAS:FTIM?"
        );
        assert!(driver.command("measure_all").is_none());
    }

    #[test]
    fn test_driver_groups() {
        let driver = InstrumentDriver::from_idn("RIGOL TECHNOLOGIES,DS1054Z,SN,1.0");
        let groups = driver.groups();
        for expected in [
            "system", "acquire", "trigger", "channel1", "timebase", "measure", "waveform",
        ] {
            assert!(
                groups.contains(&expected.to_string()),
                "missing group {expected}"
            );
        }
        assert!(!driver.commands_in_group("trigger").is_empty());
    }

    #[test]
    fn test_generic_commands_are_type_specific() {
        let scope = InstrumentDriver::from_idn("KEYSIGHT TECHNOLOGIES,DSOX1204G,SN,1.0");
        assert!(scope.command("initiate").is_some());
        assert!(scope.command("set_voltage").is_none());

        let psu = InstrumentDriver::from_idn("RIGOL TECHNOLOGIES,N6705C,SN,1.0");
        assert!(psu.command("set_voltage").is_some());
        assert!(psu.command("initiate").is_none());
    }

    #[test]
    fn test_instrument_type_labels() {
        assert_eq!(InstrumentType::Oscilloscope.label(), "oscilloscope");
        assert_eq!(InstrumentType::Oscilloscope.glyph(), "scope");
        assert_eq!(InstrumentType::Unknown.label(), "instrument");
    }
}
