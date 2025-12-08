import {
  IconChevronDown,
  IconLogout,
  IconUser,
  IconUserCheck,
  IconUserQuestion,
} from '@tabler/icons-react';
import { Link, useNavigate, useRouterState } from '@tanstack/react-router';
import { Avatar, Badge, Button, Group, Loader, Menu, Stack, Text } from '@mantine/core';
import { useAuth } from '../../auth/AuthProvider';

export function UserMenu() {
  const { status, user, logout, error } = useAuth();
  const navigate = useNavigate();
  const location = useRouterState({ select: (state) => state.location });

  const nextPath = buildNextPath(location.pathname, location.search);

  if (status === 'unauthenticated' || status === 'error') {
    return (
      <Group gap="xs">
        {status === 'error' && error ? (
          <Text size="sm" c="red">
            {error}
          </Text>
        ) : null}
        <Button
          size="sm"
          radius="md"
          variant="gradient"
          gradient={{ from: 'indigo', to: 'violet' }}
          onClick={() => navigate({ to: '/login', search: { next: nextPath } })}
        >
          Sign in
        </Button>
      </Group>
    );
  }

  if (status === 'authenticating') {
    return (
      <Group gap={6}>
        <Loader size="xs" />
        <Text size="sm" c="dimmed">
          Connecting...
        </Text>
      </Group>
    );
  }

  return (
    <Menu width={230} position="bottom-end" shadow="md" withArrow>
      <Menu.Target>
        <Button
          variant="subtle"
          radius="xl"
          size="sm"
          rightSection={<IconChevronDown size={16} />}
          leftSection={
            <Avatar radius="xl" size="sm" color="indigo">
              {(user?.name ?? user?.email ?? '?').charAt(0).toUpperCase()}
            </Avatar>
          }
        >
          <Group gap={4} wrap="nowrap">
            <Text fw={600} size="sm">
              {user?.name ?? 'Signed in'}
            </Text>
            {user?.email && (
              <Text size="xs" c="dimmed">
                {user.email}
              </Text>
            )}
          </Group>
        </Button>
      </Menu.Target>

      <Menu.Dropdown>
        <Menu.Label>
          <Stack gap={2}>
            <Group gap={6}>
              <IconUserCheck size={14} />
              <Text size="xs" fw={600}>
                Connected via OAuth
              </Text>
            </Group>
            <Text size="xs" c="dimmed">
              Manage your session or jump to account settings.
            </Text>
          </Stack>
        </Menu.Label>

        <Menu.Item
          component={Link}
          to="/account"
          leftSection={<IconUser size={16} />}
          rightSection={<Badge size="xs">Profile</Badge>}
        >
          Account
        </Menu.Item>
        <Menu.Item
          component={Link}
          to="/settings"
          leftSection={<IconUserQuestion size={16} />}
        >
          Session preferences
        </Menu.Item>
        <Menu.Divider />
        <Menu.Item color="red" leftSection={<IconLogout size={16} />} onClick={logout}>
          Sign out
        </Menu.Item>
      </Menu.Dropdown>
    </Menu>
  );
}

function buildNextPath(pathname: string, search: Record<string, unknown>) {
  const params = new URLSearchParams();
  Object.entries(search ?? {}).forEach(([key, value]) => {
    if (value === undefined || value === null) {
      return;
    }
    params.set(key, String(value));
  });

  const searchStr = params.toString();
  return searchStr ? `${pathname}?${searchStr}` : pathname;
}
