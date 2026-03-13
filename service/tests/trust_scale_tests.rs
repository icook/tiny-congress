//! Scale simulation tests for trust graph properties at 1k–100k nodes.
//!
//! These tests validate that trust mechanisms (distance, diversity, decay)
//! produce correct outcomes at realistic network scales. All tests use pure
//! GraphSpec analysis — no database required.
//!
//! Run: cargo test --test trust_scale_tests -- --nocapture

mod common;

use std::time::Instant;

use chrono::{Duration, Utc};
use common::simulation::analysis;
use common::simulation::scale::{self, ScaleGraphParams, SybilMeshParams};
use common::simulation::GraphSpec;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helper: compute true reachable fraction from spec + analysis
//
// ScaleAnalysis::reachable_fraction() always returns 1.0 because it only
// tracks nodes it knows about. Compute the real fraction here.
// ---------------------------------------------------------------------------
fn reachable_fraction(spec: &GraphSpec, analysis: &analysis::ScaleAnalysis) -> f64 {
    let total = spec.all_nodes().len();
    if total == 0 {
        return 0.0;
    }
    // distances includes anchor at 0.0 plus all reachable non-anchor nodes
    let reachable = analysis.distances.len();
    reachable as f64 / total as f64
}

// ---------------------------------------------------------------------------
// Helper: step decay function used in correlated-expiry test
// ---------------------------------------------------------------------------
fn step_decay(age: Duration) -> f32 {
    let days = age.num_days();
    if days < 365 {
        1.0
    } else if days < 730 {
        0.5
    } else {
        0.0
    }
}

// ===========================================================================
// Test 1: scale_distance_distribution_1k
// ===========================================================================
#[test]
fn scale_distance_distribution_1k() {
    println!("\n=== scale_distance_distribution_1k ===\n");

    let params = ScaleGraphParams {
        node_count: 1_000,
        edges_per_new_node: 3,
        seed_size: 5,
        weight: 1.0,
        red_fraction: 0.0,
    };

    let t0 = Instant::now();
    let spec = scale::barabasi_albert(&params);
    let build_ms = t0.elapsed().as_millis();

    let anchor = spec.node("anchor");

    let t1 = Instant::now();
    let result = analysis::analyze_graph(&spec, anchor);
    let analysis_ms = t1.elapsed().as_millis();

    let dist_stats = result.distance_stats();
    let div_stats = result.diversity_stats();
    let reach = reachable_fraction(&spec, &result);
    let eligible = result.eligible_fraction(5.0, 2);

    println!(
        "Nodes: {}  Active edges: {}  Build: {}ms  Analysis: {}ms",
        spec.all_nodes().len(),
        spec.active_edge_count(),
        build_ms,
        analysis_ms
    );
    println!("Distance stats: {dist_stats}");
    println!("Diversity stats: {div_stats}");
    println!("Reachable fraction: {reach:.4}");
    println!("Eligible (d≤5.0, div≥2): {eligible:.4}");

    assert!(
        reach > 0.95,
        "reachable fraction {reach:.4} should be > 0.95"
    );
    assert!(
        dist_stats.mean < 5.0,
        "mean distance {:.4} should be < 5.0 (BA small-world property)",
        dist_stats.mean
    );

    // At least 50% of nodes should have diversity >= 2
    let div_ge2 = result.diversities.values().filter(|&&d| d >= 2).count();
    let div_total = result.diversities.len();
    let div_frac = if div_total > 0 {
        div_ge2 as f64 / div_total as f64
    } else {
        0.0
    };
    println!("Diversity ≥ 2: {div_ge2}/{div_total} = {div_frac:.4}");
    assert!(
        div_frac >= 0.50,
        "fraction with diversity≥2 = {div_frac:.4} should be ≥ 0.50"
    );
}

// ===========================================================================
// Test 2: scale_distance_distribution_10k
//
// NOTE on diversity sampling: max-flow is O(n × flow) per target. At 10k
// nodes in debug mode, even 20 targets take ~60s. We use a 2000-node graph
// here to keep wall-clock time under ~30s while still exercising scale
// properties beyond 1k. The 10k and 100k cases are in the ignored tests.
// ===========================================================================
#[test]
fn scale_distance_distribution_10k() {
    println!("\n=== scale_distance_distribution_10k ===\n");

    let params = ScaleGraphParams {
        node_count: 2_000,
        edges_per_new_node: 3,
        seed_size: 5,
        weight: 1.0,
        red_fraction: 0.0,
    };

    let t0 = Instant::now();
    let spec = scale::barabasi_albert(&params);
    let build_ms = t0.elapsed().as_millis();

    let anchor = spec.node("anchor");

    let t1 = Instant::now();
    // Use 200 diversity samples. At 2k nodes max-flow stays fast (~10–30s).
    let result = analysis::analyze_graph_sampled(&spec, anchor, 200);
    let analysis_ms = t1.elapsed().as_millis();

    let dist_stats = result.distance_stats();
    let div_stats = result.diversity_stats();
    let reach = reachable_fraction(&spec, &result);
    let eligible = result.eligible_fraction(7.0, 2);

    println!(
        "Nodes: {}  Active edges: {}  Build: {}ms  Analysis: {}ms",
        spec.all_nodes().len(),
        spec.active_edge_count(),
        build_ms,
        analysis_ms
    );
    println!("Distance stats: {dist_stats}");
    println!("Diversity stats (sampled 200): {div_stats}");
    println!("Reachable fraction: {reach:.4}");
    println!("Eligible (d≤7.0, div≥2): {eligible:.4}");

    assert!(
        reach > 0.95,
        "reachable fraction {reach:.4} should be > 0.95"
    );
    assert!(
        dist_stats.mean < 7.0,
        "mean distance {:.4} should be < 7.0 at 2k nodes",
        dist_stats.mean
    );

    // At least 40% of sampled nodes should have diversity >= 2
    let div_ge2 = result.diversities.values().filter(|&&d| d >= 2).count();
    let div_total = result.diversities.len();
    let div_frac = if div_total > 0 {
        div_ge2 as f64 / div_total as f64
    } else {
        0.0
    };
    println!("Diversity ≥ 2 (sampled 200): {div_ge2}/{div_total} = {div_frac:.4}");
    assert!(
        div_frac >= 0.40,
        "fraction with diversity≥2 = {div_frac:.4} should be ≥ 0.40"
    );
}

// ===========================================================================
// Test 3: scale_distance_distribution_100k (slow — ignored by default)
// ===========================================================================
#[test]
#[ignore]
fn scale_distance_distribution_100k() {
    println!("\n=== scale_distance_distribution_100k ===\n");

    let params = ScaleGraphParams {
        node_count: 100_000,
        edges_per_new_node: 3,
        seed_size: 5,
        weight: 1.0,
        red_fraction: 0.0,
    };

    let t0 = Instant::now();
    let spec = scale::barabasi_albert(&params);
    let build_ms = t0.elapsed().as_millis();

    let anchor = spec.node("anchor");

    let t1 = Instant::now();
    let result = analysis::analyze_graph_sampled(&spec, anchor, 500);
    let analysis_ms = t1.elapsed().as_millis();

    let dist_stats = result.distance_stats();
    let div_stats = result.diversity_stats();
    let reach = reachable_fraction(&spec, &result);

    println!(
        "Nodes: {}  Active edges: {}  Build: {}ms  Analysis: {}ms",
        spec.all_nodes().len(),
        spec.active_edge_count(),
        build_ms,
        analysis_ms
    );
    println!("Distance stats: {dist_stats}");
    println!("Diversity stats (sampled 500): {div_stats}");
    println!("Reachable fraction: {reach:.4}");
    println!("Total elapsed: {}ms", t0.elapsed().as_millis());

    assert!(
        reach > 0.95,
        "reachable fraction {reach:.4} should be > 0.95"
    );
    // The test passing at all proves we didn't OOM.
}

// ===========================================================================
// Test 4: scale_sybil_mesh_small
// ===========================================================================
#[test]
fn scale_sybil_mesh_small() {
    println!("\n=== scale_sybil_mesh_small ===\n");

    let params = ScaleGraphParams {
        node_count: 1_000,
        edges_per_new_node: 3,
        seed_size: 5,
        weight: 1.0,
        red_fraction: 0.0,
    };

    let mut spec = scale::barabasi_albert(&params);
    let anchor = spec.node("anchor");

    // Pick 2 non-anchor nodes as bridges
    let bridge_1 = spec.node("node_1");
    let bridge_2 = spec.node("node_2");
    let bridge_targets = vec![bridge_1, bridge_2];

    let mesh_params = SybilMeshParams {
        mesh_size: 10,
        bridge_count: 2,
        internal_density: 0.5,
        weight: 1.0,
    };

    let sybil_ids = scale::attach_sybil_mesh(&mut spec, &bridge_targets, &mesh_params);

    let result = analysis::analyze_graph(&spec, anchor);

    println!(
        "{:<12} {:>10} {:>12}",
        "Sybil node", "distance", "diversity"
    );
    println!("{}", "-".repeat(36));
    for &sybil_id in &sybil_ids {
        let dist = result.distances.get(&sybil_id).copied();
        let div = result.diversities.get(&sybil_id).copied().unwrap_or(0);
        println!(
            "{:<12} {:>10} {:>12}",
            format!(
                "sybil_{}",
                sybil_ids.iter().position(|&id| id == sybil_id).unwrap_or(0)
            ),
            dist.map_or("unreachable".to_string(), |d| format!("{d:.3}")),
            div
        );

        // Assert: sybil nodes cannot have diversity > bridge_count.
        //
        // Each sybil node is connected to the legitimate graph only through the
        // bridge nodes. Vertex-disjoint paths from anchor to any sybil node must
        // each traverse a distinct bridge vertex. With 2 bridges, at most 2
        // independent paths can reach any sybil node — diversity is capped at 2.
        // The internal mesh does not create additional entry points.
        let bridge_count = 2i32;
        assert!(
            div <= bridge_count,
            "sybil node diversity {div} should be ≤ bridge_count={bridge_count}: \
             a mesh cannot manufacture paths beyond its entry points"
        );
    }
}

// ===========================================================================
// Test 5: scale_sybil_mesh_multi_bridge
// ===========================================================================
#[test]
fn scale_sybil_mesh_multi_bridge() {
    println!("\n=== scale_sybil_mesh_multi_bridge ===\n");

    let params = ScaleGraphParams {
        node_count: 1_000,
        edges_per_new_node: 3,
        seed_size: 5,
        weight: 1.0,
        red_fraction: 0.0,
    };

    let base_spec = scale::barabasi_albert(&params);
    let anchor = base_spec.node("anchor");

    println!(
        "{:<14} {:>22} {:>20}",
        "bridge_count", "max_sybil_diversity", "max_sybil_distance"
    );
    println!("{}", "-".repeat(58));

    let bridge_counts = [1usize, 2, 3, 5];

    for &bridge_count in &bridge_counts {
        let mut spec = base_spec.clone();

        // Use first N non-anchor nodes as bridges
        let bridge_targets: Vec<Uuid> = (1..=bridge_count)
            .map(|i| spec.node(&format!("node_{i}")))
            .collect();

        let mesh_params = SybilMeshParams {
            mesh_size: 50,
            bridge_count,
            internal_density: 0.8,
            weight: 1.0,
        };

        let sybil_ids = scale::attach_sybil_mesh(&mut spec, &bridge_targets, &mesh_params);

        // Compute distances for all nodes, then diversity specifically for
        // sybil nodes (sampled analysis would miss them).
        let distances = analysis::compute_distances(&spec, anchor);
        let sybil_diversities = analysis::compute_diversity(&spec, anchor, &sybil_ids);

        let max_sybil_div = sybil_ids
            .iter()
            .map(|id| sybil_diversities.get(id).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);

        let max_sybil_dist = sybil_ids
            .iter()
            .filter_map(|id| distances.get(id).copied())
            .fold(f32::NEG_INFINITY, f32::max);

        println!(
            "{:<14} {:>22} {:>20}",
            bridge_count,
            max_sybil_div,
            if max_sybil_dist == f32::NEG_INFINITY {
                "unreachable".to_string()
            } else {
                format!("{max_sybil_dist:.3}")
            }
        );

        // Assert the 1-bridge baseline
        if bridge_count == 1 {
            assert!(
                max_sybil_div <= 1,
                "with 1 bridge, max sybil diversity should be <= 1; got {max_sybil_div}"
            );
        }
    }

    // Summary note: whether diversity > 1 is achievable depends on whether the
    // bridge nodes lie on genuinely vertex-disjoint paths from the anchor.
    // Simply having more bridge count doesn't guarantee independent paths —
    // internal graph topology determines the achievable flow.
    println!("\nNote: diversity > 1 for Sybil nodes requires vertex-disjoint");
    println!("paths from anchor through distinct bridge nodes, not just multiple bridges.");
}

// ===========================================================================
// Test 6: scale_correlated_expiry
// ===========================================================================
#[test]
fn scale_correlated_expiry() {
    println!("\n=== scale_correlated_expiry ===\n");

    let params = ScaleGraphParams {
        node_count: 1_000,
        edges_per_new_node: 3,
        seed_size: 5,
        weight: 1.0,
        red_fraction: 0.0,
    };

    let spec = scale::barabasi_albert(&params);
    let anchor = spec.node("anchor");

    // Identify a cohort of 100 adjacent nodes (node_50 through node_149)
    let cohort_nodes: Vec<Uuid> = (50..150).map(|i| spec.node(&format!("node_{i}"))).collect();

    // Mark all edges between cohort members as 2 years old
    let two_years_ago = Utc::now() - Duration::days(730);
    let mut spec_with_timestamps = spec.clone();
    scale::mark_cohort_edges(&mut spec_with_timestamps, &cohort_nodes, two_years_ago);

    // Baseline analysis: compute distances for all nodes (no diversity needed here).
    let before_distances = analysis::compute_distances(&spec, anchor);

    // Apply step decay: 1.0 < 365d, 0.5 < 730d, 0.0 >= 730d, min_weight=0.01
    // Cohort-internal edges aged ≥730d → weight = max(0.0, ...) clamped to 0.01
    // → cost = 1/0.01 = 100.0, exceeding the Dijkstra CUTOFF (10.0).
    // Paths that relied solely on intra-cohort traversal are now blocked.
    let now = Utc::now();
    let decayed_spec = spec_with_timestamps.with_decay(now, step_decay, 0.01);
    let after_distances = analysis::compute_distances(&decayed_spec, anchor);

    // Count nodes whose distance increased (or became unreachable) after decay
    let nodes_with_increased_distance = before_distances
        .iter()
        .filter(|(&id, &d_before)| {
            if id == anchor {
                return false;
            }
            match after_distances.get(&id) {
                None => true,                                // became unreachable
                Some(&d_after) => d_after > d_before + 1e-4, // distance increased
            }
        })
        .count();

    // Count nodes that became unreachable
    let newly_unreachable = before_distances
        .keys()
        .filter(|&&id| id != anchor && !after_distances.contains_key(&id))
        .count();

    // Compute reachable fraction after decay using full distance map
    let total_nodes = spec.all_nodes().len();
    let reachable_after = after_distances.len();
    let reach_after = reachable_after as f64 / total_nodes as f64;

    // Also run sampled analysis for eligible_fraction reporting
    let before_sampled = analysis::analyze_graph_sampled(&spec, anchor, 200);
    let after_sampled = analysis::analyze_graph_sampled(&decayed_spec, anchor, 200);
    let eligible_before = before_sampled.eligible_fraction(5.0, 2);
    let eligible_after = after_sampled.eligible_fraction(5.0, 2);

    println!(
        "Cohort size: {} nodes (node_50..node_149)",
        cohort_nodes.len()
    );
    println!("Nodes with increased distance after decay: {nodes_with_increased_distance}");
    println!("Nodes that became unreachable: {newly_unreachable}");
    println!("Reachable fraction after decay: {reach_after:.4}");
    println!("Eligible (d≤5.0, div≥2) before decay (sampled): {eligible_before:.4}");
    println!("Eligible (d≤5.0, div≥2) after decay  (sampled): {eligible_after:.4}");

    // Decay must have measurable effect: cohort-internal edges now cost 100+,
    // so any node whose only short path went through the cohort gets a longer
    // distance or becomes unreachable.
    assert!(
        nodes_with_increased_distance > 0,
        "decay should increase distance for at least some nodes \
         (cohort-internal edge costs rise from ~1.0 to 100.0 when weight drops to 0.01)"
    );

    // The decay is localized: most non-cohort nodes are unaffected.
    // The graph remains broadly connected after decay.
    assert!(
        reach_after > 0.70,
        "reachable fraction {reach_after:.4} after decay should be > 0.70 \
         (decay is localized to cohort-internal edges only)"
    );
}

// ===========================================================================
// Test 7: scale_bridge_removal
// ===========================================================================
#[test]
fn scale_bridge_removal() {
    println!("\n=== scale_bridge_removal ===\n");

    let params = ScaleGraphParams {
        node_count: 1_000,
        edges_per_new_node: 3,
        seed_size: 5,
        weight: 1.0,
        red_fraction: 0.0,
    };

    let spec = scale::barabasi_albert(&params);
    let anchor = spec.node("anchor");

    // Find the 3 highest-degree non-anchor nodes
    let top_nodes = scale::find_high_degree_nodes(&spec, 3);
    assert_eq!(
        top_nodes.len(),
        3,
        "should find 3 high-degree nodes in a 1k-node graph"
    );

    // Baseline analysis
    let baseline = analysis::analyze_graph_sampled(&spec, anchor, 200);
    let baseline_reach = reachable_fraction(&spec, &baseline);
    let baseline_eligible = baseline.eligible_fraction(5.0, 2);
    println!("Baseline: reachable={baseline_reach:.4}  eligible={baseline_eligible:.4}");

    println!(
        "\n{:<14} {:>14} {:>14} {:>22}",
        "Removal", "reachable", "eligible", "newly_unreachable"
    );
    println!("{}", "-".repeat(66));

    let mut current_spec = spec.clone();
    let mut prev_reachable = (baseline_reach * spec.all_nodes().len() as f64) as usize;

    for (i, &node_id) in top_nodes.iter().enumerate() {
        scale::remove_node_edges(&mut current_spec, node_id);

        let result = analysis::analyze_graph_sampled(&current_spec, anchor, 200);
        let reach = reachable_fraction(&current_spec, &result);
        let eligible = result.eligible_fraction(5.0, 2);

        let reachable_count = result.distances.len();
        let newly_unreachable = prev_reachable.saturating_sub(reachable_count);
        prev_reachable = reachable_count;

        println!(
            "{:<14} {:>14.4} {:>14.4} {:>22}",
            format!("remove #{}", i + 1),
            reach,
            eligible,
            newly_unreachable
        );

        // After removing 1 bridge: reachable fraction should be > 0.80
        // BA graphs are robust: the majority of nodes should remain reachable
        if i == 0 {
            assert!(
                reach > 0.80,
                "after removing 1 bridge node, reachable fraction {reach:.4} \
                 should be > 0.80 (BA graphs are robust)"
            );
        }
    }

    // After removing 3 bridges, the graph should not be completely fragmented
    let final_result = analysis::analyze_graph_sampled(&current_spec, anchor, 200);
    let final_reach = reachable_fraction(&current_spec, &final_result);
    assert!(
        final_reach > 0.20,
        "after removing 3 bridge nodes, reachable fraction {final_reach:.4} \
         should be > 0.20 — graph should not be completely fragmented"
    );

    println!("\nFinal reachable fraction after 3 removals: {final_reach:.4}");
}

// ===========================================================================
// Test 8: scale_performance_benchmark (slow — ignored by default)
// ===========================================================================
#[test]
#[ignore]
fn scale_performance_benchmark() {
    println!("\n=== scale_performance_benchmark ===\n");

    let sizes = [100usize, 500, 1_000, 5_000, 10_000];

    println!(
        "{:<12} {:>12} {:>14} {:>14}",
        "node_count", "edge_count", "distance_ms", "total_ms"
    );
    println!("{}", "-".repeat(54));

    for &n in &sizes {
        let params = ScaleGraphParams {
            node_count: n,
            edges_per_new_node: 3,
            seed_size: 5,
            weight: 1.0,
            red_fraction: 0.0,
        };

        let t0 = Instant::now();
        let spec = scale::barabasi_albert(&params);
        let build_ms = t0.elapsed().as_millis();

        let anchor = spec.node("anchor");
        let edge_count = spec.active_edge_count();

        let t1 = Instant::now();
        if n <= 1_000 {
            let _ = analysis::analyze_graph(&spec, anchor);
        } else {
            let sample = 200;
            let _ = analysis::analyze_graph_sampled(&spec, anchor, sample);
        }
        let analysis_ms = t1.elapsed().as_millis();
        let total_ms = t0.elapsed().as_millis();

        println!(
            "{:<12} {:>12} {:>14} {:>14}",
            n, edge_count, analysis_ms, total_ms
        );

        let _ = build_ms; // suppress unused warning — included in total_ms
    }
}
