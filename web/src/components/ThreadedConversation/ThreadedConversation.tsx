import { useEffect, useState } from 'react';
import {
  IconArrowBack,
  IconArrowForward,
  IconChevronRight,
  IconClock,
  IconThumbDown,
  IconThumbUp,
} from '@tabler/icons-react';
import {
  Accordion,
  ActionIcon,
  Avatar,
  Badge,
  Box,
  Button,
  Card,
  Group,
  MantineTheme,
  Paper,
  rem,
  Slider,
  Stack,
  Text,
  Tooltip,
  Transition,
} from '@mantine/core';

// Types for our conversation system
export interface Dimension {
  id: string;
  name: string;
  description: string;
}

export interface Vote {
  userId: string;
  value: number; // -5 to 5
  dimension: string;
}

export interface ConversationBranch {
  id: string;
  parentId: string | null;
  content: string;
  author: {
    id: string;
    name: string;
    avatar?: string;
    isAI: boolean;
  };
  timestamp: Date;
  votes: Vote[];
  isSelected: boolean;
  isViable: boolean;
  isHidden: boolean;
}

export interface Thread {
  id: string;
  title: string;
  branches: ConversationBranch[];
  dimensions: Dimension[];
  activeInterval: number | null; // null means no active interval (manually selecting)
}

// Custom hook for timer
const useInterval = (callback: () => void, delay: number | null) => {
  useEffect(() => {
    if (delay === null) {
      return;
    }
    const id = setInterval(callback, delay);
    return () => clearInterval(id);
  }, [callback, delay]);
};

// Component to display a vote slider for a specific dimension
function DimensionVoteSlider({
  dimension,
  _branchId,
  initialValue = 0,
  onVote,
}: {
  dimension: Dimension;
  _branchId: string;
  initialValue?: number;
  onVote: (dimensionId: string, value: number) => void;
}) {
  const [value, setValue] = useState(initialValue);

  return (
    <Box mb="xs">
      <Group mb={5} justify="space-between">
        <Text size="sm" fw={500}>
          {dimension.name}
        </Text>
        <Badge variant="light" size="sm">
          {value}
        </Badge>
      </Group>
      <Slider
        marks={[
          { value: -5, label: '-5' },
          { value: 0, label: '0' },
          { value: 5, label: '5' },
        ]}
        min={-5}
        max={5}
        step={1}
        value={value}
        onChange={(newValue) => {
          setValue(newValue);
          onVote(dimension.id, newValue);
        }}
      />
    </Box>
  );
}

// Component to display a single branch
function Branch({
  branch,
  dimensions,
  isMainBranch = false,
  onVote,
  onExpand,
  onSelect,
}: {
  branch: ConversationBranch;
  dimensions: Dimension[];
  isMainBranch?: boolean;
  onVote: (branchId: string, dimensionId: string, value: number) => void;
  onExpand?: () => void;
  onSelect?: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const { author, content, timestamp, isSelected, isViable } = branch;

  // Calculate average vote score across all dimensions
  const averageScore =
    branch.votes.length > 0
      ? branch.votes.reduce((sum, vote) => sum + vote.value, 0) / branch.votes.length
      : 0;

  // Format the timestamp
  const formattedTime = new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });

  const handleExpand = () => {
    setExpanded(!expanded);
    if (onExpand) {
      onExpand();
    }
  };

  // Display branch differently based on its status
  return (
    <Card
      withBorder
      shadow={isMainBranch ? 'md' : 'sm'}
      padding={isMainBranch ? 'md' : 'sm'}
      radius="md"
      mb="md"
      styles={{
        root: (theme: MantineTheme) => ({
          borderLeft: isSelected ? `${rem(4)} solid ${theme.colors.blue[5]}` : undefined,
          opacity: !isViable && !isSelected ? 0.7 : 1,
          backgroundColor: isMainBranch ? theme.colors.gray[0] : undefined,
          maxWidth: isMainBranch ? '100%' : '95%',
          marginLeft: isMainBranch ? 0 : 'auto',
        }),
      }}
    >
      <Group justify="space-between" mb="xs">
        <Group>
          <Avatar src={author.avatar} radius="xl" size="md" color={author.isAI ? 'blue' : 'red'}>
            {author.name.charAt(0)}
          </Avatar>
          <div>
            <Text fw={500}>{author.name}</Text>
            <Group gap="xs">
              <Text size="xs" color="dimmed">
                {formattedTime}
              </Text>
              {author.isAI && (
                <Badge size="xs" variant="outline" color="blue">
                  AI
                </Badge>
              )}
              {isSelected && (
                <Badge size="xs" color="green">
                  Selected
                </Badge>
              )}
              {!isSelected && isViable && (
                <Badge size="xs" color="yellow">
                  Viable
                </Badge>
              )}
            </Group>
          </div>
        </Group>

        <Group gap="xs">
          <Badge
            leftSection={
              averageScore > 0 ? (
                <IconThumbUp size={12} />
              ) : averageScore < 0 ? (
                <IconThumbDown size={12} />
              ) : null
            }
            color={averageScore > 0 ? 'green' : averageScore < 0 ? 'red' : 'gray'}
          >
            {averageScore.toFixed(1)}
          </Badge>

          {!isMainBranch && (
            <Tooltip label="Select this branch">
              <ActionIcon onClick={onSelect} variant="light" color="blue" disabled={isSelected}>
                <IconChevronRight size={18} />
              </ActionIcon>
            </Tooltip>
          )}
        </Group>
      </Group>

      <Text size={isMainBranch ? 'md' : 'sm'} mb="md">
        {content}
      </Text>

      {!isMainBranch && (
        <Accordion value={expanded ? 'votes' : null} onChange={() => handleExpand()}>
          <Accordion.Item value="votes">
            <Accordion.Control>Rate this response</Accordion.Control>
            <Accordion.Panel>
              <Stack gap="xs">
                {dimensions.map((dimension) => (
                  <DimensionVoteSlider
                    key={dimension.id}
                    dimension={dimension}
                    _branchId={branch.id}
                    onVote={(dimensionId, value) => onVote(branch.id, dimensionId, value)}
                  />
                ))}
              </Stack>
            </Accordion.Panel>
          </Accordion.Item>
        </Accordion>
      )}
    </Card>
  );
}

// Main component that displays the threaded conversation
export function ThreadedConversation({ thread }: { thread: Thread }) {
  const [activeThread, setActiveThread] = useState<Thread>(thread);
  const [timeUntilNextSelection, setTimeUntilNextSelection] = useState<number | null>(
    thread.activeInterval ? thread.activeInterval : null
  );

  // Update the timer every second
  useInterval(() => {
    if (timeUntilNextSelection !== null && timeUntilNextSelection > 0) {
      setTimeUntilNextSelection(timeUntilNextSelection - 1000);
    } else if (timeUntilNextSelection === 0) {
      // Auto-select the highest rated branch
      selectHighestRatedBranch();
      // Reset the timer
      setTimeUntilNextSelection(thread.activeInterval);
    }
  }, 1000);

  // Organize branches by their parent-child relationships
  const organizedBranches = organizeBranches(activeThread.branches);

  // Handler for voting on a branch
  const handleVote = (branchId: string, dimensionId: string, value: number) => {
    // Find the branch to update
    const updatedBranches = activeThread.branches.map((branch) => {
      if (branch.id === branchId) {
        // Check if there's already a vote for this dimension
        const existingVoteIndex = branch.votes.findIndex(
          (vote) => vote.dimension === dimensionId && vote.userId === 'current-user'
        );

        if (existingVoteIndex >= 0) {
          // Update existing vote
          const updatedVotes = [...branch.votes];
          updatedVotes[existingVoteIndex] = {
            ...updatedVotes[existingVoteIndex],
            value,
          };
          return { ...branch, votes: updatedVotes };
        }

        // Add new vote
        return {
          ...branch,
          votes: [...branch.votes, { userId: 'current-user', dimension: dimensionId, value }],
        };
      }
      return branch;
    });

    setActiveThread({
      ...activeThread,
      branches: updatedBranches,
    });
  };

  // Select a branch manually
  const handleSelectBranch = (branchId: string) => {
    // Mark the selected branch and update viable branches
    const updatedBranches = activeThread.branches.map((branch) => {
      if (branch.id === branchId) {
        return { ...branch, isSelected: true };
      } else if (branch.parentId === getParentIdForBranch(branchId)) {
        // For branches with the same parent, determine if they should remain viable
        const shouldRemainViable = calculateBranchScore(branch) > 0;
        return {
          ...branch,
          isSelected: false,
          isViable: shouldRemainViable,
          isHidden: !shouldRemainViable && branch.id !== branchId,
        };
      }
      return branch;
    });

    setActiveThread({
      ...activeThread,
      branches: updatedBranches,
    });

    // Reset the timer if we're on automatic mode
    if (activeThread.activeInterval !== null) {
      setTimeUntilNextSelection(activeThread.activeInterval);
    }
  };

  // Automatically select the highest rated branch among the current options
  const selectHighestRatedBranch = () => {
    // Get all viable branches that are not yet selected
    const currentLevelBranches = activeThread.branches.filter(
      (branch) => !branch.isSelected && !branch.isHidden && branch.isViable
    );

    if (currentLevelBranches.length === 0) {
      return;
    }

    // Find branch with highest score
    let highestRatedBranch = currentLevelBranches[0];
    let highestScore = calculateBranchScore(highestRatedBranch);

    currentLevelBranches.forEach((branch) => {
      const score = calculateBranchScore(branch);
      if (score > highestScore) {
        highestRatedBranch = branch;
        highestScore = score;
      }
    });

    // Select the highest rated branch
    handleSelectBranch(highestRatedBranch.id);
  };

  // Helper function to calculate a branch's score based on votes
  const calculateBranchScore = (branch: ConversationBranch): number => {
    if (branch.votes.length === 0) {
      return 0;
    }

    return branch.votes.reduce((sum, vote) => sum + vote.value, 0) / branch.votes.length;
  };

  // Helper to get the parent ID for a branch
  const getParentIdForBranch = (branchId: string): string | null => {
    const branch = activeThread.branches.find((b) => b.id === branchId);
    return branch ? branch.parentId : null;
  };

  // Helper function to organize branches into a threaded structure
  function organizeBranches(branches: ConversationBranch[]): ConversationBranch[][] {
    const result: ConversationBranch[][] = [];
    const rootBranches = branches.filter((b) => b.parentId === null);

    // Add root branches
    result.push(rootBranches);

    // Now build the thread by following selected branches
    let currentParentId: string | null = rootBranches.find((b) => b.isSelected)?.id || null;

    while (currentParentId !== null) {
      const childBranches = branches.filter(
        (b) => b.parentId === currentParentId && (!b.isHidden || b.isSelected)
      );

      if (childBranches.length === 0) {
        break;
      }

      result.push(childBranches);

      // Find the next selected branch
      const nextSelected = childBranches.find((b) => b.isSelected);
      currentParentId = nextSelected?.id || null;
    }

    return result;
  }

  // Format the time until next selection
  const formatTimeRemaining = (ms: number) => {
    const seconds = Math.floor(ms / 1000);
    return `${seconds}s`;
  };

  return (
    <Stack>
      <Paper p="md" withBorder>
        <Group justify="space-between">
          <Text size="xl" fw={700}>
            {activeThread.title}
          </Text>
          {timeUntilNextSelection !== null && (
            <Group gap="xs">
              <IconClock size={16} />
              <Text size="sm">
                Next selection in: {formatTimeRemaining(timeUntilNextSelection)}
              </Text>
            </Group>
          )}
        </Group>
      </Paper>

      {organizedBranches.map((levelBranches, level) => (
        <Box key={`level-${level}`} pl={level > 0 ? 20 : 0}>
          {/* Display selected branch for this level as main */}
          {levelBranches
            .filter((b) => b.isSelected)
            .map((branch) => (
              <Branch
                key={branch.id}
                branch={branch}
                dimensions={activeThread.dimensions}
                isMainBranch
                onVote={handleVote}
              />
            ))}

          {/* Display other branches (viable alternatives) */}
          {levelBranches
            .filter((b) => !b.isSelected && !b.isHidden)
            .map((branch) => (
              <Transition
                key={branch.id}
                mounted={!branch.isHidden}
                transition="fade"
                duration={400}
              >
                {(styles) => (
                  <div style={styles}>
                    <Branch
                      branch={branch}
                      dimensions={activeThread.dimensions}
                      onVote={handleVote}
                      onSelect={() => handleSelectBranch(branch.id)}
                    />
                  </div>
                )}
              </Transition>
            ))}
        </Box>
      ))}

      {activeThread.activeInterval !== null && (
        <Group justify="center" mt="md">
          <Button
            variant="outline"
            leftSection={<IconArrowBack size={16} />}
            onClick={() => {
              // Reset the timer
              setTimeUntilNextSelection(activeThread.activeInterval);
            }}
          >
            Reset Timer
          </Button>
          <Button
            color="blue"
            leftSection={<IconArrowForward size={16} />}
            onClick={selectHighestRatedBranch}
          >
            Select Top Branch Now
          </Button>
        </Group>
      )}
    </Stack>
  );
}
