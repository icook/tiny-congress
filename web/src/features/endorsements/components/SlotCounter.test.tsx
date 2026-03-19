import { render, screen } from '@test-utils';
import { SlotCounter } from './SlotCounter';

describe('SlotCounter', () => {
  it('displays used and total slots', () => {
    render(<SlotCounter used={2} total={3} />);
    expect(screen.getByText('2 of 3 in-slot')).toBeInTheDocument();
  });

  it('displays 0 of 3 when empty', () => {
    render(<SlotCounter used={0} total={3} />);
    expect(screen.getByText('0 of 3 in-slot')).toBeInTheDocument();
  });

  it('shows additional out-of-slot count when present', () => {
    render(<SlotCounter used={3} total={3} outOfSlot={2} />);
    expect(screen.getByText('3 of 3 in-slot + 2 additional')).toBeInTheDocument();
  });
});
