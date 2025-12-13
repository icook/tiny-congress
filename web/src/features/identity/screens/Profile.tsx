/**
 * Profile screen
 * Display account information, tier, reputation, and endorsements
 */

import { Alert, Badge, Card, Group, Progress, Stack, Text, Title } from '@mantine/core';
import { IconAlertTriangle, IconShield, IconStar, IconUser } from '@tabler/icons-react';
import { useSession } from '../state/session';

export function Profile() {
  const { session } = useSession();

  if (!session) {
    return (
      <Alert icon={<IconAlertTriangle size={16} />} title="Not authenticated" color="red">
        Please log in to view your profile
      </Alert>
    );
  }

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <Group gap="xs">
        <IconUser size={24} />
        <Title order={2}>Account Profile</Title>
      </Group>

      {/* Account Info */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Group justify="space-between">
            <div>
              <Text size="sm" c="dimmed">
                Account ID
              </Text>
              <Text fw={500} ff="monospace" size="sm">
                {session.accountId}
              </Text>
            </div>
            <Badge color="blue" size="lg">
              Tier 0
            </Badge>
          </Group>

          <div>
            <Text size="sm" c="dimmed">
              Current Device
            </Text>
            <Text fw={500} ff="monospace" size="sm">
              {session.deviceId}
            </Text>
          </div>

          <div>
            <Text size="sm" c="dimmed">
              Session Expires
            </Text>
            <Text fw={500} size="sm">
              {session.expiresAt.toLocaleString()}
            </Text>
          </div>
        </Stack>
      </Card>

      {/* Security Posture */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Group gap="xs">
            <IconShield size={20} />
            <Text fw={500}>Security Posture</Text>
          </Group>

          <div>
            <Group justify="space-between" mb="xs">
              <Text size="sm">Security Score</Text>
              <Text size="sm" fw={500}>
                75%
              </Text>
            </Group>
            <Progress value={75} color="green" />
          </div>

          <Alert color="blue">
            <Text size="sm">
              Profile data endpoints not yet implemented. This will show real security posture,
              tier information, and reputation once the backend endpoints are ready.
            </Text>
          </Alert>
        </Stack>
      </Card>

      {/* Reputation */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Group gap="xs">
            <IconStar size={20} />
            <Text fw={500}>Reputation</Text>
          </Group>

          <Group justify="space-between">
            <Text size="sm" c="dimmed">
              Overall Score
            </Text>
            <Badge color="yellow" size="lg">
              0
            </Badge>
          </Group>

          <Text size="sm" c="dimmed">
            Reputation is calculated from endorsements you've received on various topics.
          </Text>
        </Stack>
      </Card>

      {/* Endorsements by Topic */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Text fw={500}>Endorsements by Topic</Text>

          <Alert color="blue">
            <Text size="sm">
              No endorsements yet. Endorsement aggregation will appear here once you receive
              endorsements from other users.
            </Text>
          </Alert>
        </Stack>
      </Card>
    </Stack>
  );
}
