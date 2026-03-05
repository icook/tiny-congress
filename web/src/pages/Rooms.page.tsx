/**
 * Rooms page - Lists open rooms and their polls
 */

import { IconAlertTriangle, IconDoor } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Alert, Badge, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
import { usePolls, useRooms, type Room } from '@/features/rooms';

const ROOM_STATUS_COLOR: Record<string, string> = {
  open: 'green',
  closed: 'gray',
  archived: 'gray',
};

export function RoomsPage() {
  const roomsQuery = useRooms();

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <div>
        <Title order={2}>Rooms</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Browse open rooms and participate in polls
        </Text>
      </div>

      {roomsQuery.isLoading ? <Loader size="sm" /> : null}

      {roomsQuery.isError ? (
        <Alert icon={<IconAlertTriangle size={16} />} color="red">
          Failed to load rooms: {roomsQuery.error.message}
        </Alert>
      ) : null}

      {roomsQuery.data?.length === 0 ? (
        <Alert icon={<IconDoor size={16} />} color="blue">
          No rooms are open right now — check back soon.
        </Alert>
      ) : null}

      {roomsQuery.data?.map((room) => (
        <RoomCard key={room.id} room={room} />
      ))}
    </Stack>
  );
}

function RoomCard({ room }: { room: Room }) {
  const pollsQuery = usePolls(room.id);

  const activePolls = pollsQuery.data?.filter((p) => p.status === 'active') ?? [];
  const statusColor = ROOM_STATUS_COLOR[room.status] ?? 'yellow';

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Stack gap="sm">
        <Group justify="space-between">
          <Title order={4}>{room.name}</Title>
          <Badge color={statusColor} variant="light">
            {room.status}
          </Badge>
        </Group>

        {room.description ? (
          <Text size="sm" c="dimmed">
            {room.description}
          </Text>
        ) : null}

        {pollsQuery.isLoading ? (
          <Text size="sm" c="dimmed">
            Loading polls...
          </Text>
        ) : null}

        {activePolls.length > 0 ? (
          <Stack gap="xs">
            <Text size="sm" fw={500}>
              Active polls ({String(activePolls.length)})
            </Text>
            {activePolls.map((poll) => (
              <Card
                key={poll.id}
                component={Link}
                to={`/rooms/${room.id}/polls/${poll.id}`}
                padding="sm"
                radius="sm"
                withBorder
                style={{ cursor: 'pointer', textDecoration: 'none' }}
              >
                <Text size="sm">{poll.question}</Text>
              </Card>
            ))}
          </Stack>
        ) : pollsQuery.data ? (
          <Text size="sm" c="dimmed">
            No active polls
          </Text>
        ) : null}
      </Stack>
    </Card>
  );
}
