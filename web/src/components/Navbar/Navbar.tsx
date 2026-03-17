import {
  IconBook2,
  IconCode,
  IconDoor,
  IconHeartHandshake,
  IconHome2,
  IconInfoCircle,
  IconKey,
  IconLogin,
  IconShieldCheck,
  IconShieldHalfFilled,
  IconUserPlus,
} from '@tabler/icons-react';
import { Link, useRouterState } from '@tanstack/react-router';
import { Badge, Box, NavLink, Stack, useComputedColorScheme, useMantineTheme } from '@mantine/core';
import { useTrustScores } from '@/features/trust';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

const topNavLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconDoor, label: 'Rooms', path: '/rooms' },
];

const authNavLinks = [
  { icon: IconShieldHalfFilled, label: 'Trust', path: '/trust' },
  { icon: IconHeartHandshake, label: 'Endorse', path: '/endorse' },
];

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
  const trustScoresQuery = useTrustScores(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;
  const isAuthenticated = Boolean(deviceKid);
  const trustScore = trustScoresQuery.data?.[0] ?? null;

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

        {isAuthenticated
          ? authNavLinks.map((link) => (
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
            ))
          : null}
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
          {isVerified && trustScore ? (
            (() => {
              if (trustScore.distance <= 3.0 && trustScore.path_diversity >= 2) {
                return (
                  <Badge color="violet" leftSection={<IconShieldCheck size={14} />} variant="light">
                    Congress
                  </Badge>
                );
              }
              if (trustScore.distance <= 6.0 && trustScore.path_diversity >= 1) {
                return (
                  <Badge color="blue" leftSection={<IconShieldCheck size={14} />} variant="light">
                    Community
                  </Badge>
                );
              }
              return (
                <Badge color="green" leftSection={<IconShieldCheck size={14} />} variant="light">
                  Verified
                </Badge>
              );
            })()
          ) : isVerified ? (
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
