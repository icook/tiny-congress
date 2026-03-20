//! Denouncement mechanism comparison framework.
//!
//! Captures before/after metrics for each scenario × mechanism combination
//! and prints a scored comparison table.

use std::fmt;
use std::fs;
use std::path::Path;

/// Before/after metrics for a single scenario × mechanism test.
#[derive(Debug, Clone)]
pub struct MechanismComparison {
    pub scenario: String,
    pub mechanism: String,
    pub target_name: String,
    /// Before denouncement
    pub before_distance: Option<f32>,
    pub before_diversity: i32,
    pub before_eligible: bool,
    /// After denouncement
    pub after_distance: Option<f32>,
    pub after_diversity: i32,
    pub after_eligible: bool,
    /// Collateral: blue nodes that lost eligibility
    pub blue_casualties: usize,
    pub blue_total: usize,
    /// Weaponization: did blue target survive Sybil mass-denouncement?
    /// None if not a weaponization test.
    pub survived_weaponization: Option<bool>,
}

impl MechanismComparison {
    /// Did the mechanism successfully remove the target's access?
    pub fn target_lost_access(&self) -> bool {
        self.before_eligible && !self.after_eligible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_comparison(before_eligible: bool, after_eligible: bool) -> MechanismComparison {
        MechanismComparison {
            scenario: "test-scenario".to_string(),
            mechanism: "test-mechanism".to_string(),
            target_name: "target".to_string(),
            before_distance: Some(1.0),
            before_diversity: 2,
            before_eligible,
            after_distance: None,
            after_diversity: 0,
            after_eligible,
            blue_casualties: 0,
            blue_total: 5,
            survived_weaponization: None,
        }
    }

    #[test]
    fn target_lost_access_returns_true_when_eligible_before_and_not_after() {
        let c = make_comparison(true, false);
        assert!(
            c.target_lost_access(),
            "eligible before, ineligible after = access lost"
        );
    }

    #[test]
    fn target_lost_access_returns_false_when_never_eligible() {
        let c = make_comparison(false, false);
        assert!(
            !c.target_lost_access(),
            "ineligible before AND after = did not lose access (never had it)"
        );
    }

    #[test]
    fn target_lost_access_returns_false_when_still_eligible_after() {
        let c = make_comparison(true, true);
        assert!(
            !c.target_lost_access(),
            "eligible before AND after = mechanism failed to remove access"
        );
    }

    #[test]
    fn target_lost_access_returns_false_when_gained_eligibility() {
        // Unusual case: wasn't eligible before, is now. Not a "lost access" event.
        let c = make_comparison(false, true);
        assert!(
            !c.target_lost_access(),
            "ineligible before, eligible after = gained access, not lost"
        );
    }
}

/// Collects comparison rows and prints a summary table.
pub struct ComparisonTable {
    pub rows: Vec<MechanismComparison>,
}

impl ComparisonTable {
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    pub fn add(&mut self, row: MechanismComparison) {
        self.rows.push(row);
    }

    /// Write the comparison table to a file.
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{self}"))
    }
}

impl fmt::Display for ComparisonTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<28} {:<24} {:<8} {:<8} {:<16} {:<16} {:<12}",
            "Scenario",
            "Mechanism",
            "d_before",
            "d_after",
            "div_before→after",
            "Target lost?",
            "Blue casualties"
        )?;
        writeln!(f, "{}", "─".repeat(112))?;
        for row in &self.rows {
            let d_before = row
                .before_distance
                .map_or("—".to_string(), |d| format!("{d:.2}"));
            let d_after = row
                .after_distance
                .map_or("—".to_string(), |d| format!("{d:.2}"));
            let div_change = format!("{}→{}", row.before_diversity, row.after_diversity);
            let lost = if row.target_lost_access() {
                "YES"
            } else {
                "no"
            };
            let casualties = format!("{}/{}", row.blue_casualties, row.blue_total);
            writeln!(
                f,
                "{:<28} {:<24} {:<8} {:<8} {:<16} {:<16} {:<12}",
                row.scenario, row.mechanism, d_before, d_after, div_change, lost, casualties
            )?;
        }
        // Weaponization summary
        let weapon_rows: Vec<_> = self
            .rows
            .iter()
            .filter(|r| r.survived_weaponization.is_some())
            .collect();
        if !weapon_rows.is_empty() {
            writeln!(
                f,
                "\n{:<28} {:<24} {:<16}",
                "Scenario", "Mechanism", "Blue survived?"
            )?;
            writeln!(f, "{}", "─".repeat(68))?;
            for row in weapon_rows {
                let survived = if row.survived_weaponization.unwrap_or(false) {
                    "YES"
                } else {
                    "no"
                };
                writeln!(
                    f,
                    "{:<28} {:<24} {:<16}",
                    row.scenario, row.mechanism, survived
                )?;
            }
        }
        Ok(())
    }
}
