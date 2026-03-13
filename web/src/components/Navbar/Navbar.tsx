import {
  IconCode,
  IconDoor,
  IconHeartHandshake,
  IconHome2,
  IconInfoCircle,
  IconLogin,
  IconShieldCheck,
  IconUserPlus,
} from '@tabler/icons-react';
import { Link, useRouterState } from '@tanstack/react-router';
import { Badge, Box, NavLink, Stack, useComputedColorScheme, useMantineTheme } from '@mantine/core';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

const publicNavLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconDoor, label: 'Rooms', path: '/rooms' },
  { icon: IconInfoCircle, label: 'About', path: '/about' },
  { icon: IconCode, label: 'Dev Docs', path: '/dev' },
];

const authNavLinks = [{ icon: IconHeartHandshake, label: 'Endorse', path: '/endorse' }];

const guestLinks = [
  { icon: IconLogin, label: 'Login', path: '/login' },
  { icon: IconUserPlus, label: 'Sign Up', path: '/signup' },
];

interface NavbarProps {
  onNavigate?: () => void;
}

export function Navbar({ onNavigate }: NavbarProps) {
  const { deviceKid, privateKey, username } = useDevice();
  const currentPath = useRouterState({
    select: (state) => state.location.pathname,
  });
  const theme = useMantineTheme();
  const colorScheme = useComputedColorScheme();
  const { crypto } = useCrypto();
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;
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
        {[...publicNavLinks, ...(isAuthenticated ? authNavLinks : [])].map((link) => (
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
          {isVerified ? (
            <Badge color="green" leftSection={<IconShieldCheck size={14} />} variant="light">
              Verified
            </Badge>
          ) : (
            (() => {
              const url = buildVerifierUrl(username ?? '');
              if (url) {
                return (
                  <Badge
                    component="a"
                    href={url}
                    color="yellow"
                    variant="light"
                    style={{ cursor: 'pointer', textDecoration: 'none' }}
                  >
                    Unverified — click to verify
                  </Badge>
                );
              }
              return null;
            })()
          )}
        </Box>
      )}
    </Stack>
  );
}
