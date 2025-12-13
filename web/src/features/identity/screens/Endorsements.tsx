/**
 * Endorsements screen
 * Create and manage endorsements
 */

import { Alert, Card, Stack, Text, Title } from '@mantine/core';
import { IconThumbUp } from '@tabler/icons-react';
import { EndorsementEditor } from '../components/EndorsementEditor';
import { useSession } from '../state/session';

export function Endorsements() {
  const { session } = useSession();

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <div>
        <Title order={2}>
          <IconThumbUp style={{ display: 'inline', marginRight: 8 }} size={24} />
          Endorsements
        </Title>
        <Text c="dimmed" size="sm" mt="xs">
          Create and manage your endorsements
        </Text>
      </div>

      {session ? (
        <>
          {/* Endorsement Editor */}
          <EndorsementEditor />

          {/* My Endorsements List */}
          <Card shadow="sm" padding="lg" radius="md" withBorder>
            <Stack gap="md">
              <Text fw={500}>My Endorsements</Text>

              <Alert color="blue">
                <Text size="sm">
                  Endorsement list endpoint not yet implemented. This will show all endorsements
                  you've created once the backend endpoint is ready.
                </Text>
              </Alert>
            </Stack>
          </Card>
        </>
      ) : (
        <Alert color="red" title="Not authenticated">
          Please log in to create endorsements
        </Alert>
      )}
    </Stack>
  );
}
