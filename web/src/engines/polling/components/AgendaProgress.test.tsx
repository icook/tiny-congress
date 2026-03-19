import { render, screen } from '@test-utils';
import { describe, expect, it } from 'vitest';
import type { Poll } from '../api';
import { AgendaProgress } from './AgendaProgress';

function makePoll(id: string): Poll {
  return {
    id,
    room_id: 'r1',
    question: `Q${id}`,
    description: null,
    status: 'active',
    created_at: '',
    closes_at: null,
    activated_at: null,
  };
}

describe('AgendaProgress', () => {
  it('renders nothing for a single poll', () => {
    render(<AgendaProgress polls={[makePoll('a')]} activePollId="a" />);
    expect(screen.queryByText(/\//)).not.toBeInTheDocument();
  });

  it('shows first position of N', () => {
    const polls = ['a', 'b', 'c'].map(makePoll);
    render(<AgendaProgress polls={polls} activePollId="a" />);
    expect(screen.getByText('1 / 3')).toBeInTheDocument();
    expect(screen.getByRole('progressbar')).toBeInTheDocument();
  });

  it('shows correct position when active is not first', () => {
    const polls = ['a', 'b', 'c'].map(makePoll);
    render(<AgendaProgress polls={polls} activePollId="c" />);
    expect(screen.getByText('3 / 3')).toBeInTheDocument();
  });

  it('renders nothing when activePollId is not in the list', () => {
    const polls = ['a', 'b'].map(makePoll);
    render(<AgendaProgress polls={polls} activePollId="z" />);
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });
});
