import { Badge, Card, Group, Stack, Text, Title } from '@mantine/core';
import { useAuth } from '../auth/AuthProvider';

export function DashboardPage() {
  const { user } = useAuth();

  return (
    <Stack gap="md">
      <Group justify="space-between" align="flex-start">
        <div>
          <Title order={2}>Welcome back{user ? `, ${user.name}` : ''}</Title>
          <Text c="dimmed" size="sm">
            Your OAuth session is active. Explore rooms, dashboards, and conversations without
            re-authenticating.
          </Text>
        </div>
        <Badge color="green" variant="light">
          Signed in
        </Badge>
      </Group>

      <Card shadow="sm" withBorder radius="md" p="lg">
        <Stack gap="xs">
          <Text fw={600}>Next up</Text>
          <Text c="dimmed">
            This dashboard will summarize your active rooms and invitations. For now, use the
            navigation to jump into conversations or account settings.
          </Text>
        </Stack>
      </Card>
    </Stack>
  );
}
