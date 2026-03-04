import {
  IconCalendarStats,
  IconDeviceDesktopAnalytics,
  IconDoor,
  IconGauge,
  IconHome2,
  IconInfoCircle,
  IconLogin,
  IconLogout,
  IconMessages,
  IconSettings,
  IconShieldCheck,
  IconUserPlus,
} from '@tabler/icons-react';
import { Link, useNavigate, useRouterState } from '@tanstack/react-router';
import { Badge, Box, NavLink, Stack, useComputedColorScheme, useMantineTheme } from '@mantine/core';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

const navLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconGauge, label: 'Dashboard', path: '/dashboard' },
  { icon: IconDoor, label: 'Rooms', path: '/rooms' },
  { icon: IconMessages, label: 'Conversations', path: '/conversations' },
  { icon: IconInfoCircle, label: 'About', path: '/about' },
  { icon: IconDeviceDesktopAnalytics, label: 'Analytics', path: '/analytics' },
  { icon: IconCalendarStats, label: 'Releases', path: '/releases' },
];

const guestLinks = [
  { icon: IconLogin, label: 'Login', path: '/login' },
  { icon: IconUserPlus, label: 'Sign Up', path: '/signup' },
];

const authedLinks = [{ icon: IconSettings, label: 'Settings', path: '/settings' }];

export function Navbar() {
  const { deviceKid, privateKey, clearDevice } = useDevice();
  const navigate = useNavigate();
  const currentPath = useRouterState({
    select: (state) => state.location.pathname,
  });
  const theme = useMantineTheme();
  const colorScheme = useComputedColorScheme();
  const { crypto } = useCrypto();
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;
  const isAuthenticated = Boolean(deviceKid);

  const authLinks = isAuthenticated ? authedLinks : guestLinks;

  const handleLogout = () => {
    clearDevice();
    void navigate({ to: '/' });
  };

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
        {navLinks.map((link) => (
          <NavLink
            key={link.label}
            component={Link}
            to={link.path}
            label={link.label}
            leftSection={<link.icon size={18} stroke={1.5} />}
            active={isActive(link.path)}
            fw={500}
          />
        ))}
      </Stack>

      <Box pt="sm" style={{ borderTop: `1px solid ${borderColor}` }}>
        <Stack gap={4}>
          {authLinks.map((link) => (
            <NavLink
              key={link.label}
              component={Link}
              to={link.path}
              label={link.label}
              leftSection={<link.icon size={18} stroke={1.5} />}
              active={isActive(link.path)}
              fw={500}
            />
          ))}
          {isAuthenticated ? (
            isVerified ? (
              <Badge
                color="green"
                leftSection={<IconShieldCheck size={14} />}
                variant="light"
                mt="xs"
              >
                Verified
              </Badge>
            ) : (
              (() => {
                const url = buildVerifierUrl('');
                if (url) {
                  return (
                    <Badge
                      component="a"
                      href={url}
                      color="yellow"
                      variant="light"
                      mt="xs"
                      style={{ cursor: 'pointer', textDecoration: 'none' }}
                    >
                      Unverified
                    </Badge>
                  );
                }
                return null;
              })()
            )
          ) : null}
          {isAuthenticated ? (
            <NavLink
              label="Logout"
              leftSection={<IconLogout size={18} stroke={1.5} />}
              onClick={handleLogout}
              fw={500}
            />
          ) : null}
        </Stack>
      </Box>
    </Stack>
  );
}
