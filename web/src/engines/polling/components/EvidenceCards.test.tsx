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
    expect(screen.queryByText(/Research/)).not.toBeInTheDocument();
  });

  it('shows research label and evidence by default', () => {
    render(<EvidenceCards evidence={mockEvidence} />);

    // Research label with count is visible
    expect(screen.getByText(/Research \(2\)/)).toBeInTheDocument();

    // Claims are visible by default (open by default)
    expect(screen.getByText('Good labor practices')).toBeInTheDocument();
    expect(screen.getByText('Low wages reported')).toBeInTheDocument();
  });

  it('collapses on click', async () => {
    const user = userEvent.setup();
    render(<EvidenceCards evidence={mockEvidence} />);

    // Click to collapse
    await user.click(screen.getByText(/Research \(2\)/));

    // Claims still in the DOM (Collapse hides via CSS, not removal)
    expect(screen.getByText('Good labor practices')).toBeInTheDocument();
  });

  it('shows pro/con indicators and source attribution', () => {
    render(<EvidenceCards evidence={mockEvidence} />);

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
