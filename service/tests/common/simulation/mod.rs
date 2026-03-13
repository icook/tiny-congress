//! Trust graph simulation harness for Sybil resistance testing.
//!
//! Constructs adversarial graph topologies with red/blue team designations
//! and runs the real `TrustEngine` against them.

pub mod report;
pub mod topology;

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::common::factories::{insert_endorsement, insert_revoked_endorsement, AccountFactory};

/// Team designation for simulation nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    /// Legitimate users — expected to pass trust thresholds.
    Blue,
    /// Adversarial users — expected to be blocked by trust mechanisms.
    Red,
}

impl std::fmt::Display for Team {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blue => write!(f, "Blue"),
            Self::Red => write!(f, "Red"),
        }
    }
}

/// A node in the simulation graph with team metadata.
#[derive(Debug, Clone)]
pub struct SimNode {
    pub id: Uuid,
    pub name: String,
    pub team: Team,
}

/// A directed edge in the simulation graph.
#[derive(Debug, Clone)]
pub struct SimEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub weight: f32,
    pub revoked: bool,
}

/// Programmatic graph topology constructor for simulation tests.
pub struct GraphBuilder {
    pool: PgPool,
    nodes: Vec<SimNode>,
    edges: Vec<SimEdge>,
    name_to_id: HashMap<String, Uuid>,
}

impl GraphBuilder {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            nodes: Vec::new(),
            edges: Vec::new(),
            name_to_id: HashMap::new(),
        }
    }

    pub async fn add_node(&mut self, name: &str, team: Team) -> Uuid {
        let account = AccountFactory::new()
            .with_username(name)
            .create(&self.pool)
            .await
            .unwrap_or_else(|e| panic!("Failed to create account '{name}': {e}"));

        let node = SimNode {
            id: account.id,
            name: name.to_string(),
            team,
        };
        self.nodes.push(node);
        self.name_to_id.insert(name.to_string(), account.id);
        account.id
    }

    pub async fn endorse(&mut self, from: Uuid, to: Uuid, weight: f32) {
        insert_endorsement(&self.pool, from, to, weight).await;
        self.edges.push(SimEdge {
            from,
            to,
            weight,
            revoked: false,
        });
    }

    pub async fn endorse_revoked(&mut self, from: Uuid, to: Uuid, weight: f32) {
        insert_revoked_endorsement(&self.pool, from, to, weight).await;
        self.edges.push(SimEdge {
            from,
            to,
            weight,
            revoked: true,
        });
    }

    /// Revoke an existing endorsement (set revoked_at = now()).
    ///
    /// Panics if no active endorsement exists from→to.
    pub async fn revoke(&mut self, from: Uuid, to: Uuid) {
        let result = sqlx::query(
            "UPDATE reputation__endorsements SET revoked_at = now() \
             WHERE endorser_id = $1 AND subject_id = $2 AND topic = 'trust' AND revoked_at IS NULL",
        )
        .bind(from)
        .bind(to)
        .execute(&self.pool)
        .await
        .expect("revoke endorsement query failed");
        assert_eq!(
            result.rows_affected(),
            1,
            "expected to revoke exactly 1 endorsement from {} to {}",
            from,
            to
        );
        // Mark in local edge list
        if let Some(edge) = self
            .edges
            .iter_mut()
            .find(|e| e.from == from && e.to == to && !e.revoked)
        {
            edge.revoked = true;
        }
    }

    pub fn node(&self, name: &str) -> Uuid {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("No node named '{name}'"))
    }

    pub fn nodes_by_team(&self, team: Team) -> Vec<Uuid> {
        self.nodes
            .iter()
            .filter(|n| n.team == team)
            .map(|n| n.id)
            .collect()
    }

    pub fn all_nodes(&self) -> &[SimNode] {
        &self.nodes
    }

    pub fn all_edges(&self) -> &[SimEdge] {
        &self.edges
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn node_name(&self, id: Uuid) -> &str {
        self.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.name.as_str())
            .unwrap_or("unknown")
    }
}
