//! Simulation report — runs the trust engine and formats results.

use std::collections::HashMap;
use std::fmt;
use std::fmt::Write as _;
use std::io;
use std::path::Path;

use tinycongress_api::trust::engine::TrustEngine;
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
