import { ThreadedConversation, type Thread } from './ThreadedConversation';

export default { title: 'Components/ThreadedConversation' };

const baseBranch = (overrides: Partial<Thread['branches'][0]> = {}): Thread['branches'][0] => ({
  id: 'b1',
  parentId: null,
  content: 'This is the initial proposal for discussion.',
  author: { id: 'u1', name: 'Alice', isAI: false },
  timestamp: new Date('2026-03-15T10:00:00Z'),
  votes: [],
  isSelected: true,
  isViable: true,
  isHidden: false,
  ...overrides,
});

const dimensions: Thread['dimensions'] = [
  { id: 'd1', name: 'Feasibility', description: 'How practical is this?' },
  { id: 'd2', name: 'Impact', description: 'How significant is the effect?' },
];

export const SingleBranch = () => (
  <ThreadedConversation
    thread={{
      id: 't1',
      title: 'Single Branch',
      branches: [baseBranch()],
      dimensions,
      activeInterval: null,
    }}
  />
);

export const WithAlternatives = () => (
  <ThreadedConversation
    thread={{
      id: 't2',
      title: 'Multiple Alternatives',
      branches: [
        baseBranch(),
        baseBranch({
          id: 'b2',
          content: 'Alternative perspective on the proposal.',
          author: { id: 'u2', name: 'Bob', isAI: false },
          isSelected: false,
        }),
        baseBranch({
          id: 'b3',
          content: 'A third viewpoint worth considering.',
          author: { id: 'u3', name: 'Charlie', isAI: false },
          isSelected: false,
        }),
      ],
      dimensions,
      activeInterval: null,
    }}
  />
);

export const AIAuthor = () => (
  <ThreadedConversation
    thread={{
      id: 't3',
      title: 'AI Contribution',
      branches: [
        baseBranch({
          content: 'AI-synthesized summary of the discussion so far.',
          author: { id: 'ai1', name: 'Facilitator', isAI: true },
        }),
      ],
      dimensions,
      activeInterval: null,
    }}
  />
);
