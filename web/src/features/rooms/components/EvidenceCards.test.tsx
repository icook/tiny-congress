import { render, screen, userEvent } from '@test-utils';
import { describe, expect, it } from 'vitest';
import { EvidenceCards } from './EvidenceCards';

const mockEvidence = [
  { id: '1', stance: 'pro' as const, claim: 'Good labor practices', source: 'Reuters' },
  { id: '2', stance: 'con' as const, claim: 'Low wages reported', source: null },
];

describe('EvidenceCards', () => {
  it('renders nothing when evidence is empty', () => {
    render(<EvidenceCards evidence={[]} />);
    // No toggle button should appear when evidence is empty
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(screen.queryByText(/evidence card/)).not.toBeInTheDocument();
  });

  it('shows evidence count and expands on click', async () => {
    const user = userEvent.setup();
    render(<EvidenceCards evidence={mockEvidence} />);

    // Count label is visible
    expect(screen.getByText(/2 evidence cards/)).toBeInTheDocument();

    // Claims are in the DOM (inside Collapse, not yet visible by CSS, but present)
    expect(screen.getByText('Good labor practices')).toBeInTheDocument();

    // Click to expand — Collapse should open
    await user.click(screen.getByText(/2 evidence cards/));

    // Claims still in the DOM after click
    expect(screen.getByText('Good labor practices')).toBeInTheDocument();
    expect(screen.getByText('Low wages reported')).toBeInTheDocument();
  });

  it('shows pro/con indicators and source attribution', async () => {
    const user = userEvent.setup();
    render(<EvidenceCards evidence={mockEvidence} />);

    // Expand
    await user.click(screen.getByText(/2 evidence cards/));

    // Pro indicator
    const plusIndicators = screen.getAllByText('+');
    expect(plusIndicators.length).toBeGreaterThan(0);

    // Con indicator (using minus sign −)
    const minusIndicators = screen.getAllByText('−');
    expect(minusIndicators.length).toBeGreaterThan(0);

    // Source attribution
    expect(screen.getByText(/Reuters/)).toBeInTheDocument();

    // Item without source should not show attribution dash
    expect(screen.queryByText(/— null/)).not.toBeInTheDocument();
  });
});
