import {
  IconBook2,
  IconCode,
  IconDoor,
  IconHome2,
  IconInfoCircle,
  IconKey,
  IconLogin,
  IconUserPlus,
} from '@tabler/icons-react';
import { Link, useRouterState } from '@tanstack/react-router';
import { Box, NavLink, Stack, useComputedColorScheme, useMantineTheme } from '@mantine/core';
import { useDevice } from '@/providers/DeviceProvider';
import { UserAccordion } from './UserAccordion';

const topNavLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconDoor, label: 'Rooms', path: '/rooms' },
];

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
        {topNavLinks.map((link) => (
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
