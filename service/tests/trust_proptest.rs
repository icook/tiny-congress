//! Property-based tests for trust graph behavioral invariants.
//!
//! Uses proptest to generate random graph topologies and verify that
//! behavioral predicates hold across all generated cases.
//!
//! Each test materializes a `GeneratedGraph` into a real isolated database,
//! runs the trust engine, and checks the predicate. Case counts are kept
//! low (10) because each case hits a real database.
//!
//! Run with: `cargo test --test trust_proptest`

mod common;

use common::simulation::generators::{self, GraphParams};
use common::simulation::predicates::{self, PredicateResult};
use common::simulation::report::SimulationReport;
use common::simulation::{GraphBuilder, Team};
use common::test_db::{isolated_db, run_test};
use proptest::prelude::*;

/// Materialized graph with its backing database.
///
/// Holds the `IsolatedDb` to prevent its `Drop` from terminating connections
/// while the `GraphBuilder` (which clones the pool) is still in use.
struct MaterializedGraph {
    #[allow(dead_code)] // held for Drop — prevents pg_terminate_backend during queries
    db: common::test_db::IsolatedDb,
    builder: GraphBuilder,
    anchor_id: uuid::Uuid,
}

async fn materialize_graph(gen: &generators::GeneratedGraph) -> MaterializedGraph {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let mut node_db_ids: Vec<uuid::Uuid> = Vec::with_capacity(gen.node_count());
    for node in &gen.nodes {
        let id = g.add_node(&node.name, node.team).await;
        node_db_ids.push(id);
    }

    for edge in &gen.edges {
        let from_id = node_db_ids[edge.from_idx];
        let to_id = node_db_ids[edge.to_idx];
        g.endorse(from_id, to_id, edge.weight).await;
    }

    let anchor_id = node_db_ids[0];
    MaterializedGraph {
        db,
        builder: g,
        anchor_id,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// Nodes with exactly one active endorser must have diversity ≤ 1.
    #[test]
    fn prop_single_attachment_implies_low_diversity(
        gen in generators::default_graph()
    ) {
        let result: PredicateResult = run_test(async {
            let mg = materialize_graph(&gen).await;
            let report = SimulationReport::run(&mg.builder, mg.anchor_id).await;
            predicates::single_attachment_implies_low_diversity(mg.builder.spec(), &report)
        });
        prop_assert!(result.holds, "{}: {}", result.name, result.explanation);
    }

    /// Nodes with a finite distance must be reachable via active edges.
    #[test]
    fn prop_reachable_nodes_have_active_path(
        gen in generators::default_graph()
    ) {
        let result: PredicateResult = run_test(async {
            let mg = materialize_graph(&gen).await;
            let report = SimulationReport::run(&mg.builder, mg.anchor_id).await;
            predicates::unreachable_nodes_have_no_distance(mg.builder.spec(), &report)
        });
        prop_assert!(result.holds, "{}: {}", result.name, result.explanation);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// A fully-connected red cluster attached through one bridge edge
    /// should have diversity ≤ 1 for all members.
    #[test]
    fn prop_isolated_red_cluster_diversity_bounded(
        gen in generators::graph_strategy(GraphParams {
            min_nodes: 4,
            max_nodes: 10,
            min_density: 0.1,
            max_density: 0.4,
            red_fraction: 0.0, // base graph is all blue
            min_weight: 0.5,
            max_weight: 1.0,
        }),
        cluster_size in 2usize..=4,
    ) {
        let result: PredicateResult = run_test(async {
            let db = isolated_db().await;
            let mut g = GraphBuilder::new(db.pool().clone());

            // Materialize base spec (all-blue graph)
            let mut node_db_ids: Vec<uuid::Uuid> = Vec::with_capacity(gen.node_count());
            for node in &gen.nodes {
                let id = g.add_node(&node.name, node.team).await;
                node_db_ids.push(id);
            }
            for edge in &gen.edges {
                let from_id = node_db_ids[edge.from_idx];
                let to_id = node_db_ids[edge.to_idx];
                g.endorse(from_id, to_id, edge.weight).await;
            }

            let anchor_id = node_db_ids[0];

            // Add fully-connected red cluster
            let mut cluster_ids: Vec<uuid::Uuid> = Vec::with_capacity(cluster_size);
            for i in 0..cluster_size {
                let id = g.add_node(&format!("red_cluster_{i}"), Team::Red).await;
                cluster_ids.push(id);
            }
            for i in 0..cluster_size {
                for j in 0..cluster_size {
                    if i != j {
                        g.endorse(cluster_ids[i], cluster_ids[j], 1.0).await;
                    }
                }
            }
            // Single bridge: anchor → first cluster node
            g.endorse(anchor_id, cluster_ids[0], 0.5).await;

            let report = SimulationReport::run(&g, anchor_id).await;
            predicates::isolated_cluster_diversity_bounded(g.spec(), &report, &cluster_ids)
        });
        prop_assert!(result.holds, "{}: {}", result.name, result.explanation);
    }
}
