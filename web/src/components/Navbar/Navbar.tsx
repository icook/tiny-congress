import {
  IconCalendarStats,
  IconDeviceDesktopAnalytics,
  IconFingerprint,
  IconGauge,
  IconHome2,
  IconInfoCircle,
  IconLogin,
  IconLogout,
  IconMessages,
  IconSettings,
  IconUser,
  IconUserPlus,
} from '@tabler/icons-react';
import { Link, useNavigate, useRouterState } from '@tanstack/react-router';
import {
  Box,
  Button,
  Group,
  Image,
  NavLink,
  Stack,
  Text,
  useComputedColorScheme,
  useMantineTheme,
} from '@mantine/core';
import { useSession } from '../../features/identity/state/session';

const mainNavLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconGauge, label: 'Dashboard', path: '/dashboard' },
  { icon: IconMessages, label: 'Conversations', path: '/conversations' },
  { icon: IconInfoCircle, label: 'About', path: '/about' },
  { icon: IconDeviceDesktopAnalytics, label: 'Analytics', path: '/analytics' },
  { icon: IconCalendarStats, label: 'Releases', path: '/releases' },
];

const authNavLinks = [
  { icon: IconUser, label: 'Account', path: '/account' },
  { icon: IconFingerprint, label: 'Security', path: '/security' },
  { icon: IconSettings, label: 'Settings', path: '/settings' },
];

const guestNavLinks = [
  { icon: IconLogin, label: 'Login', path: '/login' },
  { icon: IconUserPlus, label: 'Sign Up', path: '/signup' },
];

export function Navbar() {
  const currentPath = useRouterState({
    select: (state) => state.location.pathname,
  });
  const navigate = useNavigate();
  const { isAuthenticated, logout } = useSession();
  const theme = useMantineTheme();
  const colorScheme = useComputedColorScheme();

  const borderColor = colorScheme === 'dark' ? theme.colors.dark[4] : theme.colors.gray[3];

  const isActive = (path: string) =>
    path === currentPath || (path !== '/' && currentPath.startsWith(path));

  const handleLogout = async () => {
    await logout();
    void navigate({ to: '/' });
  };

  const renderNavLinks = (links: typeof mainNavLinks) =>
    links.map((link) => (
      <NavLink
        key={link.label}
        component={Link}
        to={link.path}
        label={link.label}
        leftSection={<link.icon size={18} stroke={1.5} />}
        active={isActive(link.path)}
        fw={500}
      />
    ));

  return (
    <Stack
      component="nav"
      h="100%"
      gap="md"
      p="md"
      bg="var(--mantine-color-body)"
      style={{ borderRight: `1px solid ${borderColor}` }}
    >
      <Box pb="sm" mb="xs" style={{ borderBottom: `1px solid ${borderColor}` }}>
        <Group gap="xs">
          <Image src="/src/logo.png" alt="TinyCongress logo" h={32} w="auto" fit="contain" />
          <Text fw={700} c="dimmed">
            TinyCongress
          </Text>
        </Group>
      </Box>

      <Stack gap={4} style={{ flex: 1 }}>
        {renderNavLinks(mainNavLinks)}
      </Stack>

      <Stack gap={4} pt="sm" style={{ borderTop: `1px solid ${borderColor}` }}>
        {isAuthenticated ? (
          <>
            {renderNavLinks(authNavLinks)}
            <Button
              variant="subtle"
              color="red"
              leftSection={<IconLogout size={18} stroke={1.5} />}
              justify="flex-start"
              onClick={handleLogout}
              fw={500}
            >
              Logout
            </Button>
          </>
        ) : (
          renderNavLinks(guestNavLinks)
        )}
      </Stack>
    </Stack>
  );
}
