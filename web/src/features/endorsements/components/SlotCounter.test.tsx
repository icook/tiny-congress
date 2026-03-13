import { render, screen } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { SlotCounter } from './SlotCounter';

function renderWithMantine(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

describe('SlotCounter', () => {
  it('displays used and total slots', () => {
    renderWithMantine(<SlotCounter used={2} total={3} />);
    expect(screen.getByText('2 of 3 used')).toBeInTheDocument();
  });

  it('displays 0 of 3 when empty', () => {
    renderWithMantine(<SlotCounter used={0} total={3} />);
    expect(screen.getByText('0 of 3 used')).toBeInTheDocument();
  });
});
