import { render, screen } from '@test-utils';
import { describe, expect, it, vi } from 'vitest';
import type { Poll } from '../api';
import { UpcomingPollPreview } from './UpcomingPollPreview';

vi.mock('@tanstack/react-router', () => ({
  Link: (props: any) => <a href={props.to}>{props.children}</a>,
}));

function wrap(ui: React.ReactElement) {
  return render(ui);
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
    activated_at: null,
  };
}

describe('UpcomingPollPreview', () => {
  it('renders nothing when poll is undefined', () => {
    wrap(<UpcomingPollPreview poll={undefined} roomId="r1" />);
    expect(screen.queryByText('Up next')).not.toBeInTheDocument();
  });

  it('renders the up next label and question', () => {
    wrap(<UpcomingPollPreview poll={makePoll('Should we build a park?')} roomId="r1" />);
    expect(screen.getByText('Up next')).toBeInTheDocument();
    expect(screen.getByText('Should we build a park?')).toBeInTheDocument();
  });
});
