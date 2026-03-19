import { render, screen } from '@test-utils';
import { describe, expect, it, vi } from 'vitest';
import type { Room } from '@/features/rooms';
import type { Poll } from './api';
import { PollEngineView } from './PollEngineView';

// Mock usePolls from ./api
const mockUsePolls = vi.fn();
vi.mock('./api', async (importOriginal) => {
  const original = await importOriginal<typeof import('./api')>();
  return {
    ...original,
    usePolls: (...args: unknown[]) => mockUsePolls(...args),
  };
});

// Mock usePollCountdown
vi.mock('./hooks/usePollCountdown', () => ({
  usePollCountdown: vi.fn(() => ({ secondsLeft: 300, isExpired: false })),
}));

// Mock @tanstack/react-router Link
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...props }: { children: React.ReactNode; to: string }) => (
    <a href={to} {...props}>
      {children}
    </a>
  ),
}));

function makeRoom(overrides: Partial<Room> = {}): Room {
  return {
    id: 'room-1',
    name: 'Test Room',
    description: 'A test room description',
    eligibility_topic: 'general',
    engine_type: 'polling',
    engine_config: {},
    status: 'active',
    created_at: '2026-01-01T00:00:00Z',
    owner_id: null,
    constraint_type: 'none',
    ...overrides,
  };
}

function makePoll(overrides: Partial<Poll> = {}): Poll {
  return {
    id: 'poll-1',
    room_id: 'room-1',
    question: 'Should we do this?',
    description: null,
    status: 'active',
    created_at: '2026-01-01T00:00:00Z',
    closes_at: null,
    activated_at: null,
    ...overrides,
  };
}

const defaultProps = {
  room: makeRoom(),
  roomId: 'room-1',
  eligibility: { isEligible: true },
};

describe('PollEngineView', () => {
  it('renders room name and description', () => {
    mockUsePolls.mockReturnValue({ data: [], isLoading: false });

    render(<PollEngineView {...defaultProps} />);

    expect(screen.getByText('Test Room')).toBeInTheDocument();
    expect(screen.getByText('A test room description')).toBeInTheDocument();
  });

  it('shows "No active poll" message when no active polls', () => {
    mockUsePolls.mockReturnValue({ data: [], isLoading: false });

    render(<PollEngineView {...defaultProps} />);

    expect(screen.getByText(/No active poll/i)).toBeInTheDocument();
  });

  it('renders active poll as hero card with "Vote now" link', () => {
    const activePoll = makePoll({
      id: 'poll-active',
      status: 'active',
      question: 'Active question?',
    });
    mockUsePolls.mockReturnValue({ data: [activePoll], isLoading: false });

    render(<PollEngineView {...defaultProps} />);

    expect(screen.getByText('Active question?')).toBeInTheDocument();
    const voteLink = screen.getByRole('link', { name: /vote now/i });
    expect(voteLink).toBeInTheDocument();
    expect(voteLink).toHaveAttribute('href', '/rooms/room-1/polls/poll-active');
  });

  it('renders draft polls under "Up next" heading', () => {
    const draftPoll = makePoll({ id: 'poll-draft', status: 'draft', question: 'Draft question?' });
    mockUsePolls.mockReturnValue({ data: [draftPoll], isLoading: false });

    render(<PollEngineView {...defaultProps} />);

    expect(screen.getByText(/Up next/i)).toBeInTheDocument();
    expect(screen.getByText('Draft question?')).toBeInTheDocument();
  });

  it('renders closed polls under "Past polls" heading with link to poll page', () => {
    const closedPoll = makePoll({
      id: 'poll-closed',
      status: 'closed',
      question: 'Closed question?',
    });
    mockUsePolls.mockReturnValue({ data: [closedPoll], isLoading: false });

    render(<PollEngineView {...defaultProps} />);

    expect(screen.getByText(/Past polls/i)).toBeInTheDocument();
    expect(screen.getByText('Closed question?')).toBeInTheDocument();
    const pollLink = screen.getByRole('link', { name: /Closed question\?/i });
    expect(pollLink).toHaveAttribute('href', '/rooms/room-1/polls/poll-closed');
  });

  it('hides "Up next" section when no draft polls', () => {
    const activePoll = makePoll({ status: 'active' });
    mockUsePolls.mockReturnValue({ data: [activePoll], isLoading: false });

    render(<PollEngineView {...defaultProps} />);

    expect(screen.queryByText(/Up next/i)).not.toBeInTheDocument();
  });

  it('hides "Past polls" section when no closed polls', () => {
    const activePoll = makePoll({ status: 'active' });
    mockUsePolls.mockReturnValue({ data: [activePoll], isLoading: false });

    render(<PollEngineView {...defaultProps} />);

    expect(screen.queryByText(/Past polls/i)).not.toBeInTheDocument();
  });
});
