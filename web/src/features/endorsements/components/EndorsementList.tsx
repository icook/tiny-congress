import { IconTrash } from '@tabler/icons-react';
import { ActionIcon, Card, Group, Stack, Text, Tooltip } from '@mantine/core';
import type { Endorsement } from '../types';

interface EndorsementListProps {
  endorsements: Endorsement[];
  onRevoke: (subjectId: string) => void;
  isRevoking: boolean;
}

export function EndorsementList({ endorsements, onRevoke, isRevoking }: EndorsementListProps) {
  const activeEndorsements = endorsements.filter((e) => e.topic === 'trust' && !e.revoked);

  if (activeEndorsements.length === 0) {
    return (
      <Text c="dimmed" ta="center" py="lg">
        No endorsements yet. Use the Give tab to endorse someone.
      </Text>
    );
  }

  return (
    <Stack gap="xs">
      {activeEndorsements.map((e) => (
        <Card key={e.id} padding="sm" withBorder>
          <Group justify="space-between" wrap="nowrap">
            <div>
              <Text size="sm" fw={500}>
                {e.subject_id}
              </Text>
              <Text size="xs" c="dimmed">
                {new Date(e.created_at).toLocaleDateString()}
              </Text>
            </div>
            <Tooltip label="Revoke endorsement">
              <ActionIcon
                variant="subtle"
                color="red"
                onClick={() => {
                  onRevoke(e.subject_id);
                }}
                loading={isRevoking}
              >
                <IconTrash size={16} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </Card>
      ))}
    </Stack>
  );
}
