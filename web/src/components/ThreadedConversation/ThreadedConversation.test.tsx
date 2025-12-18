import { render, screen, userEvent, waitFor } from '@test-utils';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
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
  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  test('selects the highest rated branch when triggered manually', async () => {
    const user = userEvent.setup();
    render(<ThreadedConversation thread={buildThread()} />);

    await user.click(screen.getByRole('button', { name: /select top branch now/i }));

    expect(screen.getAllByText('Selected').length).toBeGreaterThan(1);
    expect(screen.queryByText('Option B')).not.toBeInTheDocument();
  });

  test('auto-selects the top branch when the timer elapses', async () => {
    render(<ThreadedConversation thread={buildThread()} />);

    await waitFor(() => expect(screen.getAllByText('Selected').length).toBeGreaterThan(1), {
      timeout: 3000,
    });
    expect(screen.queryByText('Option B')).not.toBeInTheDocument();
  });
});
