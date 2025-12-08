import {
  IconCalendarStats,
  IconDeviceDesktopAnalytics,
  IconFingerprint,
  IconGauge,
  IconHome2,
  IconMessages,
  IconSettings,
  IconUser,
} from '@tabler/icons-react';
import { Link, useRouterState } from '@tanstack/react-router';
import { Box, Group, Text, UnstyledButton } from '@mantine/core';
import classes from './Navbar.module.css';

const navLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconGauge, label: 'Dashboard', path: '/dashboard' },
  { icon: IconMessages, label: 'Conversations', path: '/conversations' },
  { icon: IconDeviceDesktopAnalytics, label: 'Analytics', path: '/analytics' },
  { icon: IconCalendarStats, label: 'Releases', path: '/releases' },
  { icon: IconUser, label: 'Account', path: '/account' },
  { icon: IconFingerprint, label: 'Security', path: '/security' },
  { icon: IconSettings, label: 'Settings', path: '/settings' },
];

export function Navbar() {
  const currentPath = useRouterState({
    select: (state) => state.location.pathname,
  });

  const links = navLinks.map((link) => {
    const isActive =
      link.path === currentPath || (link.path !== '/' && currentPath.startsWith(link.path));

    return (
      <UnstyledButton
        component={Link}
        to={link.path}
        className={classes.navLink}
        data-active={isActive || undefined}
        key={link.label}
      >
        <Group gap="sm">
          <link.icon size={20} stroke={1.5} />
          <Text>{link.label}</Text>
        </Group>
      </UnstyledButton>
    );
  });

  return (
    <nav className={classes.navbar}>
      <div className={classes.header}>
        <Box p="md">
          <img src="/src/logo.png" alt="Logo" className={classes.logo} />
        </Box>
      </div>

      <div className={classes.navLinks}>{links}</div>
    </nav>
  );
}
