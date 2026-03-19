import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MantineProvider } from '@mantine/core';
import type { Poll } from '../api';
import { AgendaProgress } from './AgendaProgress';

function wrap(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

function makePoll(id: string): Poll {
  return {
    id,
    room_id: 'r1',
    question: `Q${id}`,
    description: null,
    status: 'active',
    created_at: '',
    closes_at: null,
  };
}

describe('AgendaProgress', () => {
  it('renders nothing for a single poll', () => {
    wrap(<AgendaProgress polls={[makePoll('a')]} activePollId="a" />);
    expect(screen.queryByText(/Question/)).not.toBeInTheDocument();
  });

  it('shows first question of N', () => {
    const polls = ['a', 'b', 'c'].map(makePoll);
    wrap(<AgendaProgress polls={polls} activePollId="a" />);
    expect(screen.getByText('Question 1 of 3')).toBeInTheDocument();
  });

  it('shows correct position when active is not first', () => {
    const polls = ['a', 'b', 'c'].map(makePoll);
    wrap(<AgendaProgress polls={polls} activePollId="c" />);
    expect(screen.getByText('Question 3 of 3')).toBeInTheDocument();
  });

  it('renders nothing when activePollId is not in the list', () => {
    const polls = ['a', 'b'].map(makePoll);
    wrap(<AgendaProgress polls={polls} activePollId="z" />);
    expect(screen.queryByText(/Question/)).not.toBeInTheDocument();
  });
});
