import { useState } from 'react';
import {
  IconCalendarStats,
  IconDeviceDesktopAnalytics,
  IconFingerprint,
  IconGauge,
  IconHome2,
  IconSettings,
  IconUser,
  IconMessages,
} from '@tabler/icons-react';
import { Link, useLocation } from 'react-router-dom';
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
  const location = useLocation();
  const currentPath = location.pathname;

  // Find active link based on current path
  const currentLink =
    navLinks.find(
      (link) => link.path === currentPath || (link.path !== '/' && currentPath.startsWith(link.path))
    ) || navLinks[0];

  const [active, setActive] = useState(currentLink.label);

  const links = navLinks.map((link) => (
    <UnstyledButton
      component={Link}
      to={link.path}
      onClick={() => setActive(link.label)}
      className={classes.navLink}
      data-active={link.label === active || currentPath === link.path || undefined}
      key={link.label}
    >
      <Group gap="sm">
        <link.icon size={20} stroke={1.5} />
        <Text>{link.label}</Text>
      </Group>
    </UnstyledButton>
  ));

  return (
    <nav className={classes.navbar}>
      <div className={classes.header}>
        <Box p="md">
          <img src="/src/logo.png" alt="Logo" className={classes.logo} />
        </Box>
      </div>
      
      <div className={classes.navLinks}>
        {links}
      </div>
    </nav>
  );
}