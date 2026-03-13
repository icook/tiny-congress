//! Scale test scenarios for trust graph analysis.
//!
//! Pure unit tests — no database, no async, no `#[shared_runtime_test]`.
//! Exercises Barabási-Albert graphs at increasing scale, Sybil mesh injection,
//! time decay, and hub removal.
//!
//! # Findings
//!
//! ## Distance & reachability (BA m=3)
//!
//! | Scale | Reachable | Mean dist | Max dist | Notes |
//! |-------|-----------|-----------|----------|-------|
//! | 1k    | 100%      | ~2.2      | 3        | All nodes within 3 hops |
//! | 2k    | 100%      | ~2.4      | 4        | Still very compact |
//! | 100k  | >99%      | ~3.0      | 5–6      | Sparse Dijkstra runs in seconds |
//!
//! ## Diversity
//!
//! At 1k–2k nodes (m=3), 100% of sampled nodes have diversity >= 2.
//! Mean diversity ~17 (highly connected graph). The BA model produces
//! power-law degree distributions where high-degree hubs create many
//! vertex-disjoint paths.
//!
//! ## Sybil resistance
//!
//! **Key finding: sybil diversity = bridge count.** A sybil mesh of any size
//! connected via N independently-compromised bridges achieves max diversity = N.
//! This validates the trust model's core resistance mechanism.
//!
//! ## Scale thresholds
//!
//! - **Sparse Dijkstra**: scales to 100k+ nodes (O(E log V), seconds at 100k).
//! - **Sparse Edmonds-Karp**: feasible at 1k–2k in debug, ~5k in release. At 100k
//!   nodes, computing diversity for all nodes is impractical — sampled analysis
//!   (first N by UUID) is the workaround.
//! - **Dense FlowGraph (production)**: O(n²) capacity matrix. ~160 GB at 100k.
//!   Must migrate to sparse implementation before 5k users (see #681).
//!
//! ## Resilience
//!
//! - **Hub removal**: removing top-3 hubs in a 1k BA graph drops reachability
//!   by <1% per hub — BA graphs are resilient to targeted removal.
//! - **Correlated decay**: 100-node cohort with expired edges causes ~2%
//!   reachability loss and increased distances for nearby nodes, but the rest
//!   of the graph remains unaffected.
//!
//! # Running
//!
//! ```sh
//! cargo test --test trust_scale_tests -- --nocapture         # active tests
//! cargo test --test trust_scale_tests -- --ignored --nocapture  # all including 100k
//! ```

mod common;

use uuid::Uuid;

use common::simulation::{
    analysis::{analyze_graph_sampled, compute_diversity},
    scale::{
        attach_sybil_mesh, barabasi_albert, find_high_degree_nodes, mark_cohort_edges,
        remove_node_edges, ScaleGraphParams, SybilMeshParams,
    },
};

// ---------------------------------------------------------------------------
// Test 1: 1k node distance distribution
// ---------------------------------------------------------------------------

/// 1k-node BA graph (m=3). Asserts basic connectivity and distance properties.
#[test]
fn scale_distance_distribution_1k() {
    let params = ScaleGraphParams {
        node_count: 1000,
        m: 3,
        seed_size: 5,
        seed: 42,
    };
    let spec = barabasi_albert(&params);
    let anchor = Uuid::from_u128(0);

    let analysis = analyze_graph_sampled(&spec, anchor, 100);
    let total = spec.all_nodes().len();

    let reachable = analysis.reachable_fraction(total);
    let dist_stats = analysis.distance_stats();
    let div_stats = analysis.diversity_stats();

    println!("=== scale_distance_distribution_1k ===");
    println!("Total nodes: {total}");
    println!("Reachable fraction: {reachable:.4}");
    println!("Distance: {dist_stats}");
    println!("Diversity (sample=100): {div_stats}");

    // Count nodes with diversity >= 2 in the sample
    let div2_count = analysis.diversities.values().filter(|&&d| d >= 2).count();
    let div2_frac = div2_count as f64 / analysis.diversities.len().max(1) as f64;
    println!(
        "Diversity>=2 fraction: {div2_frac:.4} ({div2_count}/{})",
        analysis.diversities.len()
    );

    assert!(
        reachable > 0.95,
        "reachable fraction should be >0.95, got {reachable:.4}"
    );
    assert!(
        dist_stats.mean < 5.0,
        "mean distance should be <5.0, got {:.3}",
        dist_stats.mean
    );
    assert!(
        div2_frac >= 0.50,
        "diversity>=2 fraction should be >=0.50, got {div2_frac:.4}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: 2k node distance distribution (proxy for 10k scale)
// ---------------------------------------------------------------------------

/// 2k-node BA graph (m=3) — runs as the "10k" scenario to stay fast in debug.
/// At 10k nodes, Edmonds-Karp diversity is too slow without release mode.
#[test]
fn scale_distance_distribution_10k() {
    let params = ScaleGraphParams {
        node_count: 2000,
        m: 3,
        seed_size: 5,
        seed: 42,
    };
    let spec = barabasi_albert(&params);
    let anchor = Uuid::from_u128(0);
    let total = spec.all_nodes().len();

    let analysis = analyze_graph_sampled(&spec, anchor, 200);

    let reachable = analysis.reachable_fraction(total);
    let dist_stats = analysis.distance_stats();
    let div_stats = analysis.diversity_stats();

    println!("=== scale_distance_distribution_10k (2k nodes) ===");
    println!("Total nodes: {total}");
    println!("Reachable fraction: {reachable:.4}");
    println!("Distance: {dist_stats}");
    println!("Diversity (sample=200): {div_stats}");

    let div2_count = analysis.diversities.values().filter(|&&d| d >= 2).count();
    let div2_frac = div2_count as f64 / analysis.diversities.len().max(1) as f64;
    println!(
        "Diversity>=2 fraction: {div2_frac:.4} ({div2_count}/{})",
        analysis.diversities.len()
    );

    assert!(
        reachable > 0.95,
        "reachable fraction should be >0.95, got {reachable:.4}"
    );
    assert!(
        dist_stats.mean < 7.0,
        "mean distance should be <7.0, got {:.3}",
        dist_stats.mean
    );
    assert!(
        div2_frac >= 0.40,
        "diversity>=2 fraction should be >=0.40, got {div2_frac:.4}"
    );
}

// ---------------------------------------------------------------------------
// Test 3: 100k node distance distribution (ignored by default)
// ---------------------------------------------------------------------------

/// 100k-node BA graph. Ignored by default — run explicitly with `--ignored`.
///
/// Test passing proves we didn't OOM. Only checks reachability and distance
/// (diversity via Edmonds-Karp is O(n * E) and prohibitively slow at this scale).
#[test]
#[ignore]
fn scale_distance_distribution_100k() {
    use std::time::Instant;

    let params = ScaleGraphParams {
        node_count: 100_000,
        m: 3,
        seed_size: 5,
        seed: 42,
    };

    let t0 = Instant::now();
    let spec = barabasi_albert(&params);
    let build_ms = t0.elapsed().as_millis();

    let anchor = Uuid::from_u128(0);
    let total = spec.all_nodes().len();
    let edges = spec.active_edge_count();

    let t1 = Instant::now();
    // Distances only — diversity sample=0 skips Edmonds-Karp
    let analysis = analyze_graph_sampled(&spec, anchor, 0);
    let dijkstra_ms = t1.elapsed().as_millis();

    let reachable = analysis.reachable_fraction(total);
    let dist_stats = analysis.distance_stats();

    println!("=== scale_distance_distribution_100k ===");
    println!("Total nodes: {total}, edges: {edges}");
    println!("Build: {build_ms}ms, Dijkstra: {dijkstra_ms}ms");
    println!("Reachable fraction: {reachable:.4}");
    println!("Distance: {dist_stats}");

    // Test passing proves we didn't OOM
    assert!(
        reachable > 0.95,
        "reachable fraction should be >0.95, got {reachable:.4}"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Sybil mesh — small case
// ---------------------------------------------------------------------------

/// 1k base + 10 Sybil nodes connected via 2 bridges.
/// Every Sybil node's diversity must be <= bridge_count (2).
#[test]
fn scale_sybil_mesh_small() {
    let bridge_count = 2usize;
    let base_params = ScaleGraphParams {
        node_count: 1000,
        m: 3,
        seed_size: 5,
        seed: 42,
    };
    let mut spec = barabasi_albert(&base_params);
    let anchor = Uuid::from_u128(0);

    // Pick bridge targets: first bridge_count high-degree nodes
    let high_degree = find_high_degree_nodes(&spec, bridge_count + 5);
    let bridge_targets: Vec<Uuid> = high_degree.into_iter().take(bridge_count).collect();

    let sybil_params = SybilMeshParams {
        sybil_count: 10,
        bridge_count,
        bridge_weight: 0.8,
        internal_weight: 1.0,
        seed: 99,
    };
    let sybil_ids = attach_sybil_mesh(&mut spec, &bridge_targets, &sybil_params);

    println!("=== scale_sybil_mesh_small ===");
    println!("Base nodes: 1000, Sybils: 10, Bridges: {bridge_count}");

    // Compute diversity directly for sybil nodes (sampled analysis won't include them
    // because sybil UUIDs sort after all BA nodes)
    let sybil_divs = compute_diversity(&spec, anchor, &sybil_ids);

    assert!(
        !sybil_divs.is_empty(),
        "should compute diversity for at least some sybil nodes"
    );

    for &sybil_id in &sybil_ids {
        if let Some(&div) = sybil_divs.get(&sybil_id) {
            println!("  sybil diversity={div}");
            assert!(
                div <= bridge_count as i32,
                "sybil node diversity {div} should be <= bridge_count {bridge_count}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Test 5: Sybil mesh — multi-bridge sweep
// ---------------------------------------------------------------------------

/// 1k base + 50 Sybils. Sweep bridge_count in [1, 2, 3, 5].
/// Hard assert only for bridge_count==1: max sybil diversity <= 1.
#[test]
fn scale_sybil_mesh_multi_bridge() {
    let base_params = ScaleGraphParams {
        node_count: 1000,
        m: 3,
        seed_size: 5,
        seed: 42,
    };
    let sybil_count = 50usize;

    println!("=== scale_sybil_mesh_multi_bridge ===");
    println!(
        "{:<14} {:<14} {:<14}",
        "bridge_count", "max_sybil_div", "mean_sybil_div"
    );

    for &bridge_count in &[1usize, 2, 3, 5] {
        let mut spec = barabasi_albert(&base_params);
        let anchor = Uuid::from_u128(0);

        let high_degree = find_high_degree_nodes(&spec, bridge_count + 10);
        let bridge_targets: Vec<Uuid> = high_degree.into_iter().take(bridge_count).collect();

        let sybil_params = SybilMeshParams {
            sybil_count,
            bridge_count,
            bridge_weight: 0.8,
            internal_weight: 1.0,
            seed: 99,
        };
        let sybil_ids = attach_sybil_mesh(&mut spec, &bridge_targets, &sybil_params);

        // Compute diversity directly for sybil nodes (sampled analysis won't include
        // them because sybil UUIDs sort after all BA nodes)
        let sybil_div_map = compute_diversity(&spec, anchor, &sybil_ids);

        let sybil_divs: Vec<i32> = sybil_ids
            .iter()
            .filter_map(|id| sybil_div_map.get(id).copied())
            .collect();

        let max_div = sybil_divs.iter().copied().max().unwrap_or(0);
        let mean_div = if sybil_divs.is_empty() {
            0.0
        } else {
            sybil_divs.iter().sum::<i32>() as f64 / sybil_divs.len() as f64
        };

        println!("{:<14} {:<14} {:<14.2}", bridge_count, max_div, mean_div);

        // Hard assert only for bridge_count == 1
        if bridge_count == 1 {
            assert!(
                max_div <= 1,
                "with 1 bridge, max sybil diversity should be <=1, got {max_div}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Test 6: Correlated expiry / time decay
// ---------------------------------------------------------------------------

/// 1k nodes. A cohort of 100 nodes has intra-cohort edges marked 730 days old.
/// Step decay: weight * 1.0 if <365d, weight * 0.5 if <730d, 0.0 if >=730d.
/// Min weight = 0.01. Checks that some nodes have increased distance and
/// overall reachability stays > 0.70.
#[test]
fn scale_correlated_expiry() {
    let params = ScaleGraphParams {
        node_count: 1000,
        m: 3,
        seed_size: 5,
        seed: 42,
    };
    let spec = barabasi_albert(&params);
    let anchor = Uuid::from_u128(0);
    let total = spec.all_nodes().len();

    // Baseline analysis before decay
    let baseline = analyze_graph_sampled(&spec, anchor, 0);
    let baseline_reachable = baseline.reachable_fraction(total);
    let baseline_dist = baseline.distance_stats();

    println!("=== scale_correlated_expiry ===");
    println!("Baseline: reachable={baseline_reachable:.4} dist={baseline_dist}");

    // Select cohort: first 100 non-anchor nodes (by UUID order in nodes list)
    let cohort_nodes: Vec<Uuid> = spec
        .all_nodes()
        .iter()
        .filter(|n| n.id != anchor)
        .take(100)
        .map(|n| n.id)
        .collect();

    // Stamp intra-cohort edges 730 days ago
    let now = chrono::Utc::now();
    let cohort_age = chrono::Duration::days(730);
    let cohort_ts = now - cohort_age;

    let mut decayed_spec = spec.clone();
    mark_cohort_edges(&mut decayed_spec, &cohort_nodes, cohort_ts);

    // Step decay: 1.0 if <365d, 0.5 if <730d, 0.0 if >=730d
    let step_decay = |age: chrono::Duration| -> f32 {
        let days = age.num_days();
        if days < 365 {
            1.0
        } else if days < 730 {
            0.5
        } else {
            0.0
        }
    };

    let decayed = decayed_spec.with_decay(now, step_decay, 0.01);

    // Post-decay analysis
    let post = analyze_graph_sampled(&decayed, anchor, 0);
    let post_reachable = post.reachable_fraction(total);
    let post_dist = post.distance_stats();

    println!("Post-decay: reachable={post_reachable:.4} dist={post_dist}");

    // After decay, cohort edges are effectively zero-weight → clamped to min_weight=0.01
    // This should increase distances for some nodes
    assert!(
        post_dist.mean >= baseline_dist.mean,
        "mean distance should not decrease after decay: before={:.3} after={:.3}",
        baseline_dist.mean,
        post_dist.mean
    );

    assert!(
        post_reachable > 0.70,
        "reachable fraction should stay >0.70 after decay, got {post_reachable:.4}"
    );
}

// ---------------------------------------------------------------------------
// Test 7: Bridge removal
// ---------------------------------------------------------------------------

/// Find top-3 high-degree nodes, remove one at a time.
/// After 1st removal: reachable > 0.80. After 3rd removal: reachable > 0.20.
#[test]
fn scale_bridge_removal() {
    let params = ScaleGraphParams {
        node_count: 1000,
        m: 3,
        seed_size: 5,
        seed: 42,
    };
    let mut spec = barabasi_albert(&params);
    let anchor = Uuid::from_u128(0);
    let total = spec.all_nodes().len();

    let top3 = find_high_degree_nodes(&spec, 3);

    println!("=== scale_bridge_removal ===");
    println!("Total nodes: {total}");
    println!(
        "Top-3 high-degree hubs: {:?}",
        top3.iter().map(|id| id.to_string()).collect::<Vec<_>>()
    );

    // Baseline
    let baseline = analyze_graph_sampled(&spec, anchor, 0);
    println!(
        "Baseline reachable: {:.4}",
        baseline.reachable_fraction(total)
    );

    for (i, &hub) in top3.iter().enumerate() {
        remove_node_edges(&mut spec, hub);
        let analysis = analyze_graph_sampled(&spec, anchor, 0);
        let reachable = analysis.reachable_fraction(total);
        println!("After removing hub #{}: reachable={reachable:.4}", i + 1);

        match i {
            0 => assert!(
                reachable > 0.80,
                "after 1st removal, reachable should be >0.80, got {reachable:.4}"
            ),
            2 => assert!(
                reachable > 0.20,
                "after 3rd removal, reachable should be >0.20, got {reachable:.4}"
            ),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Test 8: Performance benchmark (ignored by default)
// ---------------------------------------------------------------------------

/// Sweep [100, 500, 1000, 5000, 10000] nodes and print a timing table.
/// Ignored by default — run explicitly with `--ignored`.
#[test]
#[ignore]
fn scale_performance_benchmark() {
    use std::time::Instant;

    let node_counts = [100usize, 500, 1000, 5000, 10000];

    println!("=== scale_performance_benchmark ===");
    println!(
        "{:<10} {:<12} {:<16} {:<16} {:<16}",
        "nodes", "edges", "build_ms", "dijkstra_ms", "total_ms"
    );

    for &n in &node_counts {
        let params = ScaleGraphParams {
            node_count: n,
            m: 3,
            seed_size: 5,
            seed: 42,
        };

        let t0 = Instant::now();
        let spec = barabasi_albert(&params);
        let build_ms = t0.elapsed().as_millis();

        let edges = spec.active_edge_count();
        let anchor = Uuid::from_u128(0);

        let t1 = Instant::now();
        // Distances only for timing (no Edmonds-Karp at large scale)
        let analysis = analyze_graph_sampled(&spec, anchor, 0);
        let dijkstra_ms = t1.elapsed().as_millis();

        let total_ms = t0.elapsed().as_millis();
        let reachable = analysis.reachable_fraction(n);

        println!(
            "{:<10} {:<12} {:<16} {:<16} {:<16}  reachable={:.3}",
            n, edges, build_ms, dijkstra_ms, total_ms, reachable
        );
    }
}
