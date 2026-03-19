//! Simulation report — runs the trust engine and formats results.

use std::collections::HashMap;
use std::fmt;
use std::fmt::Write as _;
use std::io;
use std::path::Path;

use std::sync::Arc;

use sqlx::PgPool;
use tinycongress_api::trust::constraints::{Eligibility, RoomConstraint};
use tinycongress_api::trust::engine::TrustEngine;
use tinycongress_api::trust::graph_reader::TrustRepoGraphReader;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
use uuid::Uuid;

use super::{GraphBuilder, Team};

/// Score data for a single node in the simulation.
#[derive(Debug, Clone)]
pub struct NodeScore {
    pub id: Uuid,
    pub name: String,
    pub team: Team,
    pub distance: Option<f32>,
    pub diversity: i32,
}

/// Results of running the trust engine on a simulation topology.
pub struct SimulationReport {
    pub anchor_id: Uuid,
    pub scores: Vec<NodeScore>,
}

impl SimulationReport {
    /// Run the trust engine from the given anchor and collect results.
    pub async fn run(g: &GraphBuilder, anchor_id: Uuid) -> Self {
        let engine = TrustEngine::new(g.pool().clone());

        let distances = engine
            .compute_distances_from(anchor_id)
            .await
            .expect("compute_distances_from failed");

        let diversities: HashMap<Uuid, i32> = engine
            .compute_diversity_from(anchor_id)
            .await
            .expect("compute_diversity_from failed")
            .into_iter()
            .collect();

        let distance_map: HashMap<Uuid, Option<f32>> = distances
            .iter()
            .map(|s| (s.user_id, s.trust_distance))
            .collect();

        let scores = g
            .all_nodes()
            .iter()
            .map(|node| NodeScore {
                id: node.id,
                name: node.name.clone(),
                team: node.team,
                distance: distance_map.get(&node.id).copied().flatten(),
                diversity: diversities.get(&node.id).copied().unwrap_or(0),
            })
            .collect();

        Self { anchor_id, scores }
    }

    /// Write computed scores to `trust__score_snapshots` via `recompute_from_anchor`.
    ///
    /// Must be called before `check_eligibility`. Separated from `run()` to keep
    /// the default path side-effect-free.
    pub async fn materialize(&self, pool: &PgPool) {
        let engine = TrustEngine::new(pool.clone());
        let repo = PgTrustRepo::new(pool.clone());
        engine
            .recompute_from_anchor(self.anchor_id, &repo)
            .await
            .expect("recompute_from_anchor failed during materialize");
    }

    /// Check a node's eligibility against a room constraint.
    ///
    /// Requires `materialize()` to have been called first (reads from snapshot table).
    pub async fn check_eligibility(
        &self,
        node_id: Uuid,
        constraint: &dyn RoomConstraint,
        pool: &PgPool,
    ) -> Eligibility {
        let repo = PgTrustRepo::new(pool.clone());
        let reader = TrustRepoGraphReader::new(Arc::new(repo));
        constraint
            .check(node_id, &reader)
            .await
            .expect("constraint check failed")
    }

    /// Refresh in-memory scores from the `trust__score_snapshots` table.
    ///
    /// Call this after any direct DB mutations to the snapshot (e.g., score
    /// penalty updates) so that `distance()` and `diversity()` reflect the
    /// actual snapshot state used by `check_eligibility`.
    pub async fn refresh_from_snapshot(&mut self, pool: &PgPool) {
        let rows: Vec<(Uuid, Option<f32>, Option<i32>)> = sqlx::query_as(
            "SELECT user_id, trust_distance, path_diversity \
             FROM trust__score_snapshots \
             WHERE context_user_id = $1",
        )
        .bind(self.anchor_id)
        .fetch_all(pool)
        .await
        .expect("refresh_from_snapshot query failed");

        let snapshot: HashMap<Uuid, (Option<f32>, i32)> = rows
            .into_iter()
            .map(|(uid, dist, div)| (uid, (dist, div.unwrap_or(0))))
            .collect();

        for score in &mut self.scores {
            if let Some(&(dist, div)) = snapshot.get(&score.id) {
                score.distance = dist;
                score.diversity = div;
            }
        }
    }

    /// Get distance for a specific node.
    pub fn distance(&self, node_id: Uuid) -> Option<f32> {
        self.scores.iter().find(|s| s.id == node_id)?.distance
    }

    /// Get diversity for a specific node.
    pub fn diversity(&self, node_id: Uuid) -> i32 {
        self.scores
            .iter()
            .find(|s| s.id == node_id)
            .map_or(0, |s| s.diversity)
    }

    /// Get all red team node scores.
    pub fn red_nodes(&self) -> Vec<&NodeScore> {
        self.scores.iter().filter(|s| s.team == Team::Red).collect()
    }

    /// Get all blue team node scores.
    pub fn blue_nodes(&self) -> Vec<&NodeScore> {
        self.scores
            .iter()
            .filter(|s| s.team == Team::Blue)
            .collect()
    }

    /// Write a DOT/Graphviz file with red/blue coloring and score annotations.
    #[allow(clippy::expect_used)]
    pub fn write_dot(&self, g: &GraphBuilder, path: &Path) -> io::Result<()> {
        use std::fs;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut dot = String::from("digraph trust_simulation {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  node [shape=box, style=filled, fontname=\"Helvetica\"];\n");
        dot.push_str("  edge [fontname=\"Helvetica\", fontsize=10];\n\n");

        // Nodes with team coloring and score labels
        for score in &self.scores {
            let color = match score.team {
                Team::Blue => "#a8d5e2",
                Team::Red => "#f4a9a8",
            };
            let dist = score
                .distance
                .map_or_else(|| "unreachable".to_string(), |d| format!("{d:.2}"));
            let label = format!("{}\\nd={} div={}", score.name, dist, score.diversity);
            writeln!(
                dot,
                "  \"{}\" [label=\"{}\", fillcolor=\"{}\"];",
                score.name, label, color
            )
            .expect("write to String is infallible");
        }

        dot.push('\n');

        // Edges with weight labels
        for edge in g.all_edges() {
            let from_name = g.node_name(edge.from);
            let to_name = g.node_name(edge.to);
            let style = if edge.revoked {
                ", style=dashed, color=gray"
            } else {
                ""
            };
            writeln!(
                dot,
                "  \"{}\" -> \"{}\" [label=\"{:.1}\"{style}];",
                from_name, to_name, edge.weight
            )
            .expect("write to String is infallible");
        }

        dot.push_str("}\n");
        fs::write(path, dot)
    }
}

impl fmt::Display for SimulationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<20} {:<6} {:>10} {:>10}",
            "Node", "Team", "Distance", "Diversity"
        )?;
        writeln!(
            f,
            "{:<20} {:<6} {:>10} {:>10}",
            "----", "----", "--------", "---------"
        )?;
        for score in &self.scores {
            let dist = score
                .distance
                .map_or_else(|| "unreachable".to_string(), |d| format!("{d:.3}"));
            writeln!(
                f,
                "{:<20} {:<6} {:>10} {:>10}",
                score.name, score.team, dist, score.diversity
            )?;
        }
        Ok(())
    }
}
