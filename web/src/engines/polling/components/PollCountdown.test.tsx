import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MantineProvider } from '@mantine/core';
import { PollCountdown } from './PollCountdown';

function wrap(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

describe('PollCountdown', () => {
  it('renders nothing when secondsLeft is null', () => {
    wrap(<PollCountdown secondsLeft={null} />);
    expect(screen.queryByText(/Closes in/)).not.toBeInTheDocument();
    expect(screen.queryByText('Closing...')).not.toBeInTheDocument();
  });

  it('displays formatted time for > 60 seconds', () => {
    wrap(<PollCountdown secondsLeft={90} />);
    expect(screen.getByText('Closes in 01:30')).toBeInTheDocument();
  });

  it('displays formatted time for < 60 seconds', () => {
    wrap(<PollCountdown secondsLeft={45} />);
    expect(screen.getByText('Closes in 00:45')).toBeInTheDocument();
  });

  it('displays closing message when secondsLeft is 0', () => {
    wrap(<PollCountdown secondsLeft={0} />);
    expect(screen.getByText('Closing...')).toBeInTheDocument();
  });
});
