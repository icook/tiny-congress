export interface TierInfo {
  label: string;
  color: string;
}

/**
 * Returns tier label and color for a trust score, or null if the score does
 * not meet the minimum threshold for a named tier.
 *
 * Tiers (highest to lowest):
 *   Congress  — distance ≤ 3.0 AND path_diversity ≥ 2  → violet
 *   Community — distance ≤ 6.0 AND path_diversity ≥ 1  → blue
 */
export function getTierInfo(distance: number, diversity: number): TierInfo | null {
  if (distance <= 3.0 && diversity >= 2) {
    return { label: 'Congress', color: 'violet' };
  }
  if (distance <= 6.0 && diversity >= 1) {
    return { label: 'Community', color: 'blue' };
  }
  return null;
}
