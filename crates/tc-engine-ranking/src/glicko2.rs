/// Glicko-2 rating system (Mark Glickman, 2012).
///
/// Reference: <http://www.glicko.net/glicko/glicko2.pdf>
use std::f64::consts::PI;

/// System constant — controls how quickly volatility can change.
const TAU: f64 = 0.5;

/// Convergence tolerance for the Illinois root-finding iteration.
const CONVERGENCE_TOLERANCE: f64 = 0.000_001;

/// Scale factor for converting between Glicko-1 and Glicko-2 scales.
/// = 400 / ln(10)
const SCALE: f64 = 173.7178;

/// A player's rating state.
#[derive(Debug, Clone)]
pub struct Rating {
    /// Glicko-1 scale, default 1500.0
    pub rating: f64,
    /// Glicko-1 scale, default 350.0
    pub deviation: f64,
    /// Default 0.06
    pub volatility: f64,
}

impl Default for Rating {
    fn default() -> Self {
        Self {
            rating: 1500.0,
            deviation: 350.0,
            volatility: 0.06,
        }
    }
}

/// g(phi) reduction factor — reduces impact of uncertain opponents.
fn g(phi: f64) -> f64 {
    1.0 / (1.0 + 3.0 * phi * phi / (PI * PI)).sqrt()
}

/// Expected score `E(mu, mu_j, phi_j)`.
fn expected_score(mu: f64, mu_j: f64, phi_j: f64) -> f64 {
    1.0 / (1.0 + (-g(phi_j) * (mu - mu_j)).exp())
}

/// Compute new volatility using the Illinois variant of the Regula Falsi method.
#[allow(clippy::suboptimal_flops, clippy::while_float)]
fn new_volatility(phi: f64, sigma: f64, delta: f64, v: f64) -> f64 {
    let a = sigma.powi(2).ln();
    let tau2 = TAU * TAU;
    let delta2 = delta * delta;
    let phi2 = phi * phi;

    // f(x) as defined in Glickman's paper
    let f = |x: f64| -> f64 {
        let ex = x.exp();
        let denom = phi2 + v + ex;
        (ex * (delta2 - phi2 - v - ex)) / (2.0 * denom * denom) - (x - a) / tau2
    };

    // Choose initial bracket [A, B] per Glickman step 5.2
    let mut big_a = a;
    let mut big_b = if delta2 > phi2 + v {
        (delta2 - phi2 - v).ln()
    } else {
        // Find k such that f(a - k*tau) < 0
        let mut k = 1.0_f64;
        while f(a - k * TAU) < 0.0 {
            k += 1.0;
        }
        a - k * TAU
    };

    let mut f_a = f(big_a);
    let mut f_b = f(big_b);

    // Illinois iteration
    while (big_b - big_a).abs() > CONVERGENCE_TOLERANCE {
        let big_c = big_a + (big_a - big_b) * f_a / (f_b - f_a);
        let f_c = f(big_c);

        if f_c * f_b <= 0.0 {
            big_a = big_b;
            f_a = f_b;
        } else {
            // Illinois step: halve f_a to ensure convergence
            f_a /= 2.0;
        }
        big_b = big_c;
        f_b = f_c;
    }

    (f64::midpoint(big_a, big_b) / 2.0).exp()
}

/// Update both ratings in-place after a single pairwise matchup.
/// The first argument is the winner.
#[allow(clippy::suboptimal_flops, clippy::imprecise_flops)]
pub fn update_ratings(winner: &mut Rating, loser: &mut Rating) {
    // Step 1: convert to Glicko-2 scale
    let mu_w = (winner.rating - 1500.0) / SCALE;
    let phi_w = winner.deviation / SCALE;

    let mu_l = (loser.rating - 1500.0) / SCALE;
    let phi_l = loser.deviation / SCALE;

    // --- Update winner (s = 1.0 for win) ---
    {
        let g_l = g(phi_l);
        let e_w = expected_score(mu_w, mu_l, phi_l);
        let v_w = 1.0 / (g_l * g_l * e_w * (1.0 - e_w));
        let delta_w = v_w * g_l * (1.0 - e_w);

        let sigma_w_prime = new_volatility(phi_w, winner.volatility, delta_w, v_w);
        let phi_star_w = phi_w.hypot(sigma_w_prime);
        let phi_w_prime = 1.0 / (1.0 / (phi_star_w * phi_star_w) + 1.0 / v_w).sqrt();
        let mu_w_prime = mu_w + phi_w_prime * phi_w_prime * g_l * (1.0 - e_w);

        winner.rating = mu_w_prime * SCALE + 1500.0;
        winner.deviation = phi_w_prime * SCALE;
        winner.volatility = sigma_w_prime;
    }

    // --- Update loser (s = 0.0 for loss) ---
    {
        let g_w = g(phi_w);
        let e_l = expected_score(mu_l, mu_w, phi_w);
        let v_l = 1.0 / (g_w * g_w * e_l * (1.0 - e_l));
        let delta_l = v_l * g_w * (0.0 - e_l);

        let sigma_l_prime = new_volatility(phi_l, loser.volatility, delta_l, v_l);
        let phi_star_l = phi_l.hypot(sigma_l_prime);
        let phi_l_prime = 1.0 / (1.0 / (phi_star_l * phi_star_l) + 1.0 / v_l).sqrt();
        let mu_l_prime = mu_l + phi_l_prime * phi_l_prime * g_w * (0.0 - e_l);

        loser.rating = mu_l_prime * SCALE + 1500.0;
        loser.deviation = phi_l_prime * SCALE;
        loser.volatility = sigma_l_prime;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rating() {
        let r = Rating::default();
        assert_eq!(r.rating, 1500.0);
        assert_eq!(r.deviation, 350.0);
        assert_eq!(r.volatility, 0.06);
    }

    #[test]
    fn test_winner_rating_increases() {
        let mut winner = Rating::default();
        let mut loser = Rating::default();
        update_ratings(&mut winner, &mut loser);
        assert!(winner.rating > 1500.0);
        assert!(loser.rating < 1500.0);
    }

    #[test]
    fn test_symmetric_changes_for_equal_players() {
        let mut a = Rating::default();
        let mut b = Rating::default();
        let a_before = a.rating;
        let b_before = b.rating;
        update_ratings(&mut a, &mut b);
        let a_gain = a.rating - a_before;
        let b_loss = b_before - b.rating;
        assert!(
            (a_gain - b_loss).abs() < 1.0,
            "changes should be roughly symmetric"
        );
    }

    #[test]
    fn test_deviation_decreases_after_match() {
        let mut a = Rating::default();
        let mut b = Rating::default();
        update_ratings(&mut a, &mut b);
        assert!(a.deviation < 350.0);
        assert!(b.deviation < 350.0);
    }

    #[test]
    fn test_upset_produces_larger_swing() {
        let mut strong = Rating {
            rating: 1800.0,
            deviation: 50.0,
            volatility: 0.06,
        };
        let mut weak = Rating {
            rating: 1200.0,
            deviation: 50.0,
            volatility: 0.06,
        };
        let strong_before = strong.rating;
        update_ratings(&mut weak, &mut strong); // weak wins (upset)
        let swing = strong_before - strong.rating;
        assert!(
            swing > 5.0,
            "upset should produce significant swing, got {swing}"
        );
    }

    #[test]
    fn test_high_deviation_produces_larger_change() {
        let mut a1 = Rating {
            rating: 1500.0,
            deviation: 350.0,
            volatility: 0.06,
        }; // uncertain
        let mut b1 = Rating::default();
        update_ratings(&mut a1, &mut b1);

        let mut a2 = Rating {
            rating: 1500.0,
            deviation: 50.0,
            volatility: 0.06,
        }; // confident
        let mut b2 = Rating::default();
        update_ratings(&mut a2, &mut b2);

        assert!(
            (a1.rating - 1500.0).abs() > (a2.rating - 1500.0).abs(),
            "uncertain player should have larger rating change"
        );
    }

    #[test]
    fn test_convergence_over_many_matches() {
        // If a 1500-rated player beats a 1500-rated player 10 times in a row,
        // their rating should be significantly higher and deviation should be low.
        let mut winner = Rating::default();
        let mut loser = Rating::default();
        for _ in 0..10 {
            let mut w = winner.clone();
            let mut l = loser.clone();
            update_ratings(&mut w, &mut l);
            winner = w;
            loser = l;
        }
        assert!(
            winner.rating > 1700.0,
            "winner rating after 10 wins: {}",
            winner.rating
        );
        // Deviation should meaningfully decrease from the initial 350.0.
        // With 10 rounds against an equally uncertain opponent (also starting at 350)
        // the algorithm converges to ~200, so we verify it's well below the starting value.
        assert!(
            winner.deviation < 210.0,
            "winner deviation should decrease from 350.0, got {}",
            winner.deviation
        );
    }
}
