//! Presets: named batches of SCPI commands, one per instrument
//! (concepts.md §4.1, §2.5).
//!
//! A preset is data. Applying it produces a *plan* — a map from instrument to
//! the commands to send — which the caller (the Tauri layer or a script) then
//! fans out. Keeping the expansion pure makes it testable without a network and
//! lets the UI preview exactly what a preset will do before it runs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::instrument::InstrumentId;

/// A named collection of SCPI commands, one list per instrument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    /// Stable id within the workspace.
    pub id: String,
    /// Display name, e.g. `"Rack Power-Up"`.
    pub name: String,
    /// Commands to send, keyed by instrument.
    pub commands: BTreeMap<InstrumentId, Vec<String>>,
    /// Values interpolated into commands as `{name}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
}

impl Preset {
    /// A new, empty preset.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Preset {
            id: id.into(),
            name: name.into(),
            commands: BTreeMap::new(),
            parameters: BTreeMap::new(),
        }
    }

    /// Add a command for an instrument.
    pub fn add_command(&mut self, instrument: InstrumentId, command: impl Into<String>) {
        self.commands
            .entry(instrument)
            .or_default()
            .push(command.into());
    }

    /// Set a parameter value.
    pub fn set_parameter(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.parameters.insert(name.into(), value.into());
    }

    /// The instruments this preset targets.
    pub fn targets(&self) -> Vec<&InstrumentId> {
        self.commands.keys().collect()
    }

    /// Expand the preset into a concrete plan, interpolating parameters.
    ///
    /// Only instruments present in `available` are included, so a preset that
    /// references a removed instrument degrades gracefully rather than failing.
    pub fn plan(&self, available: &[InstrumentId]) -> PresetPlan {
        let mut steps = Vec::new();
        for (instrument, commands) in &self.commands {
            if !available.contains(instrument) {
                continue;
            }
            for command in commands {
                steps.push(PresetStep {
                    instrument: instrument.clone(),
                    command: interpolate(command, &self.parameters),
                });
            }
        }
        PresetPlan {
            preset_id: self.id.clone(),
            preset_name: self.name.clone(),
            steps,
        }
    }
}

/// One command in an expanded preset plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetStep {
    pub instrument: InstrumentId,
    pub command: String,
}

/// A preset expanded into concrete, interpolated commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetPlan {
    pub preset_id: String,
    pub preset_name: String,
    pub steps: Vec<PresetStep>,
}

impl PresetPlan {
    /// The number of commands the plan will send.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// True when the plan has no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Replace `{name}` placeholders with parameter values.
///
/// Unknown placeholders are left untouched, so a typo is visible rather than
/// silently producing an empty command.
fn interpolate(command: &str, parameters: &BTreeMap<String, String>) -> String {
    let mut result = command.to_string();
    for (name, value) in parameters {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> InstrumentId {
        InstrumentId::new(s)
    }

    #[test]
    fn test_preset_build_and_targets() {
        let mut preset = Preset::new("p1", "Rack Power-Up");
        preset.add_command(id("psu1"), ":VOLT 12.0");
        preset.add_command(id("psu1"), ":OUTP ON");
        preset.add_command(id("psu2"), ":VOLT 12.0");
        assert_eq!(preset.targets().len(), 2);
        assert_eq!(preset.commands[&id("psu1")].len(), 2);
    }

    #[test]
    fn test_plan_expands_in_order() {
        let mut preset = Preset::new("p1", "Rack");
        preset.add_command(id("psu1"), ":VOLT 12.0");
        preset.add_command(id("psu1"), ":OUTP ON");
        let plan = preset.plan(&[id("psu1")]);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan.steps[0].command, ":VOLT 12.0");
        assert_eq!(plan.steps[1].command, ":OUTP ON");
    }

    #[test]
    fn test_plan_skips_missing_instruments() {
        let mut preset = Preset::new("p1", "Rack");
        preset.add_command(id("psu1"), ":VOLT 12.0");
        preset.add_command(id("gone"), ":VOLT 5.0");
        let plan = preset.plan(&[id("psu1")]);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan.steps[0].instrument, id("psu1"));
    }

    #[test]
    fn test_plan_interpolates_parameters() {
        let mut preset = Preset::new("p1", "Rack");
        preset.set_parameter("voltage", "12.0");
        preset.add_command(id("psu1"), ":VOLT {voltage}");
        let plan = preset.plan(&[id("psu1")]);
        assert_eq!(plan.steps[0].command, ":VOLT 12.0");
    }

    #[test]
    fn test_plan_leaves_unknown_placeholder() {
        let mut preset = Preset::new("p1", "Rack");
        preset.add_command(id("psu1"), ":VOLT {missing}");
        let plan = preset.plan(&[id("psu1")]);
        assert_eq!(plan.steps[0].command, ":VOLT {missing}");
    }

    #[test]
    fn test_empty_plan() {
        let preset = Preset::new("p1", "Empty");
        let plan = preset.plan(&[id("psu1")]);
        assert!(plan.is_empty());
    }
}
