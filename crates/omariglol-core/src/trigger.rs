//! Triggers: directed dependency edges between instruments
//! (concepts.md §4.1, §2.5).
//!
//! A trigger declares that one instrument's action depends on another's state.
//! Triggers form a directed graph; the platform validates that the graph stays
//! acyclic (concepts.md §2.5, "drag-to-chain") so a trigger chain can never
//! deadlock itself.
//!
//! Validation is pure and lives here; arming, firing and event emission are the
//! caller's responsibility.

use serde::{Deserialize, Serialize};

use crate::instrument::InstrumentId;

/// The armed/disarmed state of a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerState {
    /// Not armed; will not fire.
    Disarmed,
    /// Armed; will fire when its condition holds.
    Armed,
    /// Fired at least once since being armed.
    Fired,
}

/// A directed dependency edge between two instruments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    /// Stable id within the workspace.
    pub id: String,
    /// The instrument whose state is watched.
    pub source: InstrumentId,
    /// The instrument the action is sent to.
    pub target: InstrumentId,
    /// The SCPI query whose result is the trigger condition.
    pub condition: String,
    /// The command sent to the target when the condition holds.
    pub action: String,
    /// Current armed state.
    pub state: TriggerState,
    /// Millisecond timestamp of the last firing, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fired_ms: Option<u64>,
}

impl Trigger {
    /// A new, disarmed trigger.
    pub fn new(
        id: impl Into<String>,
        source: InstrumentId,
        target: InstrumentId,
        condition: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Trigger {
            id: id.into(),
            source,
            target,
            condition: condition.into(),
            action: action.into(),
            state: TriggerState::Disarmed,
            last_fired_ms: None,
        }
    }

    /// Arm the trigger.
    pub fn arm(&mut self) {
        self.state = TriggerState::Armed;
    }

    /// Disarm the trigger.
    pub fn disarm(&mut self) {
        self.state = TriggerState::Disarmed;
    }

    /// Record that the trigger fired.
    pub fn fire(&mut self, now_ms: u64) {
        self.state = TriggerState::Fired;
        self.last_fired_ms = Some(now_ms);
    }

    /// True when the trigger is armed and ready to fire.
    pub fn is_armed(&self) -> bool {
        matches!(self.state, TriggerState::Armed)
    }
}

/// Why a proposed trigger edge was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TriggerError {
    #[error("a trigger cannot depend on itself")]
    SelfLoop,
    #[error("that trigger would create a cycle in the dependency graph")]
    Cycle,
}

/// Validate that adding `candidate` to `existing` keeps the graph acyclic.
///
/// The graph is a set of `source -> target` edges. Adding an edge is safe when
/// the target cannot already reach the source. A self-loop is always rejected.
pub fn validate_edge(
    existing: &[Trigger],
    candidate: &Trigger,
) -> Result<(), TriggerError> {
    if candidate.source == candidate.target {
        return Err(TriggerError::SelfLoop);
    }
    // Adding source -> target creates a cycle iff target already reaches source.
    if reaches(existing, &candidate.target, &candidate.source) {
        return Err(TriggerError::Cycle);
    }
    Ok(())
}

/// True when `from` can reach `to` by following existing trigger edges.
fn reaches(existing: &[Trigger], from: &InstrumentId, to: &InstrumentId) -> bool {
    let mut stack = vec![from.clone()];
    let mut seen = Vec::new();
    while let Some(node) = stack.pop() {
        if &node == to {
            return true;
        }
        if seen.contains(&node) {
            continue;
        }
        seen.push(node.clone());
        for trigger in existing {
            if trigger.source == node {
                stack.push(trigger.target.clone());
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> InstrumentId {
        InstrumentId::new(s)
    }

    fn edge(id_: &str, from: &str, to: &str) -> Trigger {
        Trigger::new(id_, id(from), id(to), "*OPC?", ":INIT")
    }

    #[test]
    fn test_trigger_lifecycle() {
        let mut trigger = edge("t1", "gen", "scope");
        assert!(!trigger.is_armed());
        trigger.arm();
        assert!(trigger.is_armed());
        trigger.fire(1000);
        assert_eq!(trigger.state, TriggerState::Fired);
        assert_eq!(trigger.last_fired_ms, Some(1000));
        trigger.disarm();
        assert_eq!(trigger.state, TriggerState::Disarmed);
    }

    #[test]
    fn test_validate_rejects_self_loop() {
        let candidate = edge("t1", "a", "a");
        assert_eq!(
            validate_edge(&[], &candidate),
            Err(TriggerError::SelfLoop)
        );
    }

    #[test]
    fn test_validate_accepts_simple_edge() {
        let candidate = edge("t1", "gen", "scope");
        assert!(validate_edge(&[], &candidate).is_ok());
    }

    #[test]
    fn test_validate_accepts_chain() {
        let existing = vec![edge("t1", "gen", "scope")];
        let candidate = edge("t2", "scope", "dmm");
        assert!(validate_edge(&existing, &candidate).is_ok());
    }

    #[test]
    fn test_validate_rejects_direct_cycle() {
        let existing = vec![edge("t1", "gen", "scope")];
        let candidate = edge("t2", "scope", "gen");
        assert_eq!(validate_edge(&existing, &candidate), Err(TriggerError::Cycle));
    }

    #[test]
    fn test_validate_rejects_indirect_cycle() {
        let existing = vec![edge("t1", "a", "b"), edge("t2", "b", "c")];
        let candidate = edge("t3", "c", "a");
        assert_eq!(validate_edge(&existing, &candidate), Err(TriggerError::Cycle));
    }

    #[test]
    fn test_validate_allows_diamond() {
        // a -> b, a -> c, b -> d, c -> d is acyclic.
        let existing = vec![
            edge("t1", "a", "b"),
            edge("t2", "a", "c"),
            edge("t3", "b", "d"),
        ];
        let candidate = edge("t4", "c", "d");
        assert!(validate_edge(&existing, &candidate).is_ok());
    }
}
