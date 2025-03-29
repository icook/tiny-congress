import { useState } from 'react';
import { Link, useLocation } from 'react-router-dom';
import {
  IconCalendarStats,
  IconDeviceDesktopAnalytics,
  IconFingerprint,
  IconGauge,
  IconHome2,
  IconSettings,
  IconUser,
} from '@tabler/icons-react';
import { Title, Tooltip, UnstyledButton } from '@mantine/core';
import { MantineLogo } from '@mantinex/mantine-logo';
import classes from './DoubleNavbar.module.css';

const mainLinksMockdata = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconGauge, label: 'Dashboard', path: '/dashboard' },
  { icon: IconDeviceDesktopAnalytics, label: 'Analytics', path: '/analytics' },
  { icon: IconCalendarStats, label: 'Releases', path: '/releases' },
  { icon: IconUser, label: 'Account', path: '/account' },
  { icon: IconFingerprint, label: 'Security', path: '/security' },
  { icon: IconSettings, label: 'Settings', path: '/settings' },
];

const linksMockdata = [
  { label: 'Security', path: '/security' },
  { label: 'Settings', path: '/settings' },
  { label: 'Dashboard', path: '/dashboard' },
  { label: 'Releases', path: '/releases' },
  { label: 'Account', path: '/account' },
  { label: 'Orders', path: '/orders' },
  { label: 'Clients', path: '/clients' },
  { label: 'Databases', path: '/databases' },
  { label: 'Pull Requests', path: '/pull-requests' },
  { label: 'Open Issues', path: '/issues' },
  { label: 'Wiki pages', path: '/wiki' },
];

export function DoubleNavbar() {
  const location = useLocation();
  const currentPath = location.pathname;
  
  // Find active main link based on current path
  const currentMainLink = mainLinksMockdata.find(link => 
    link.path === currentPath || 
    (link.path !== '/' && currentPath.startsWith(link.path))
  ) || mainLinksMockdata[0];

  const [active, setActive] = useState(currentMainLink.label);

  const mainLinks = mainLinksMockdata.map((link) => (
    <Tooltip
      label={link.label}
      position="right"
      withArrow
      transitionProps={{ duration: 0 }}
      key={link.label}
    >
      <UnstyledButton
        component={Link}
        to={link.path}
        onClick={() => setActive(link.label)}
        className={classes.mainLink}
        data-active={link.label === active || undefined}
      >
        <link.icon size={22} stroke={1.5} />
      </UnstyledButton>
    </Tooltip>
  ));

  const links = linksMockdata.map((link) => (
    <Link
      className={classes.link}
      data-active={currentPath === link.path || undefined}
      to={link.path}
      key={link.label}
    >
      {link.label}
    </Link>
  ));

  return (
    <nav className={classes.navbar}>
      <div className={classes.wrapper}>
        <div className={classes.aside}>
          <div className={classes.logo}>
            <MantineLogo type="mark" size={30} />
          </div>
          {mainLinks}
        </div>
        <div className={classes.main}>
          <Title order={4} className={classes.title}>
            {active}
          </Title>

          {links}
        </div>
      </div>
    </nav>
  );
}
