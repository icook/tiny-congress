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

/// Nodes with a finite distance should be reachable via active edges
/// from the anchor. Guards against phantom reachability in the engine.
pub fn unreachable_nodes_have_no_distance(
    spec: &GraphSpec,
    report: &SimulationReport,
) -> PredicateResult {
    use std::collections::{HashSet, VecDeque};

    let name = "unreachable_nodes_have_no_distance";

    // BFS from anchor over active edges
    let anchor_id = report.anchor_id;
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    reachable.insert(anchor_id);
    queue.push_back(anchor_id);

    while let Some(current) = queue.pop_front() {
        for edge in spec.outbound_edges(current) {
            if reachable.insert(edge.to) {
                queue.push_back(edge.to);
            }
        }
    }

    let mut violations = Vec::new();
    for node in spec.all_nodes() {
        let has_distance = report.distance(node.id).is_some();
        let is_reachable = reachable.contains(&node.id);

        if has_distance && !is_reachable {
            violations.push(format!(
                "{} has distance but no active path from anchor",
                node.name
            ));
        }
    }

    if violations.is_empty() {
        PredicateResult::pass(name)
    } else {
        PredicateResult::fail(name, violations.join("; "))
    }
}

/// Nodes in an isolated cluster (≤1 external endorser) should have
/// diversity bounded by external connections. A fully-connected cluster
/// attached through a single bridge edge should have diversity ≤ 1.
pub fn isolated_cluster_diversity_bounded(
    spec: &GraphSpec,
    report: &SimulationReport,
    cluster_ids: &[uuid::Uuid],
) -> PredicateResult {
    let name = "isolated_cluster_diversity_bounded";
    let mut violations = Vec::new();

    let cluster_set: std::collections::HashSet<uuid::Uuid> = cluster_ids.iter().copied().collect();

    for &id in cluster_ids {
        let external_endorsers = spec
            .inbound_edges(id)
            .iter()
            .filter(|e| !cluster_set.contains(&e.from))
            .count();

        let diversity = report.diversity(id);
        let max_expected = external_endorsers.max(1) as i32;

        if diversity > max_expected {
            let node_name = spec.node_name(id);
            violations.push(format!(
                "{node_name}: diversity={diversity} > expected max={max_expected} \
                 (external_endorsers={external_endorsers})"
            ));
        }
    }

    if violations.is_empty() {
        PredicateResult::pass(name)
    } else {
        PredicateResult::fail(name, violations.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::super::report::{NodeScore, SimulationReport};
    use super::super::{GraphSpec, Team};
    use super::*;

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn make_report(anchor_id: Uuid, scores: Vec<NodeScore>) -> SimulationReport {
        SimulationReport { anchor_id, scores }
    }

    fn make_score(id: Uuid, team: Team, distance: Option<f32>, diversity: i32) -> NodeScore {
        NodeScore {
            id,
            name: format!("{team}-{}", &id.to_string()[..8]),
            team,
            distance,
            diversity,
        }
    }

    fn make_spec_with_nodes(nodes: &[(Uuid, &str, Team)]) -> GraphSpec {
        let mut spec = GraphSpec::new();
        for &(id, name, team) in nodes {
            spec.add_node(name, team, id);
        }
        spec
    }

    // ---------------------------------------------------------------------------
    // single_attachment_implies_low_diversity
    // ---------------------------------------------------------------------------

    #[test]
    fn single_attachment_low_diversity_passes_when_endorser_one_and_diversity_one() {
        let anchor = Uuid::new_v4();
        let target = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge(anchor, target, 1.0);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(target, Team::Blue, Some(1.0), 1),
            ],
        );

        let result = single_attachment_implies_low_diversity(&spec, &report);
        assert!(result.holds, "should pass: single endorser, diversity=1");
    }

    #[test]
    fn single_attachment_low_diversity_fails_when_diversity_above_one() {
        let anchor = Uuid::new_v4();
        let target = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge(anchor, target, 1.0); // only one endorser

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                // diversity=3 contradicts single-endorser structure
                make_score(target, Team::Blue, Some(1.0), 3),
            ],
        );

        let result = single_attachment_implies_low_diversity(&spec, &report);
        assert!(!result.holds, "should fail: 1 endorser but diversity=3");
        assert!(result.explanation.contains("target"));
    }

    #[test]
    fn single_attachment_low_diversity_ignores_multi_endorser_nodes() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let target = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("a", Team::Blue, a);
        spec.add_node("b", Team::Blue, b);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge(a, target, 1.0);
        spec.add_edge(b, target, 1.0); // two endorsers — diversity=2 is fine

        let report = make_report(
            a,
            vec![
                make_score(a, Team::Blue, Some(0.0), 1),
                make_score(b, Team::Blue, Some(0.0), 1),
                make_score(target, Team::Blue, Some(1.0), 2),
            ],
        );

        let result = single_attachment_implies_low_diversity(&spec, &report);
        assert!(
            result.holds,
            "should pass: node has 2 endorsers so high diversity is allowed"
        );
    }

    #[test]
    fn single_attachment_low_diversity_passes_on_empty_graph() {
        let anchor = Uuid::new_v4();
        let spec = GraphSpec::new();
        let report = make_report(anchor, vec![]);
        let result = single_attachment_implies_low_diversity(&spec, &report);
        assert!(result.holds, "empty graph should trivially pass");
    }

    // ---------------------------------------------------------------------------
    // red_nodes_blocked
    // ---------------------------------------------------------------------------

    #[test]
    fn red_nodes_blocked_passes_when_red_has_no_distance() {
        let anchor = Uuid::new_v4();
        let red = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("red", Team::Red, red);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(red, Team::Red, None, 0), // unreachable
            ],
        );

        let result = red_nodes_blocked(&spec, &report, 3.0, 2);
        assert!(result.holds, "unreachable red node should be blocked");
    }

    #[test]
    fn red_nodes_blocked_passes_when_red_distance_exceeds_threshold() {
        let anchor = Uuid::new_v4();
        let red = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("red", Team::Red, red);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(red, Team::Red, Some(5.0), 3), // distance > max_distance
            ],
        );

        let result = red_nodes_blocked(&spec, &report, 3.0, 2);
        assert!(
            result.holds,
            "red beyond distance threshold should be blocked"
        );
    }

    #[test]
    fn red_nodes_blocked_passes_when_red_diversity_below_min() {
        let anchor = Uuid::new_v4();
        let red = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("red", Team::Red, red);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(red, Team::Red, Some(1.0), 1), // diversity < min_diversity=2
            ],
        );

        let result = red_nodes_blocked(&spec, &report, 3.0, 2);
        assert!(
            result.holds,
            "red with low diversity should be blocked even if close"
        );
    }

    #[test]
    fn red_nodes_blocked_fails_when_red_passes_both_thresholds() {
        let anchor = Uuid::new_v4();
        let red = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("red", Team::Red, red);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(red, Team::Red, Some(1.0), 3), // within distance AND above diversity
            ],
        );

        let result = red_nodes_blocked(&spec, &report, 3.0, 2);
        assert!(!result.holds, "red passing both thresholds is a violation");
        assert!(result.explanation.contains("red"));
    }

    #[test]
    fn red_nodes_blocked_passes_with_no_red_nodes() {
        let anchor = Uuid::new_v4();
        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        let report = make_report(anchor, vec![make_score(anchor, Team::Blue, Some(0.0), 1)]);
        let result = red_nodes_blocked(&spec, &report, 3.0, 2);
        assert!(result.holds, "no red nodes means no violations");
    }

    // ---------------------------------------------------------------------------
    // blue_nodes_reachable
    // ---------------------------------------------------------------------------

    #[test]
    fn blue_nodes_reachable_passes_when_all_blue_have_distance() {
        let anchor = Uuid::new_v4();
        let blue = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("blue", Team::Blue, blue);
        spec.add_edge(anchor, blue, 1.0);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(blue, Team::Blue, Some(1.0), 1),
            ],
        );

        let result = blue_nodes_reachable(&spec, &report);
        assert!(result.holds, "reachable blue node should pass");
    }

    #[test]
    fn blue_nodes_reachable_fails_when_a_blue_is_unreachable() {
        let anchor = Uuid::new_v4();
        let blue = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("blue", Team::Blue, blue);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(blue, Team::Blue, None, 0), // unreachable
            ],
        );

        let result = blue_nodes_reachable(&spec, &report);
        assert!(!result.holds, "unreachable blue is a violation");
        assert!(result.explanation.contains("blue"));
    }

    #[test]
    fn blue_nodes_reachable_ignores_red_unreachable_nodes() {
        let anchor = Uuid::new_v4();
        let red = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("red", Team::Red, red);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(red, Team::Red, None, 0), // red unreachable — fine
            ],
        );

        let result = blue_nodes_reachable(&spec, &report);
        assert!(result.holds, "unreachable red nodes are not violations");
    }

    // ---------------------------------------------------------------------------
    // no_single_denounce_changes_blue_eligibility
    // ---------------------------------------------------------------------------

    #[test]
    fn no_single_denounce_passes_when_blue_stays_eligible() {
        let anchor = Uuid::new_v4();
        let blue = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("blue", Team::Blue, blue);

        let before = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(blue, Team::Blue, Some(1.0), 2),
            ],
        );
        let after = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(blue, Team::Blue, Some(1.0), 2), // unchanged
            ],
        );

        let result = no_single_denounce_changes_blue_eligibility(&spec, &before, &after, 3.0, 2);
        assert!(result.holds, "blue stays eligible — no violation");
    }

    #[test]
    fn no_single_denounce_fails_when_eligible_blue_loses_eligibility() {
        let anchor = Uuid::new_v4();
        let blue = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("blue", Team::Blue, blue);

        let before = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(blue, Team::Blue, Some(1.0), 2), // eligible
            ],
        );
        let after = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(blue, Team::Blue, Some(5.0), 1), // now ineligible
            ],
        );

        let result = no_single_denounce_changes_blue_eligibility(&spec, &before, &after, 3.0, 2);
        assert!(
            !result.holds,
            "eligible blue losing eligibility is a violation"
        );
        assert!(result.explanation.contains("blue"));
    }

    #[test]
    fn no_single_denounce_passes_when_blue_was_ineligible_before() {
        // If blue was already ineligible before the denouncement, losing eligibility
        // is not a violation of this predicate.
        let anchor = Uuid::new_v4();
        let blue = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("blue", Team::Blue, blue);

        let before = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(blue, Team::Blue, Some(5.0), 1), // already ineligible
            ],
        );
        let after = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(blue, Team::Blue, None, 0), // still ineligible
            ],
        );

        let result = no_single_denounce_changes_blue_eligibility(&spec, &before, &after, 3.0, 2);
        assert!(
            result.holds,
            "already-ineligible blue losing more eligibility is not a new violation"
        );
    }

    #[test]
    fn no_single_denounce_ignores_red_nodes() {
        let anchor = Uuid::new_v4();
        let red = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("red", Team::Red, red);

        let before = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(red, Team::Red, Some(1.0), 3),
            ],
        );
        let after = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 2),
                make_score(red, Team::Red, None, 0), // red lost eligibility — not relevant
            ],
        );

        let result = no_single_denounce_changes_blue_eligibility(&spec, &before, &after, 3.0, 2);
        assert!(result.holds, "red node eligibility changes are ignored");
    }

    // ---------------------------------------------------------------------------
    // ring_diversity_bounded
    // ---------------------------------------------------------------------------

    #[test]
    fn ring_diversity_bounded_passes_when_all_within_max() {
        let anchor = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Red, a);
        spec.add_node("b", Team::Red, b);
        spec.add_node("c", Team::Red, c);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(a, Team::Red, Some(1.0), 1),
                make_score(b, Team::Red, Some(1.0), 1),
                make_score(c, Team::Red, Some(1.0), 1),
            ],
        );

        let result = ring_diversity_bounded(&spec, &report, &[a, b, c], 1);
        assert!(result.holds, "all ring members at diversity=1 within max=1");
    }

    #[test]
    fn ring_diversity_bounded_fails_when_member_exceeds_max() {
        let anchor = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Red, a);
        spec.add_node("b", Team::Red, b);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(a, Team::Red, Some(1.0), 3), // diversity=3 > max=1
                make_score(b, Team::Red, Some(1.0), 1),
            ],
        );

        let result = ring_diversity_bounded(&spec, &report, &[a, b], 1);
        assert!(
            !result.holds,
            "ring member exceeds max diversity — violation"
        );
    }

    #[test]
    fn ring_diversity_bounded_passes_for_empty_ring() {
        let anchor = Uuid::new_v4();
        let spec = GraphSpec::new();
        let report = make_report(anchor, vec![]);
        let result = ring_diversity_bounded(&spec, &report, &[], 1);
        assert!(result.holds, "empty ring trivially passes");
    }

    // ---------------------------------------------------------------------------
    // unreachable_nodes_have_no_distance
    // ---------------------------------------------------------------------------

    #[test]
    fn unreachable_nodes_have_no_distance_passes_when_reachable_has_distance() {
        let anchor = Uuid::new_v4();
        let blue = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("blue", Team::Blue, blue);
        spec.add_edge(anchor, blue, 1.0);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(blue, Team::Blue, Some(1.0), 1),
            ],
        );

        let result = unreachable_nodes_have_no_distance(&spec, &report);
        assert!(result.holds, "reachable node has distance — correct");
    }

    #[test]
    fn unreachable_nodes_have_no_distance_passes_when_unreachable_has_no_distance() {
        let anchor = Uuid::new_v4();
        let isolated = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("isolated", Team::Blue, isolated);
        // No edge — isolated is unreachable

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(isolated, Team::Blue, None, 0), // correctly has no distance
            ],
        );

        let result = unreachable_nodes_have_no_distance(&spec, &report);
        assert!(result.holds, "unreachable node correctly has no distance");
    }

    #[test]
    fn unreachable_nodes_have_no_distance_fails_when_phantom_distance_reported() {
        let anchor = Uuid::new_v4();
        let phantom = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("phantom", Team::Blue, phantom);
        // No edge — phantom is not reachable from anchor

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                // Engine bug: reports a distance even though there's no active path
                make_score(phantom, Team::Blue, Some(1.0), 1),
            ],
        );

        let result = unreachable_nodes_have_no_distance(&spec, &report);
        assert!(!result.holds, "phantom distance is a violation");
        assert!(result.explanation.contains("phantom"));
    }

    #[test]
    fn unreachable_nodes_have_no_distance_ignores_revoked_edges() {
        let anchor = Uuid::new_v4();
        let target = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("target", Team::Blue, target);
        spec.add_edge_revoked(anchor, target, 1.0); // revoked — not a real path

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                // Engine correctly reports no distance (revoked edge is not a path)
                make_score(target, Team::Blue, None, 0),
            ],
        );

        let result = unreachable_nodes_have_no_distance(&spec, &report);
        assert!(
            result.holds,
            "revoked edge should not count as an active path"
        );
    }

    // ---------------------------------------------------------------------------
    // isolated_cluster_diversity_bounded
    // ---------------------------------------------------------------------------

    #[test]
    fn isolated_cluster_diversity_bounded_passes_single_external_endorser() {
        let anchor = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Red, a);
        spec.add_node("b", Team::Red, b);
        // Only the anchor endorses a (1 external endorser for the cluster)
        spec.add_edge(anchor, a, 1.0);
        // b is internal to the cluster — endorsed only by a
        spec.add_edge(a, b, 1.0);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(a, Team::Red, Some(1.0), 1), // 1 external endorser → max=1
                make_score(b, Team::Red, Some(2.0), 1), // 0 external endorsers → max=1
            ],
        );

        let result = isolated_cluster_diversity_bounded(&spec, &report, &[a, b]);
        assert!(
            result.holds,
            "cluster members within external endorser bound"
        );
    }

    #[test]
    fn isolated_cluster_diversity_bounded_fails_when_diversity_exceeds_external_count() {
        let anchor = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Red, a);
        spec.add_node("b", Team::Red, b);
        spec.add_edge(anchor, a, 1.0); // 1 external endorser for a
        spec.add_edge(a, b, 1.0);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(a, Team::Red, Some(1.0), 3), // diversity=3 > max=1
                make_score(b, Team::Red, Some(2.0), 1),
            ],
        );

        let result = isolated_cluster_diversity_bounded(&spec, &report, &[a, b]);
        assert!(!result.holds, "inflated cluster diversity is a violation");
    }

    #[test]
    fn isolated_cluster_diversity_bounded_min_is_one_when_no_external_endorsers() {
        // Even with 0 external endorsers, max_expected = max(0, 1) = 1.
        // A node with diversity=1 should still pass.
        let anchor = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let mut spec = GraphSpec::new();
        spec.add_node("anchor", Team::Blue, anchor);
        spec.add_node("a", Team::Red, a);
        spec.add_node("b", Team::Red, b);
        // b is endorsed only by a (internal) — 0 external endorsers
        spec.add_edge(anchor, a, 1.0);
        spec.add_edge(a, b, 1.0);

        let report = make_report(
            anchor,
            vec![
                make_score(anchor, Team::Blue, Some(0.0), 1),
                make_score(a, Team::Red, Some(1.0), 1),
                make_score(b, Team::Red, Some(2.0), 1), // diversity=1 = floor of 1
            ],
        );

        let result = isolated_cluster_diversity_bounded(&spec, &report, &[b]);
        assert!(
            result.holds,
            "diversity=1 with 0 external endorsers is the minimum allowed"
        );
    }
}
