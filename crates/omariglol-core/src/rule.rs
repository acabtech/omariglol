//! Conditional routing rules (concepts.md §4.1, §2.5).
//!
//! A rule says: "query instrument A; if the reading satisfies a condition, send
//! command Y to instrument B; otherwise run a fallback." Rules are evaluated
//! either continuously (on every state change) or on demand.
//!
//! Evaluation is pure: [`Rule::evaluate`] takes a reading and returns the
//! decision, so the routing logic is testable without any I/O. The caller is
//! responsible for performing the query and sending the resulting command.

use serde::{Deserialize, Serialize};

use crate::instrument::InstrumentId;

/// How a rule's condition is compared against a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Comparison {
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Equal,
    NotEqual,
}

impl Comparison {
    /// Apply the comparison to a reading and a threshold.
    pub fn test(&self, reading: f64, threshold: f64) -> bool {
        match self {
            Comparison::GreaterThan => reading > threshold,
            Comparison::GreaterOrEqual => reading >= threshold,
            Comparison::LessThan => reading < threshold,
            Comparison::LessOrEqual => reading <= threshold,
            Comparison::Equal => (reading - threshold).abs() < f64::EPSILON,
            Comparison::NotEqual => (reading - threshold).abs() >= f64::EPSILON,
        }
    }

    /// The operator symbol, for display.
    pub fn symbol(&self) -> &'static str {
        match self {
            Comparison::GreaterThan => ">",
            Comparison::GreaterOrEqual => ">=",
            Comparison::LessThan => "<",
            Comparison::LessOrEqual => "<=",
            Comparison::Equal => "==",
            Comparison::NotEqual => "!=",
        }
    }
}

/// When a rule is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluationMode {
    /// Re-evaluated on every state change.
    Continuous,
    /// Evaluated only when explicitly requested.
    OnDemand,
}

/// A conditional routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Stable id within the workspace.
    pub id: String,
    /// Display name.
    pub name: String,
    /// The instrument whose reading drives the decision.
    pub source: InstrumentId,
    /// The SCPI query that produces the reading.
    pub query: String,
    /// How the reading is compared.
    pub comparison: Comparison,
    /// The threshold the reading is compared against.
    pub threshold: f64,
    /// The instrument the action is sent to.
    pub target: InstrumentId,
    /// The command sent when the condition holds.
    pub action: String,
    /// The command sent when the condition does not hold, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    /// When the rule is evaluated.
    pub mode: EvaluationMode,
}

impl Rule {
    /// A new rule with a `>` comparison and continuous evaluation.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        source: InstrumentId,
        target: InstrumentId,
    ) -> Self {
        Rule {
            id: id.into(),
            name: name.into(),
            source,
            query: String::new(),
            comparison: Comparison::GreaterThan,
            threshold: 0.0,
            target,
            action: String::new(),
            fallback: None,
            mode: EvaluationMode::Continuous,
        }
    }

    /// Evaluate the rule against a reading.
    ///
    /// Returns the command to send to the target, or `None` when the condition
    /// is false and there is no fallback.
    pub fn evaluate(&self, reading: f64) -> RuleDecision {
        if self.comparison.test(reading, self.threshold) {
            RuleDecision {
                rule_id: self.id.clone(),
                fired: true,
                target: self.target.clone(),
                command: Some(self.action.clone()),
                reading,
            }
        } else {
            RuleDecision {
                rule_id: self.id.clone(),
                fired: false,
                target: self.target.clone(),
                command: self.fallback.clone(),
                reading,
            }
        }
    }

    /// A one-line description of the condition, for the UI.
    pub fn condition_text(&self) -> String {
        format!(
            "{} {} {}",
            self.query,
            self.comparison.symbol(),
            self.threshold
        )
    }
}

/// The outcome of evaluating a rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleDecision {
    pub rule_id: String,
    /// True when the condition held.
    pub fired: bool,
    /// The instrument the command targets.
    pub target: InstrumentId,
    /// The command to send, or `None` when nothing should happen.
    pub command: Option<String>,
    /// The reading that produced the decision.
    pub reading: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> InstrumentId {
        InstrumentId::new(s)
    }

    fn rule() -> Rule {
        let mut rule = Rule::new("r1", "Route", id("dmm"), id("relay"));
        rule.query = ":MEAS:VOLT?".to_string();
        rule.comparison = Comparison::GreaterThan;
        rule.threshold = 5.0;
        rule.action = ":ROUT:CLOS A".to_string();
        rule.fallback = Some(":ROUT:CLOS B".to_string());
        rule
    }

    #[test]
    fn test_comparison_operators() {
        assert!(Comparison::GreaterThan.test(6.0, 5.0));
        assert!(!Comparison::GreaterThan.test(5.0, 5.0));
        assert!(Comparison::GreaterOrEqual.test(5.0, 5.0));
        assert!(Comparison::LessThan.test(4.0, 5.0));
        assert!(Comparison::LessOrEqual.test(5.0, 5.0));
        assert!(Comparison::Equal.test(5.0, 5.0));
        assert!(Comparison::NotEqual.test(4.0, 5.0));
    }

    #[test]
    fn test_evaluate_fires_action() {
        let decision = rule().evaluate(6.0);
        assert!(decision.fired);
        assert_eq!(decision.command.as_deref(), Some(":ROUT:CLOS A"));
        assert_eq!(decision.target, id("relay"));
        assert_eq!(decision.reading, 6.0);
    }

    #[test]
    fn test_evaluate_uses_fallback() {
        let decision = rule().evaluate(3.0);
        assert!(!decision.fired);
        assert_eq!(decision.command.as_deref(), Some(":ROUT:CLOS B"));
    }

    #[test]
    fn test_evaluate_without_fallback() {
        let mut rule = rule();
        rule.fallback = None;
        let decision = rule.evaluate(3.0);
        assert!(!decision.fired);
        assert!(decision.command.is_none());
    }

    #[test]
    fn test_condition_text() {
        assert_eq!(rule().condition_text(), ":MEAS:VOLT? > 5");
    }

    #[test]
    fn test_comparison_symbols() {
        assert_eq!(Comparison::GreaterOrEqual.symbol(), ">=");
        assert_eq!(Comparison::NotEqual.symbol(), "!=");
    }
}
