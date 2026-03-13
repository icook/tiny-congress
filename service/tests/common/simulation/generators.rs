//! Random graph topology generators for property-based testing.
//!
//! Uses proptest strategies to generate [`GeneratedGraph`] instances with
//! configurable parameters. `GeneratedGraph` uses index-based edges
//! (suitable for proptest shrinking) and is materialized into a
//! [`GraphBuilder`](super::GraphBuilder) for engine testing.

use proptest::prelude::*;
use uuid::Uuid;

use super::Team;

/// A node in a generated graph (index-based, no DB state).
#[derive(Debug, Clone)]
pub struct GeneratedNode {
    pub id: Uuid,
    pub name: String,
    pub team: Team,
}

/// An edge in a generated graph (index-based, no DB state).
#[derive(Debug, Clone)]
pub struct GeneratedEdge {
    pub from_idx: usize,
    pub to_idx: usize,
    pub weight: f32,
}

/// A generated graph topology for property-based testing.
///
/// Uses index-based edges (not UUIDs) so proptest can shrink effectively.
/// Materialized into a `GraphBuilder` via [`materialize`].
#[derive(Debug, Clone)]
pub struct GeneratedGraph {
    pub nodes: Vec<GeneratedNode>,
    pub edges: Vec<GeneratedEdge>,
}

impl GeneratedGraph {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

/// Parameters for generating random graphs.
#[derive(Debug, Clone)]
pub struct GraphParams {
    pub min_nodes: usize,
    pub max_nodes: usize,
    pub min_density: f64,
    pub max_density: f64,
    pub red_fraction: f64,
    pub min_weight: f32,
    pub max_weight: f32,
}

impl Default for GraphParams {
    fn default() -> Self {
        Self {
            min_nodes: 3,
            max_nodes: 15,
            min_density: 0.1,
            max_density: 0.5,
            red_fraction: 0.3,
            min_weight: 0.1,
            max_weight: 1.0,
        }
    }
}

/// Build a proptest strategy that generates random graphs.
///
/// Node 0 is always "anchor" (Blue) with at least one outbound edge.
/// UUIDs are deterministic (`Uuid::from_u128(index)`) for reproducibility.
pub fn graph_strategy(params: GraphParams) -> impl Strategy<Value = GeneratedGraph> {
    let min_n = params.min_nodes;
    let max_n = params.max_nodes;
    let min_d = params.min_density;
    let max_d = params.max_density;
    let red_frac = params.red_fraction;
    let min_w = params.min_weight;
    let max_w = params.max_weight;

    (min_n..=max_n).prop_flat_map(move |node_count| {
        let min_d2 = min_d;
        let max_d2 = max_d;
        let min_w2 = min_w;
        let max_w2 = max_w;

        (min_d2..=max_d2, any::<u64>()).prop_flat_map(move |(density, seed)| {
            let mut nodes = Vec::with_capacity(node_count);

            // Node 0 is always anchor (Blue)
            nodes.push(GeneratedNode {
                id: Uuid::from_u128(0u128),
                name: "anchor".to_string(),
                team: Team::Blue,
            });

            let non_anchor = node_count - 1;
            let blue_count = ((non_anchor as f64) * (1.0 - red_frac)).round() as usize;

            for i in 1..node_count {
                let team = if i - 1 < blue_count {
                    Team::Blue
                } else {
                    Team::Red
                };
                nodes.push(GeneratedNode {
                    id: Uuid::from_u128(i as u128),
                    name: format!("node_{i}"),
                    team,
                });
            }

            let possible_edges = node_count * (node_count - 1);
            proptest::collection::vec(min_w2..=max_w2, possible_edges).prop_map(move |weights| {
                let mut edges = Vec::new();
                let mut weight_idx = 0usize;

                for i in 0..node_count {
                    for j in 0..node_count {
                        if i == j {
                            continue;
                        }
                        let hash_input = seed
                            .wrapping_add((i as u64).wrapping_mul(7919))
                            .wrapping_add((j as u64).wrapping_mul(6271));
                        #[allow(clippy::cast_precision_loss)]
                        let hash_val = (hash_input % 10_000) as f64 / 10_000.0;

                        if hash_val < density {
                            edges.push(GeneratedEdge {
                                from_idx: i,
                                to_idx: j,
                                weight: weights[weight_idx],
                            });
                        }
                        weight_idx += 1;
                    }
                }

                // Ensure anchor has at least one outbound edge
                let anchor_has_out = edges.iter().any(|e| e.from_idx == 0);
                if !anchor_has_out && node_count > 1 {
                    edges.push(GeneratedEdge {
                        from_idx: 0,
                        to_idx: 1,
                        weight: weights[0],
                    });
                }

                GeneratedGraph {
                    nodes: nodes.clone(),
                    edges,
                }
            })
        })
    })
}

/// Default strategy: 3–15 nodes, 10–50% density, 30% red.
pub fn default_graph() -> impl Strategy<Value = GeneratedGraph> {
    graph_strategy(GraphParams::default())
}
