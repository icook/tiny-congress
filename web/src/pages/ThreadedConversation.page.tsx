import { Container, Title, Text, Box, Group, Button } from '@mantine/core';
import { useState } from 'react';
import { ThreadedConversation, Thread } from '../components/ThreadedConversation/ThreadedConversation';

// Mock data for our conversation
const mockDimensions = [
  {
    id: 'relevance',
    name: 'Relevance',
    description: 'How relevant and on-topic is this response'
  },
  {
    id: 'depth',
    name: 'Depth',
    description: 'How deep or insightful is this response'
  },
  {
    id: 'creativity',
    name: 'Creativity',
    description: 'How creative or novel is this response'
  },
  {
    id: 'clarity',
    name: 'Clarity',
    description: 'How clear and easy to understand is this response'
  }
];

// Create initial conversation thread
const createMockThread = (): Thread => {
  const now = new Date();
  return {
    id: '1',
    title: 'Discussion on AI Ethics and Governance',
    dimensions: mockDimensions,
    activeInterval: 30000, // 30 seconds
    branches: [
      // Root message
      {
        id: 'msg1',
        parentId: null,
        content: "Welcome to our discussion on AI ethics and governance. Today we'll explore how society should regulate artificial intelligence as it becomes increasingly capable.",
        author: {
          id: 'host',
          name: 'Discussion Host',
          avatar: '',
          isAI: false
        },
        timestamp: new Date(now.getTime() - 3600000), // 1 hour ago
        votes: [],
        isSelected: true,
        isViable: true,
        isHidden: false
      },
      
      // First level responses
      {
        id: 'msg2',
        parentId: 'msg1',
        content: "I believe AI regulation should primarily focus on transparency. Companies developing powerful AI systems should be required to disclose their capabilities, limitations, and potential risks.",
        author: {
          id: 'user1',
          name: 'Sarah Thompson',
          avatar: '',
          isAI: false
        },
        timestamp: new Date(now.getTime() - 3300000), // 55 minutes ago
        votes: [
          { userId: 'user4', dimension: 'relevance', value: 4 },
          { userId: 'user2', dimension: 'depth', value: 3 },
          { userId: 'user3', dimension: 'creativity', value: 2 },
          { userId: 'user5', dimension: 'clarity', value: 4 }
        ],
        isSelected: true,
        isViable: true,
        isHidden: false
      },
      {
        id: 'msg3',
        parentId: 'msg1',
        content: "Regulation is less important than education. We need to focus on making sure the public understands what AI can and cannot do, to prevent both irrational fears and dangerous overreliance.",
        author: {
          id: 'user2',
          name: 'Marcus Johnson',
          avatar: '',
          isAI: false
        },
        timestamp: new Date(now.getTime() - 3250000), // ~54 minutes ago
        votes: [
          { userId: 'user1', dimension: 'relevance', value: 3 },
          { userId: 'user3', dimension: 'depth', value: 2 },
          { userId: 'user4', dimension: 'creativity', value: 1 },
          { userId: 'user5', dimension: 'clarity', value: 3 }
        ],
        isSelected: false,
        isViable: true,
        isHidden: false
      },
      {
        id: 'msg4',
        parentId: 'msg1',
        content: "I think we should ban AI development entirely until we have better safety protocols in place. The risks are too high.",
        author: {
          id: 'user3',
          name: 'Alex Rivera',
          avatar: '',
          isAI: false
        },
        timestamp: new Date(now.getTime() - 3200000), // ~53 minutes ago
        votes: [
          { userId: 'user1', dimension: 'relevance', value: 2 },
          { userId: 'user2', dimension: 'depth', value: -1 },
          { userId: 'user4', dimension: 'creativity', value: -2 },
          { userId: 'user5', dimension: 'clarity', value: 1 }
        ],
        isSelected: false,
        isViable: false,
        isHidden: true
      },
      
      // Second level - responses to the selected first level message
      {
        id: 'msg5',
        parentId: 'msg2',
        content: "Transparency is essential, but I'd add that we also need mandatory safety evaluations before deploying systems that reach certain capability thresholds. Companies should be required to demonstrate their AI systems won't cause harm before release.",
        author: {
          id: 'ai-assistant',
          name: 'AI Assistant',
          avatar: '',
          isAI: true
        },
        timestamp: new Date(now.getTime() - 3000000), // 50 minutes ago
        votes: [
          { userId: 'user1', dimension: 'relevance', value: 5 },
          { userId: 'user2', dimension: 'depth', value: 4 },
          { userId: 'user3', dimension: 'creativity', value: 3 },
          { userId: 'user4', dimension: 'clarity', value: 5 }
        ],
        isSelected: true,
        isViable: true,
        isHidden: false
      },
      
      // Third level - branches after AI response
      {
        id: 'msg6',
        parentId: 'msg5',
        content: "I agree with mandatory safety evaluations, but who should perform these evaluations? A government agency might lack technical expertise, while industry self-regulation has obvious conflicts of interest.",
        author: {
          id: 'user5',
          name: 'Elena Chen',
          avatar: '',
          isAI: false
        },
        timestamp: new Date(now.getTime() - 2700000), // 45 minutes ago
        votes: [
          { userId: 'user1', dimension: 'relevance', value: 4 },
          { userId: 'user2', dimension: 'depth', value: 4 },
          { userId: 'user3', dimension: 'creativity', value: 3 },
          { userId: 'user4', dimension: 'clarity', value: 4 }
        ],
        isSelected: false,
        isViable: true,
        isHidden: false
      },
      {
        id: 'msg7',
        parentId: 'msg5',
        content: "What about international coordination? If one country imposes strict regulations but others don't, companies might just relocate their AI development to less regulated jurisdictions.",
        author: {
          id: 'user6',
          name: 'David Patel',
          avatar: '',
          isAI: false
        },
        timestamp: new Date(now.getTime() - 2650000), // ~44 minutes ago
        votes: [
          { userId: 'user1', dimension: 'relevance', value: 5 },
          { userId: 'user2', dimension: 'depth', value: 5 },
          { userId: 'user3', dimension: 'creativity', value: 4 },
          { userId: 'user4', dimension: 'clarity', value: 5 }
        ],
        isSelected: true,
        isViable: true,
        isHidden: false
      },
      {
        id: 'msg8',
        parentId: 'msg5',
        content: "I think open source is the answer. If AI models and safety methods are transparent and publicly available, the entire global community can help identify and address risks.",
        author: {
          id: 'user7',
          name: 'Jamal Washington',
          avatar: '',
          isAI: false
        },
        timestamp: new Date(now.getTime() - 2600000), // ~43 minutes ago
        votes: [
          { userId: 'user1', dimension: 'relevance', value: 3 },
          { userId: 'user2', dimension: 'depth', value: 2 },
          { userId: 'user3', dimension: 'creativity', value: 4 },
          { userId: 'user4', dimension: 'clarity', value: 3 }
        ],
        isSelected: false,
        isViable: true,
        isHidden: false
      },
      
      // Fourth level - response to selected third level message
      {
        id: 'msg9',
        parentId: 'msg7',
        content: "International coordination is indeed crucial. We might need something like an international AI governance body, similar to how we have organizations for nuclear energy or climate change. This body could establish minimum safety standards that all participating countries agree to enforce, preventing regulatory arbitrage while respecting national sovereignty in implementation details. Countries with advanced AI sectors could lead by example, demonstrating that robust safety protocols are compatible with innovation.",
        author: {
          id: 'ai-assistant',
          name: 'AI Assistant',
          avatar: '',
          isAI: true
        },
        timestamp: new Date(now.getTime() - 2400000), // 40 minutes ago
        votes: [],
        isSelected: false,
        isViable: true,
        isHidden: false
      },
      {
        id: 'msg10',
        parentId: 'msg7',
        content: "Perhaps we need a differentiated approach to regulation. Basic AI applications could have lighter touch regulations, while highly capable systems that pose systemic risks would be subject to more stringent oversight. This would prevent stifling innovation in less risky areas while ensuring proper scrutiny where it matters most.",
        author: {
          id: 'ai-assistant',
          name: 'AI Assistant',
          avatar: '',
          isAI: true
        },
        timestamp: new Date(now.getTime() - 2390000), // ~40 minutes ago
        votes: [],
        isSelected: false,
        isViable: true,
        isHidden: false
      },
      {
        id: 'msg11',
        parentId: 'msg7',
        content: "We could learn from other fields like biotechnology or nuclear energy, which have established international safety regimes. What worked well in those domains, and what didn't? One approach might be to establish an international technical standards body that defines safety benchmarks and testing protocols, separate from the political bodies that determine how those standards are enforced.",
        author: {
          id: 'ai-assistant',
          name: 'AI Assistant',
          avatar: '',
          isAI: true
        },
        timestamp: new Date(now.getTime() - 2380000), // ~40 minutes ago
        votes: [],
        isSelected: false,
        isViable: true,
        isHidden: false
      }
    ]
  };
};

export function ThreadedConversationPage() {
  const [thread, setThread] = useState<Thread>(createMockThread());
  
  const handleReset = () => {
    setThread(createMockThread());
  };
  
  const toggleTimerMode = () => {
    setThread({
      ...thread,
      activeInterval: thread.activeInterval === null ? 30000 : null
    });
  };

  return (
    <Container size="lg" py="xl">
      <Title order={2} mb="md">Threaded Conversation Demo</Title>
      <Text mb="xl">
        This demo shows a threaded conversation interface where users can vote on potential
        conversation branches. The system will automatically select the highest-rated branch 
        after a set period of time, or you can manually select branches.
      </Text>
      
      <Group mb="xl">
        <Button onClick={handleReset} variant="outline">
          Reset Conversation
        </Button>
        <Button onClick={toggleTimerMode} color={thread.activeInterval === null ? "blue" : "red"}>
          {thread.activeInterval === null ? "Enable Auto-Selection (30s)" : "Disable Auto-Selection"}
        </Button>
      </Group>
      
      <Box>
        <ThreadedConversation thread={thread} />
      </Box>
    </Container>
  );
}