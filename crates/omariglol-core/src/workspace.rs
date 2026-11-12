//! Workspaces: the unit of work in the multi-instrument platform
//! (concepts.md §4.1, §2.4).
//!
//! A [`Workspace`] is a named collection of instruments, their presets, rules
//! and triggers, and the shared [`Timeline`] that records what happened. The
//! [`WorkspaceManager`] owns the set of workspaces and tracks which one is
//! active, persisting them to disk as JSON when a directory is configured.
//!
//! This module is pure data plus file I/O: it never opens a network connection.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::device::Device;
use crate::instrument::{Instrument, InstrumentId};
use crate::preset::Preset;
use crate::rule::Rule;
use crate::timeline::Timeline;
use crate::trigger::{self, Trigger, TriggerError};

/// Milliseconds since the UNIX epoch, saturating at 0 before it.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A monotonic counter so ids are unique even within the same millisecond.
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique, sortable id with the given prefix.
fn generate_id(prefix: &str) -> String {
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{counter}", now_millis())
}

/// A stable identifier for a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorkspaceId(pub String);

impl WorkspaceId {
    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Attribution and versioning metadata for a workspace (concepts.md §3.4).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    /// The workspace owner, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Collaborators who have contributed to the workspace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collaborators: Vec<String>,
    /// A monotonically increasing version number.
    #[serde(default)]
    pub version: u32,
}

/// A named collection of instruments and automation artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Stable id.
    pub id: WorkspaceId,
    /// Display name.
    pub name: String,
    /// Creation time, in milliseconds since the epoch.
    pub created_ms: u64,
    /// Last-modified time, in milliseconds since the epoch.
    pub modified_ms: u64,
    /// The instruments in the workspace.
    #[serde(default)]
    pub instruments: Vec<Instrument>,
    /// Presets defined for this workspace.
    #[serde(default)]
    pub presets: Vec<Preset>,
    /// Conditional routing rules.
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Trigger dependency edges.
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    /// The shared automation timeline.
    #[serde(default)]
    pub timeline: Timeline,
    /// Attribution and versioning metadata.
    #[serde(default)]
    pub metadata: WorkspaceMetadata,
}

impl Workspace {
    /// Create an empty workspace with a generated id.
    pub fn new(name: impl Into<String>) -> Self {
        let now = now_millis();
        Workspace {
            id: WorkspaceId(generate_id("ws")),
            name: name.into(),
            created_ms: now,
            modified_ms: now,
            instruments: Vec::new(),
            presets: Vec::new(),
            rules: Vec::new(),
            triggers: Vec::new(),
            timeline: Timeline::new(),
            metadata: WorkspaceMetadata::default(),
        }
    }

    /// Update the modified timestamp.
    pub fn touch(&mut self) {
        self.modified_ms = now_millis();
    }

    /// Add a device as an instrument, returning its id.
    ///
    /// Adding a device that is already present (same id) is a no-op that
    /// returns the existing id, so re-adding from discovery is safe.
    pub fn add_instrument(&mut self, device: Device) -> InstrumentId {
        let instrument = Instrument::from_device(device);
        let id = instrument.id.clone();
        if self.instrument(&id).is_none() {
            self.instruments.push(instrument);
            self.touch();
        }
        id
    }

    /// Remove an instrument and every artifact that references it.
    pub fn remove_instrument(&mut self, id: &InstrumentId) -> bool {
        let before = self.instruments.len();
        self.instruments.retain(|instrument| &instrument.id != id);
        if self.instruments.len() == before {
            return false;
        }
        // Drop presets, rules and triggers that reference the removed instrument.
        self.presets
            .retain(|preset| !preset.commands.contains_key(id));
        self.rules
            .retain(|rule| &rule.source != id && &rule.target != id);
        self.triggers
            .retain(|trigger| &trigger.source != id && &trigger.target != id);
        self.touch();
        true
    }

    /// Look up an instrument by id.
    pub fn instrument(&self, id: &InstrumentId) -> Option<&Instrument> {
        self.instruments
            .iter()
            .find(|instrument| &instrument.id == id)
    }

    /// Look up an instrument mutably by id.
    pub fn instrument_mut(&mut self, id: &InstrumentId) -> Option<&mut Instrument> {
        self.instruments
            .iter_mut()
            .find(|instrument| &instrument.id == id)
    }

    /// Look up an instrument by its display label.
    pub fn instrument_by_label(&self, label: &str) -> Option<&Instrument> {
        self.instruments
            .iter()
            .find(|instrument| instrument.label() == label)
    }

    /// The ids of every instrument in the workspace.
    pub fn instrument_ids(&self) -> Vec<InstrumentId> {
        self.instruments
            .iter()
            .map(|instrument| instrument.id.clone())
            .collect()
    }

    /// The number of connected instruments.
    pub fn connected_count(&self) -> usize {
        self.instruments
            .iter()
            .filter(|instrument| instrument.state.is_connected())
            .count()
    }

    /// Add a preset.
    pub fn add_preset(&mut self, preset: Preset) {
        self.presets.retain(|existing| existing.id != preset.id);
        self.presets.push(preset);
        self.touch();
    }

    /// Add a rule.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.retain(|existing| existing.id != rule.id);
        self.rules.push(rule);
        self.touch();
    }

    /// Add a trigger, validating that it keeps the dependency graph acyclic.
    ///
    /// Triggers are replaced by id, so a trigger being updated is excluded from
    /// the graph during validation; otherwise its own old edge would be seen as
    /// a cycle and a valid replacement would be rejected.
    pub fn add_trigger(&mut self, trigger: Trigger) -> Result<(), TriggerError> {
        let existing: Vec<Trigger> = self
            .triggers
            .iter()
            .filter(|existing| existing.id != trigger.id)
            .cloned()
            .collect();
        trigger::validate_edge(&existing, &trigger)?;
        self.triggers.retain(|existing| existing.id != trigger.id);
        self.triggers.push(trigger);
        self.touch();
        Ok(())
    }

    /// Remove a trigger by id.
    pub fn remove_trigger(&mut self, id: &str) -> bool {
        let before = self.triggers.len();
        self.triggers.retain(|trigger| trigger.id != id);
        if self.triggers.len() != before {
            self.touch();
            true
        } else {
            false
        }
    }
}

/// Owns the set of workspaces and tracks the active one.
#[derive(Debug, Default)]
pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,
    active: Option<WorkspaceId>,
    dir: Option<PathBuf>,
}

impl WorkspaceManager {
    /// An in-memory manager with no persistence.
    pub fn new() -> Self {
        WorkspaceManager::default()
    }

    /// A manager that persists workspaces as JSON files in `dir`.
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        WorkspaceManager {
            workspaces: Vec::new(),
            active: None,
            dir: Some(dir.into()),
        }
    }

    /// The directory workspaces are persisted to, if any.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Create a workspace, make it active, and persist it.
    pub fn create(&mut self, name: impl Into<String>) -> WorkspaceId {
        let workspace = Workspace::new(name);
        let id = workspace.id.clone();
        self.workspaces.push(workspace);
        self.active = Some(id.clone());
        let _ = self.save(&id);
        id
    }

    /// All workspaces, in creation order.
    pub fn list(&self) -> &[Workspace] {
        &self.workspaces
    }

    /// The active workspace id, if any.
    pub fn active_id(&self) -> Option<&WorkspaceId> {
        self.active.as_ref()
    }

    /// The active workspace, if any.
    pub fn active(&self) -> Option<&Workspace> {
        self.active.as_ref().and_then(|id| self.get(id))
    }

    /// The active workspace, mutably, if any.
    pub fn active_mut(&mut self) -> Option<&mut Workspace> {
        let id = self.active.clone()?;
        self.get_mut(&id)
    }

    /// Look up a workspace by id.
    pub fn get(&self, id: &WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| &workspace.id == id)
    }

    /// Look up a workspace mutably by id.
    pub fn get_mut(&mut self, id: &WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces
            .iter_mut()
            .find(|workspace| &workspace.id == id)
    }

    /// Make a workspace active, returning false when it does not exist.
    pub fn open(&mut self, id: &WorkspaceId) -> bool {
        if self.get(id).is_some() {
            self.active = Some(id.clone());
            true
        } else {
            false
        }
    }

    /// Delete a workspace and clear it as active if it was.
    pub fn delete(&mut self, id: &WorkspaceId) -> bool {
        let before = self.workspaces.len();
        self.workspaces.retain(|workspace| &workspace.id != id);
        if self.workspaces.len() == before {
            return false;
        }
        if self.active.as_ref() == Some(id) {
            self.active = None;
        }
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(dir.join(format!("{}.json", id.as_str())));
        }
        true
    }

    /// Persist a workspace to disk, if a directory is configured.
    pub fn save(&self, id: &WorkspaceId) -> std::io::Result<()> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        let Some(workspace) = self.get(id) else {
            return Ok(());
        };
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.json", id.as_str()));
        let json = serde_json::to_string_pretty(workspace)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Persist the active workspace, if any.
    pub fn save_active(&self) -> std::io::Result<()> {
        match &self.active {
            Some(id) => self.save(id),
            None => Ok(()),
        }
    }

    /// Load every workspace from the configured directory.
    ///
    /// Unreadable or malformed files are skipped rather than failing the whole
    /// load, so one corrupt workspace cannot make the app unusable.
    pub fn load_all(&mut self) -> std::io::Result<usize> {
        let Some(dir) = &self.dir else {
            return Ok(0);
        };
        if !dir.exists() {
            return Ok(0);
        }
        let mut loaded = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(workspace) = serde_json::from_str::<Workspace>(&text) else {
                continue;
            };
            if self.get(&workspace.id).is_none() {
                self.workspaces.push(workspace);
                loaded += 1;
            }
        }
        if self.active.is_none() {
            self.active = self.workspaces.first().map(|w| w.id.clone());
        }
        Ok(loaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn device(addr: &str) -> Device {
        Device::manual(addr.parse::<IpAddr>().unwrap())
    }

    #[test]
    fn test_workspace_new() {
        let workspace = Workspace::new("Test");
        assert_eq!(workspace.name, "Test");
        assert!(workspace.instruments.is_empty());
        assert!(workspace.id.as_str().starts_with("ws-"));
        assert_eq!(workspace.created_ms, workspace.modified_ms);
    }

    #[test]
    fn test_add_and_remove_instrument() {
        let mut workspace = Workspace::new("Test");
        let id = workspace.add_instrument(device("192.168.1.50"));
        assert_eq!(workspace.instruments.len(), 1);
        assert!(workspace.instrument(&id).is_some());

        // Re-adding the same device is a no-op.
        let again = workspace.add_instrument(device("192.168.1.50"));
        assert_eq!(again, id);
        assert_eq!(workspace.instruments.len(), 1);

        assert!(workspace.remove_instrument(&id));
        assert!(workspace.instruments.is_empty());
        assert!(!workspace.remove_instrument(&id));
    }

    #[test]
    fn test_remove_instrument_cleans_artifacts() {
        let mut workspace = Workspace::new("Test");
        let a = workspace.add_instrument(device("192.168.1.50"));
        let b = workspace.add_instrument(device("192.168.1.51"));

        let mut preset = Preset::new("p1", "P");
        preset.add_command(a.clone(), ":VOLT 1");
        workspace.add_preset(preset);

        let mut rule = Rule::new("r1", "R", a.clone(), b.clone());
        rule.action = ":INIT".to_string();
        workspace.add_rule(rule);

        let trigger = Trigger::new("t1", a.clone(), b.clone(), "*OPC?", ":INIT");
        workspace.add_trigger(trigger).unwrap();

        workspace.remove_instrument(&a);
        assert!(workspace.presets.is_empty());
        assert!(workspace.rules.is_empty());
        assert!(workspace.triggers.is_empty());
    }

    #[test]
    fn test_add_trigger_replaces_by_id_without_false_cycle() {
        let mut workspace = Workspace::new("Test");
        let a = workspace.add_instrument(device("192.168.1.50"));
        let b = workspace.add_instrument(device("192.168.1.51"));

        workspace
            .add_trigger(Trigger::new("t1", a.clone(), b.clone(), "*OPC?", ":INIT"))
            .unwrap();
        // Replacing t1 with the reverse edge leaves a single acyclic edge, so
        // the old a -> b edge must not count against it.
        workspace
            .add_trigger(Trigger::new("t1", b.clone(), a.clone(), "*OPC?", ":INIT"))
            .unwrap();
        assert_eq!(workspace.triggers.len(), 1);
        let trigger = &workspace.triggers[0];
        assert_eq!(trigger.id, "t1");
        assert_eq!(trigger.source, b);
        assert_eq!(trigger.target, a);
    }

    #[test]
    fn test_add_trigger_validates_cycles() {
        let mut workspace = Workspace::new("Test");
        let a = workspace.add_instrument(device("192.168.1.50"));
        let b = workspace.add_instrument(device("192.168.1.51"));

        workspace
            .add_trigger(Trigger::new("t1", a.clone(), b.clone(), "*OPC?", ":INIT"))
            .unwrap();
        // The reverse edge would create a cycle.
        let result =
            workspace.add_trigger(Trigger::new("t2", b.clone(), a.clone(), "*OPC?", ":INIT"));
        assert_eq!(result, Err(TriggerError::Cycle));
        assert_eq!(workspace.triggers.len(), 1);
    }

    #[test]
    fn test_connected_count() {
        let mut workspace = Workspace::new("Test");
        let id = workspace.add_instrument(device("192.168.1.50"));
        assert_eq!(workspace.connected_count(), 0);
        workspace
            .instrument_mut(&id)
            .unwrap()
            .mark_connected("IDN".to_string(), 1);
        assert_eq!(workspace.connected_count(), 1);
    }

    #[test]
    fn test_manager_create_and_open() {
        let mut manager = WorkspaceManager::new();
        let a = manager.create("A");
        let b = manager.create("B");
        assert_eq!(manager.list().len(), 2);
        // The most recently created is active.
        assert_eq!(manager.active_id(), Some(&b));
        assert!(manager.open(&a));
        assert_eq!(manager.active_id(), Some(&a));
        assert!(!manager.open(&WorkspaceId("nope".to_string())));
    }

    #[test]
    fn test_manager_delete() {
        let mut manager = WorkspaceManager::new();
        let a = manager.create("A");
        assert!(manager.delete(&a));
        assert!(manager.active_id().is_none());
        assert!(!manager.delete(&a));
    }

    #[test]
    fn test_manager_persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("omariglol-test-{}", now_millis()));
        let mut manager = WorkspaceManager::with_dir(&dir);
        let id = manager.create("Persisted");
        manager
            .get_mut(&id)
            .unwrap()
            .add_instrument(device("192.168.1.50"));
        manager.save(&id).unwrap();

        let mut reloaded = WorkspaceManager::with_dir(&dir);
        let count = reloaded.load_all().unwrap();
        assert_eq!(count, 1);
        let workspace = reloaded.get(&id).unwrap();
        assert_eq!(workspace.name, "Persisted");
        assert_eq!(workspace.instruments.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_manager_load_skips_corrupt() {
        let dir = std::env::temp_dir().join(format!("omariglol-corrupt-{}", now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.json"), "{ not json").unwrap();
        let mut manager = WorkspaceManager::with_dir(&dir);
        assert_eq!(manager.load_all().unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
