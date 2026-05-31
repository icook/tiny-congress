/// Hybrid uncertainty + near-frontier pair selection.
use uuid::Uuid;

/// A submission with its current rating info, used for pair selection.
#[derive(Debug, Clone)]
pub struct RatedSubmission {
    pub submission_id: Uuid,
    pub author_id: Uuid,
    pub rating: f64,
    pub deviation: f64,
}

const UNCERTAINTY_WEIGHT: f64 = 0.7;
const PROXIMITY_WEIGHT: f64 = 0.3;

/// Select the best pair for a ranker to judge.
///
/// - `ratings`: all submissions with their current ratings
/// - `judged_pairs`: pairs this ranker has already judged, as ordered (min, max) UUID tuples
/// - `ranker_id`: the ranker's account ID (to exclude their own submission)
///
/// Returns the pair `(submission_a_id, submission_b_id)` with the highest score,
/// or `None` if no unjudged pairs are available.
#[must_use]
pub fn select_pair(
    ratings: &[RatedSubmission],
    judged_pairs: &[(Uuid, Uuid)],
    ranker_id: Uuid,
) -> Option<(Uuid, Uuid)> {
    // Step 1: filter out the ranker's own submissions
    let eligible: Vec<&RatedSubmission> = ratings
        .iter()
        .filter(|s| s.author_id != ranker_id)
        .collect();

    if eligible.len() < 2 {
        return None;
    }

    // Build a fast lookup set for already-judged pairs
    let judged_set: std::collections::HashSet<(Uuid, Uuid)> =
        judged_pairs.iter().copied().collect();

    let mut best_score = f64::NEG_INFINITY;
    let mut best_pair: Option<(Uuid, Uuid)> = None;

    // Step 2 & 3: iterate all candidate pairs, skip already-judged
    for i in 0..eligible.len() {
        for j in (i + 1)..eligible.len() {
            let a = eligible[i];
            let b = eligible[j];

            // Normalise to (min, max) order for lookup
            let key = (
                a.submission_id.min(b.submission_id),
                a.submission_id.max(b.submission_id),
            );
            if judged_set.contains(&key) {
                continue;
            }

            // Step 4: score = uncertainty component + proximity component
            let uncertainty = (a.deviation + b.deviation) * UNCERTAINTY_WEIGHT;
            let proximity = (1.0 / (1.0 + (a.rating - b.rating).abs())) * PROXIMITY_WEIGHT;
            let score = uncertainty + proximity;

            if score > best_score {
                best_score = score;
                best_pair = Some((a.submission_id, b.submission_id));
            }
        }
    }

    best_pair
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rated(id: Uuid, author: Uuid, rating: f64, deviation: f64) -> RatedSubmission {
        RatedSubmission {
            submission_id: id,
            author_id: author,
            rating,
            deviation,
        }
    }

    #[test]
    fn test_no_submissions_returns_none() {
        assert!(select_pair(&[], &[], Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_single_submission_returns_none() {
        let s = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 350.0);
        assert!(select_pair(&[s], &[], Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_two_submissions_returns_pair() {
        let a = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 350.0);
        let b = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 350.0);
        assert!(select_pair(&[a, b], &[], Uuid::new_v4()).is_some());
    }

    #[test]
    fn test_excludes_already_judged() {
        let a_id = Uuid::new_v4();
        let b_id = Uuid::new_v4();
        let a = make_rated(a_id, Uuid::new_v4(), 1500.0, 350.0);
        let b = make_rated(b_id, Uuid::new_v4(), 1500.0, 350.0);
        let judged = vec![(a_id.min(b_id), a_id.max(b_id))];
        assert!(select_pair(&[a, b], &judged, Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_excludes_rankers_own_submission() {
        let ranker = Uuid::new_v4();
        let other_author = Uuid::new_v4();
        let a = make_rated(Uuid::new_v4(), ranker, 1500.0, 350.0); // ranker's submission
        let b = make_rated(Uuid::new_v4(), other_author, 1500.0, 350.0);
        // Only 2 submissions, one is ranker's — can't form a valid pair without ranker's
        assert!(select_pair(&[a, b], &[], ranker).is_none());
    }

    #[test]
    fn test_three_submissions_ranker_excluded_still_works() {
        let ranker = Uuid::new_v4();
        let a = make_rated(Uuid::new_v4(), ranker, 1500.0, 350.0); // ranker's
        let b = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 350.0);
        let c = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 350.0);
        // b and c should still be a valid pair
        let result = select_pair(&[a, b.clone(), c.clone()], &[], ranker);
        assert!(result.is_some());
        let (x, y) = result.unwrap();
        assert!(x == b.submission_id || x == c.submission_id);
        assert!(y == b.submission_id || y == c.submission_id);
        assert_ne!(x, y);
    }

    #[test]
    fn test_prefers_high_uncertainty() {
        let a = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 350.0); // high dev
        let b = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 350.0); // high dev
        let c = make_rated(Uuid::new_v4(), Uuid::new_v4(), 1500.0, 50.0); // low dev
        let result = select_pair(&[a.clone(), b.clone(), c], &[], Uuid::new_v4()).unwrap();
        // Should prefer (a, b) since both have high deviation
        let ids = [result.0, result.1];
        assert!(ids.contains(&a.submission_id) && ids.contains(&b.submission_id));
    }
}
