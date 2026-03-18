import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MantineProvider } from '@mantine/core';
import type { Poll } from '../api';
import { UpcomingPollPreview } from './UpcomingPollPreview';

function wrap(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

function makePoll(question: string): Poll {
  return {
    id: '1',
    room_id: 'r1',
    question,
    description: null,
    status: 'pending',
    created_at: '',
    closes_at: null,
  };
}

describe('UpcomingPollPreview', () => {
  it('renders nothing when poll is undefined', () => {
    wrap(<UpcomingPollPreview poll={undefined} />);
    expect(screen.queryByText('Up next')).not.toBeInTheDocument();
  });

  it('renders the up next label and question', () => {
    wrap(<UpcomingPollPreview poll={makePoll('Should we build a park?')} />);
    expect(screen.getByText('Up next')).toBeInTheDocument();
    expect(screen.getByText('Should we build a park?')).toBeInTheDocument();
  });
});
