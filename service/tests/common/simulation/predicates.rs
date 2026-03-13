//! Behavioral invariant predicates for trust graph simulation.
//!
//! Each predicate expresses a property that should hold across graph topologies.
//! Predicates take a `GraphSpec` + `SimulationReport` and return a `PredicateResult`
//! indicating whether the invariant holds, with an explanation if it fails.

use super::report::SimulationReport;
use super::{GraphSpec, Team};

/// Result of evaluating a behavioral predicate.
#[derive(Debug)]
pub struct PredicateResult {
    pub holds: bool,
    pub name: &'static str,
    pub explanation: String,
}

impl PredicateResult {
    fn pass(name: &'static str) -> Self {
        Self {
            holds: true,
            name,
            explanation: String::new(),
        }
    }

    fn fail(name: &'static str, explanation: String) -> Self {
        Self {
            holds: false,
            name,
            explanation,
        }
    }
}

/// Nodes with only one endorser (single attachment point) should have
/// low diversity scores, making them ineligible for high-trust contexts.
///
/// Rationale: a single endorser means a single point of failure / trust.
/// The engine should reflect this by assigning diversity = 1.
pub fn single_attachment_implies_low_diversity(
    spec: &GraphSpec,
    report: &SimulationReport,
) -> PredicateResult {
    let name = "single_attachment_implies_low_diversity";
    let mut violations = Vec::new();

    for node in spec.all_nodes() {
        let endorser_count = spec.endorser_count(node.id);
        if endorser_count == 1 {
            let diversity = report.diversity(node.id);
            if diversity > 1 {
                violations.push(format!(
                    "{} has 1 endorser but diversity={diversity}",
                    node.name
                ));
            }
        }
    }

    if violations.is_empty() {
        PredicateResult::pass(name)
    } else {
        PredicateResult::fail(name, violations.join("; "))
    }
}

/// Red team nodes should not pass the given constraint threshold.
///
/// This is the fundamental Sybil resistance property: adversarial nodes
/// should be blocked by trust mechanisms.
pub fn red_nodes_blocked(
    spec: &GraphSpec,
    report: &SimulationReport,
    max_distance: f32,
    min_diversity: i32,
) -> PredicateResult {
    let name = "red_nodes_blocked";
    let mut violations = Vec::new();

    for node in spec.all_nodes() {
        if node.team != Team::Red {
            continue;
        }
        let distance = report.distance(node.id);
        let diversity = report.diversity(node.id);

        let within_distance = distance.map_or(false, |d| d <= max_distance);
        let meets_diversity = diversity >= min_diversity;

        if within_distance && meets_diversity {
            violations.push(format!(
                "{}: d={}, div={} — passes threshold (max_d={max_distance}, min_div={min_diversity})",
                node.name,
                distance.map_or("none".to_string(), |d| format!("{d:.2}")),
                diversity,
            ));
        }
    }

    if violations.is_empty() {
        PredicateResult::pass(name)
    } else {
        PredicateResult::fail(name, violations.join("; "))
    }
}

/// Blue team nodes should remain reachable (have a finite distance).
///
/// If a mechanism causes blue nodes to become unreachable, it has
/// unacceptable collateral damage.
pub fn blue_nodes_reachable(spec: &GraphSpec, report: &SimulationReport) -> PredicateResult {
    let name = "blue_nodes_reachable";
    let mut violations = Vec::new();

    for node in spec.all_nodes() {
        if node.team != Team::Blue {
            continue;
        }
        if report.distance(node.id).is_none() {
            violations.push(format!("{} is unreachable", node.name));
        }
    }

    if violations.is_empty() {
        PredicateResult::pass(name)
    } else {
        PredicateResult::fail(name, violations.join("; "))
    }
}

/// No single denouncement should change a blue node's eligibility.
///
/// This is the anti-weaponization property: a single adversarial actor
/// should not be able to remove a legitimate user's access.
///
/// Compares a before and after report. Checks that all blue nodes that
/// were eligible before are still eligible after.
pub fn no_single_denounce_changes_blue_eligibility(
    spec: &GraphSpec,
    before: &SimulationReport,
    after: &SimulationReport,
    max_distance: f32,
    min_diversity: i32,
) -> PredicateResult {
    let name = "no_single_denounce_changes_blue_eligibility";
    let mut violations = Vec::new();

    for node in spec.all_nodes() {
        if node.team != Team::Blue {
            continue;
        }

        let was_eligible = before
            .distance(node.id)
            .map_or(false, |d| d <= max_distance)
            && before.diversity(node.id) >= min_diversity;

        let still_eligible = after.distance(node.id).map_or(false, |d| d <= max_distance)
            && after.diversity(node.id) >= min_diversity;

        if was_eligible && !still_eligible {
            violations.push(format!(
                "{} lost eligibility: d={}->{}, div={}->{}",
                node.name,
                before
                    .distance(node.id)
                    .map_or("none".to_string(), |d| format!("{d:.2}")),
                after
                    .distance(node.id)
                    .map_or("none".to_string(), |d| format!("{d:.2}")),
                before.diversity(node.id),
                after.diversity(node.id),
            ));
        }
    }

    if violations.is_empty() {
        PredicateResult::pass(name)
    } else {
        PredicateResult::fail(name, violations.join("; "))
    }
}

/// Colluding rings should not generate diversity > 1 for their members.
///
/// A ring where every node is endorsed by exactly one other ring member
/// should not fool the diversity metric.
pub fn ring_diversity_bounded(
    spec: &GraphSpec,
    report: &SimulationReport,
    ring_node_ids: &[uuid::Uuid],
    max_expected_diversity: i32,
) -> PredicateResult {
    let name = "ring_diversity_bounded";
    let mut violations = Vec::new();

    for &id in ring_node_ids {
        let diversity = report.diversity(id);
        if diversity > max_expected_diversity {
            let node_name = spec.node_name(id);
            violations.push(format!(
                "{node_name}: diversity={diversity} > max={max_expected_diversity}"
            ));
        }
    }

    if violations.is_empty() {
        PredicateResult::pass(name)
    } else {
        PredicateResult::fail(name, violations.join("; "))
    }
}
