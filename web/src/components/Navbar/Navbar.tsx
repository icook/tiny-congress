import {
  IconBook2,
  IconCode,
  IconDoor,
  IconHome2,
  IconInfoCircle,
  IconKey,
  IconList,
  IconLogin,
  IconUserPlus,
} from '@tabler/icons-react';
import { Link, useRouterState } from '@tanstack/react-router';
import { Box, NavLink, Stack, Text, useComputedColorScheme, useMantineTheme } from '@mantine/core';
import { PollCountdown, usePollCountdown, usePollDetail, useRooms } from '@/features/rooms';
import { useDevice } from '@/providers/DeviceProvider';
import { UserAccordion } from './UserAccordion';

const guestLinks = [
  { icon: IconLogin, label: 'Login', path: '/login' },
  { icon: IconUserPlus, label: 'Sign Up', path: '/signup' },
];

interface NavbarProps {
  onNavigate?: () => void;
}

export function Navbar({ onNavigate }: NavbarProps) {
  const { deviceKid } = useDevice();
  const currentPath = useRouterState({
    select: (state) => state.location.pathname,
  });
  const pollRouteMatch = /^\/rooms\/([^/]+)\/polls\/([^/]+)/.exec(currentPath);
  const pollRouteRoomId = pollRouteMatch?.[1];
  const pollRoutePollId = pollRouteMatch?.[2];
  const theme = useMantineTheme();
  const colorScheme = useComputedColorScheme();
  const isAuthenticated = Boolean(deviceKid);

  const borderColor = colorScheme === 'dark' ? theme.colors.dark[4] : theme.colors.gray[3];

  const isActive = (path: string) =>
    path === currentPath || (path !== '/' && currentPath.startsWith(path));

  return (
    <Stack
      component="nav"
      h="100%"
      gap="md"
      p="md"
      bg="var(--mantine-color-body)"
      style={{ borderRight: `1px solid ${borderColor}` }}
    >
      <Stack gap={4} style={{ flex: 1 }}>
        <NavLink
          component={Link}
          to="/"
          label="Home"
          leftSection={<IconHome2 size={18} stroke={1.5} />}
          active={isActive('/')}
          onClick={onNavigate}
          fw={500}
        />

        <RoomsAccordion isActive={isActive} onNavigate={onNavigate} />

        <NavLink
          component={Link}
          to="/docs"
          label="Docs"
          leftSection={<IconBook2 size={18} stroke={1.5} />}
          active={isActive('/docs')}
          defaultOpened={
            currentPath.startsWith('/about') ||
            currentPath.startsWith('/keys') ||
            currentPath.startsWith('/dev')
          }
          fw={500}
        >
          <NavLink
            component={Link}
            to="/about"
            label="About"
            leftSection={<IconInfoCircle size={16} stroke={1.5} />}
            active={isActive('/about')}
            onClick={onNavigate}
          />
          <NavLink
            component={Link}
            to="/keys"
            label="How Keys Work"
            leftSection={<IconKey size={16} stroke={1.5} />}
            active={isActive('/keys')}
            onClick={onNavigate}
          />
          <NavLink
            component={Link}
            to="/dev"
            label="Dev Docs"
            leftSection={<IconCode size={16} stroke={1.5} />}
            active={isActive('/dev')}
            defaultOpened={currentPath.startsWith('/dev/')}
          >
            <NavLink
              component={Link}
              to="/dev/architecture"
              label="Architecture"
              active={isActive('/dev/architecture')}
              onClick={onNavigate}
            />
            <NavLink
              component={Link}
              to="/dev/domain-model"
              label="Domain Model"
              active={isActive('/dev/domain-model')}
              onClick={onNavigate}
            />
          </NavLink>
        </NavLink>
      </Stack>

      {pollRouteRoomId && pollRoutePollId ? (
        <NavbarPollCountdown roomId={pollRouteRoomId} pollId={pollRoutePollId} />
      ) : null}

      {!isAuthenticated ? (
        <Box pt="sm" style={{ borderTop: `1px solid ${borderColor}` }}>
          <Stack gap={4}>
            {guestLinks.map((link) => (
              <NavLink
                key={link.label}
                component={Link}
                to={link.path}
                label={link.label}
                leftSection={<link.icon size={18} stroke={1.5} />}
                active={isActive(link.path)}
                onClick={onNavigate}
                fw={500}
              />
            ))}
          </Stack>
        </Box>
      ) : (
        <Box pt="sm" style={{ borderTop: `1px solid ${borderColor}` }}>
          <UserAccordion onNavigate={onNavigate} />
        </Box>
      )}
    </Stack>
  );
}

function NavbarPollCountdown({ roomId, pollId }: { roomId: string; pollId: string }) {
  const detailQuery = usePollDetail(roomId, pollId);
  const { secondsLeft } = usePollCountdown(detailQuery.data?.poll);

  if (secondsLeft === null) {
    return null;
  }

  return (
    <Box px="sm" py="xs">
      <Text size="xs" c="dimmed" mb={2}>
        Active poll
      </Text>
      <PollCountdown secondsLeft={secondsLeft} />
    </Box>
  );
}

function RoomsAccordion({
  isActive,
  onNavigate,
}: {
  isActive: (path: string) => boolean;
  onNavigate?: () => void;
}) {
  const { data: rooms } = useRooms();
  const currentPath = useRouterState({
    select: (state) => state.location.pathname,
  });

  return (
    <NavLink
      label="Rooms"
      leftSection={<IconDoor size={18} stroke={1.5} />}
      active={isActive('/rooms')}
      defaultOpened={currentPath.startsWith('/rooms')}
      fw={500}
    >
      <NavLink
        component={Link}
        to="/rooms"
        label="All Rooms"
        leftSection={<IconList size={16} stroke={1.5} />}
        active={currentPath === '/rooms'}
        onClick={onNavigate}
      />
      {rooms?.map((room) => (
        <NavLink
          key={room.id}
          component={Link}
          to={`/rooms/${room.id}`}
          label={room.name}
          active={isActive(`/rooms/${room.id}`)}
          onClick={onNavigate}
        />
      ))}
    </NavLink>
  );
}
