import {
  computeWeight,
  DELIVERY_METHODS,
  RELATIONSHIP_DEPTHS,
  weightLabel,
} from './weightCalculator';

describe('computeWeight', () => {
  it('computes QR + years as 1.0', () => {
    expect(computeWeight('qr', 'years')).toBe(1.0);
  });

  it('computes video + months as ~0.49', () => {
    expect(computeWeight('video', 'months')).toBeCloseTo(0.49);
  });

  it('computes text + acquaintance as ~0.2', () => {
    expect(computeWeight('text', 'acquaintance')).toBeCloseTo(0.2);
  });

  it('computes email + acquaintance as ~0.1', () => {
    expect(computeWeight('email', 'acquaintance')).toBeCloseTo(0.1);
  });

  it('clamps to minimum above zero', () => {
    expect(computeWeight('email', 'acquaintance')).toBeGreaterThan(0);
  });

  it('never exceeds 1.0', () => {
    for (const method of DELIVERY_METHODS) {
      for (const depth of RELATIONSHIP_DEPTHS) {
        const w = computeWeight(method.value, depth.value);
        expect(w).toBeLessThanOrEqual(1.0);
        expect(w).toBeGreaterThan(0);
      }
    }
  });
});

describe('weightLabel', () => {
  it('labels strong endorsements', () => {
    expect(weightLabel(1.0)).toBe('Strong endorsement');
    expect(weightLabel(0.8)).toBe('Strong endorsement');
  });

  it('labels moderate endorsements', () => {
    expect(weightLabel(0.49)).toBe('Moderate endorsement');
    expect(weightLabel(0.4)).toBe('Moderate endorsement');
  });

  it('labels weak endorsements', () => {
    expect(weightLabel(0.1)).toBe('Weak endorsement');
    expect(weightLabel(0.39)).toBe('Weak endorsement');
  });
});
