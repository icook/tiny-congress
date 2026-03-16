/**
 * Endorsement weight computation (ADR-023: fixed slots with variable weight).
 *
 * Final weight = base_weight(delivery_method) × depth_multiplier(relationship_depth),
 * clamped to (0, 1.0].
 */

export const DELIVERY_METHODS = [
  { value: 'qr', label: 'In-person (QR scan)', base: 1.0 },
  { value: 'video', label: 'Video chat', base: 0.7 },
  { value: 'text', label: 'Text / messaging', base: 0.4 },
  { value: 'email', label: 'Email', base: 0.2 },
] as const;

export const RELATIONSHIP_DEPTHS = [
  { value: 'years', label: 'Known for years', multiplier: 1.0 },
  { value: 'months', label: 'Known for months', multiplier: 0.7 },
  { value: 'acquaintance', label: 'Acquaintance', multiplier: 0.5 },
] as const;

export type DeliveryMethod = (typeof DELIVERY_METHODS)[number]['value'];
export type RelationshipDepth = (typeof RELATIONSHIP_DEPTHS)[number]['value'];

/**
 * Compute endorsement weight for a given delivery method and relationship depth.
 * Returns a value in (0, 1.0].
 */
export function computeWeight(method: DeliveryMethod, depth: RelationshipDepth): number {
  const base = DELIVERY_METHODS.find((m) => m.value === method)?.base ?? 0.2;
  const multiplier = RELATIONSHIP_DEPTHS.find((d) => d.value === depth)?.multiplier ?? 0.5;
  return Math.min(1.0, Math.max(Number.EPSILON, base * multiplier));
}

/**
 * Human-readable label for a weight value.
 */
export function weightLabel(weight: number): string {
  if (weight >= 0.8) {
    return 'Strong endorsement';
  }
  if (weight >= 0.4) {
    return 'Moderate endorsement';
  }
  return 'Weak endorsement';
}
