import { render, screen } from '@test-utils';
import { describe, expect, it } from 'vitest';
import { PollCountdown } from './PollCountdown';

describe('PollCountdown', () => {
  it('renders nothing when secondsLeft is null', () => {
    render(<PollCountdown secondsLeft={null} />);
    expect(screen.queryByText(/Closes in/)).not.toBeInTheDocument();
    expect(screen.queryByText('Closing...')).not.toBeInTheDocument();
  });

  it('displays formatted time for > 60 seconds', () => {
    render(<PollCountdown secondsLeft={90} />);
    expect(screen.getByText('Closes in 01:30')).toBeInTheDocument();
  });

  it('displays formatted time for < 60 seconds', () => {
    render(<PollCountdown secondsLeft={45} />);
    expect(screen.getByText('Closes in 00:45')).toBeInTheDocument();
  });

  it('displays hours and minutes for durations >= 1 hour', () => {
    render(<PollCountdown secondsLeft={3661} />);
    expect(screen.getByText('Closes in 1h 1m')).toBeInTheDocument();
  });

  it('displays days and hours for durations >= 24 hours', () => {
    render(<PollCountdown secondsLeft={90000} />);
    expect(screen.getByText('Closes in 1d 1h')).toBeInTheDocument();
  });

  it('displays closing message when secondsLeft is 0', () => {
    render(<PollCountdown secondsLeft={0} />);
    expect(screen.getByText('Closing...')).toBeInTheDocument();
  });
});
