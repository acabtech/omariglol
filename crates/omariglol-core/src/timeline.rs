//! The shared automation timeline (concepts.md §4.1, §2.5).
//!
//! Every command sent, query made, response received, error raised, preset
//! applied and rule evaluated across a workspace is recorded as a timestamped
//! [`TimelineEvent`]. The timeline is the audit trail the UI scrubs through and
//! the scripting layer records into.
//!
//! It is a bounded ring: a long-running workspace must not grow without limit,
//! so the oldest events are dropped once `capacity` is reached.

use serde::{Deserialize, Serialize};

use crate::instrument::InstrumentId;

/// The default number of events a timeline retains.
pub const DEFAULT_CAPACITY: usize = 10_000;

/// What kind of thing happened on the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimelineEventKind {
    /// A command was written to an instrument.
    Command,
    /// A query was sent and a response received.
    Query,
    /// An error occurred.
    Error,
    /// A preset was applied.
    Preset,
    /// A rule was evaluated.
    Rule,
    /// A trigger fired.
    Trigger,
    /// A script step or marker.
    Script,
    /// A user annotation.
    Marker,
}

impl TimelineEventKind {
    /// A short label for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            TimelineEventKind::Command => "command",
            TimelineEventKind::Query => "query",
            TimelineEventKind::Error => "error",
            TimelineEventKind::Preset => "preset",
            TimelineEventKind::Rule => "rule",
            TimelineEventKind::Trigger => "trigger",
            TimelineEventKind::Script => "script",
            TimelineEventKind::Marker => "marker",
        }
    }
}

/// One timestamped event on the shared timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Milliseconds since the UNIX epoch.
    pub timestamp_ms: u64,
    /// The instrument the event concerns, when it concerns one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_id: Option<InstrumentId>,
    /// What kind of event this is.
    pub kind: TimelineEventKind,
    /// A human-readable description.
    pub message: String,
    /// Arbitrary structured payload (a query result, a recorded tuple, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl TimelineEvent {
    /// Build an event with no structured payload.
    pub fn new(
        timestamp_ms: u64,
        instrument_id: Option<InstrumentId>,
        kind: TimelineEventKind,
        message: impl Into<String>,
    ) -> Self {
        TimelineEvent {
            timestamp_ms,
            instrument_id,
            kind,
            message: message.into(),
            data: None,
        }
    }

    /// Attach a structured payload.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// A bounded, time-ordered log of workspace events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    events: Vec<TimelineEvent>,
    capacity: usize,
}

impl Timeline {
    /// A timeline with the default capacity.
    pub fn new() -> Self {
        Timeline::with_capacity(DEFAULT_CAPACITY)
    }

    /// A timeline that retains at most `capacity` events.
    pub fn with_capacity(capacity: usize) -> Self {
        Timeline {
            events: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// Append an event, dropping the oldest when at capacity.
    pub fn record(&mut self, event: TimelineEvent) {
        if self.events.len() >= self.capacity {
            // Drop the oldest quarter at once rather than shifting on every
            // push, which keeps recording O(1) amortised.
            let drop = (self.capacity / 4).max(1);
            self.events.drain(0..drop.min(self.events.len()));
        }
        self.events.push(event);
    }

    /// All events, oldest first.
    pub fn events(&self) -> &[TimelineEvent] {
        &self.events
    }

    /// The number of retained events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// True when nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Events within `[start_ms, end_ms]`, oldest first.
    pub fn range(&self, start_ms: u64, end_ms: u64) -> Vec<&TimelineEvent> {
        self.events
            .iter()
            .filter(|event| event.timestamp_ms >= start_ms && event.timestamp_ms <= end_ms)
            .collect()
    }

    /// Events for one instrument, oldest first.
    pub fn for_instrument(&self, id: &InstrumentId) -> Vec<&TimelineEvent> {
        self.events
            .iter()
            .filter(|event| event.instrument_id.as_ref() == Some(id))
            .collect()
    }

    /// The most recent event at or before `timestamp_ms`.
    ///
    /// This is what timeline scrubbing uses to reconstruct state at a point in
    /// time.
    pub fn at(&self, timestamp_ms: u64) -> Option<&TimelineEvent> {
        self.events
            .iter()
            .rev()
            .find(|event| event.timestamp_ms <= timestamp_ms)
    }

    /// Remove every event.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Timeline::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(ts: u64, kind: TimelineEventKind, message: &str) -> TimelineEvent {
        TimelineEvent::new(ts, None, kind, message)
    }

    #[test]
    fn test_record_and_len() {
        let mut timeline = Timeline::new();
        assert!(timeline.is_empty());
        timeline.record(event(1, TimelineEventKind::Command, "a"));
        timeline.record(event(2, TimelineEventKind::Query, "b"));
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.events()[0].message, "a");
    }

    #[test]
    fn test_capacity_drops_oldest() {
        let mut timeline = Timeline::with_capacity(4);
        for i in 0..10 {
            timeline.record(event(i, TimelineEventKind::Command, &format!("e{i}")));
        }
        // Never exceeds capacity.
        assert!(timeline.len() <= 4);
        // The newest event is always retained.
        assert_eq!(timeline.events().last().unwrap().message, "e9");
    }

    #[test]
    fn test_range_filters() {
        let mut timeline = Timeline::new();
        for i in 0..10 {
            timeline.record(event(i, TimelineEventKind::Command, &format!("e{i}")));
        }
        let range = timeline.range(3, 6);
        assert_eq!(range.len(), 4);
        assert_eq!(range[0].timestamp_ms, 3);
        assert_eq!(range[3].timestamp_ms, 6);
    }

    #[test]
    fn test_for_instrument() {
        let mut timeline = Timeline::new();
        let a = InstrumentId::new("a");
        let b = InstrumentId::new("b");
        timeline.record(TimelineEvent::new(
            1,
            Some(a.clone()),
            TimelineEventKind::Command,
            "to a",
        ));
        timeline.record(TimelineEvent::new(
            2,
            Some(b.clone()),
            TimelineEventKind::Command,
            "to b",
        ));
        timeline.record(TimelineEvent::new(
            3,
            Some(a.clone()),
            TimelineEventKind::Query,
            "from a",
        ));
        assert_eq!(timeline.for_instrument(&a).len(), 2);
        assert_eq!(timeline.for_instrument(&b).len(), 1);
    }

    #[test]
    fn test_at_returns_latest_before() {
        let mut timeline = Timeline::new();
        timeline.record(event(10, TimelineEventKind::Command, "a"));
        timeline.record(event(20, TimelineEventKind::Command, "b"));
        timeline.record(event(30, TimelineEventKind::Command, "c"));
        assert_eq!(timeline.at(25).unwrap().message, "b");
        assert_eq!(timeline.at(30).unwrap().message, "c");
        assert!(timeline.at(5).is_none());
    }

    #[test]
    fn test_event_with_data() {
        let event =
            event(1, TimelineEventKind::Query, "q").with_data(serde_json::json!({ "value": 1.5 }));
        assert_eq!(event.data.unwrap()["value"], 1.5);
    }

    #[test]
    fn test_clear() {
        let mut timeline = Timeline::new();
        timeline.record(event(1, TimelineEventKind::Command, "a"));
        timeline.clear();
        assert!(timeline.is_empty());
    }
}
