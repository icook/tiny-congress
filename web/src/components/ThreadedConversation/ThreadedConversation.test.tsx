import { act, render, screen, userEvent } from '@test-utils';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { ThreadedConversation, type ConversationBranch, type Thread } from './ThreadedConversation';

const baseTimestamp = new Date('2024-01-01T00:00:00Z');

const baseAuthor = { id: 'u1', name: 'User One', isAI: false, avatar: '' };

function buildBranch(overrides: Partial<ConversationBranch>): ConversationBranch {
  return {
    id: 'branch',
    parentId: null,
    content: 'content',
    author: baseAuthor,
    timestamp: baseTimestamp,
    votes: [],
    isSelected: false,
    isViable: true,
    isHidden: false,
    ...overrides,
  };
}

function buildThread(): Thread {
  return {
    id: 'thread-1',
    title: 'Test Thread',
    activeInterval: 1000,
    dimensions: [{ id: 'quality', name: 'Quality', description: '' }],
    branches: [
      buildBranch({ id: 'root', content: 'Root', isSelected: true }),
      buildBranch({
        id: 'a',
        parentId: 'root',
        content: 'Option A',
        votes: [{ userId: 'user', dimension: 'quality', value: 5 }],
      }),
      buildBranch({
        id: 'b',
        parentId: 'root',
        content: 'Option B',
        votes: [{ userId: 'user', dimension: 'quality', value: -2 }],
      }),
    ],
  };
}

describe('ThreadedConversation', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  test('selects the highest rated branch when triggered manually', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup();
    render(<ThreadedConversation thread={buildThread()} />);

    await user.click(screen.getByRole('button', { name: /select top branch now/i }));

    expect(screen.getAllByText('Selected').length).toBeGreaterThan(1);
    expect(screen.queryByText('Option B')).not.toBeInTheDocument();
  });

  test('auto-selects the top branch when the timer elapses', () => {
    vi.useFakeTimers();
    render(<ThreadedConversation thread={buildThread()} />);

    // Timer starts at 1000ms, interval fires every 1000ms:
    // - First tick: decrements 1000 -> 0
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    // - Second tick: sees 0, triggers selection
    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(screen.getAllByText('Selected').length).toBeGreaterThan(1);
    expect(screen.queryByText('Option B')).not.toBeInTheDocument();
  });
});
