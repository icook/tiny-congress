import { IconMoon, IconSun } from '@tabler/icons-react';
import { Outlet } from '@tanstack/react-router';
import {
  ActionIcon,
  AppShell,
  Badge,
  Burger,
  Group,
  Image,
  Text,
  useComputedColorScheme,
  useMantineColorScheme,
} from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import logoDark from '@/logo-dark.svg';
import logoLight from '@/logo-light.svg';
import { Navbar } from '../components/Navbar/Navbar';
import { getEnvironment } from '../config';

const ENV_BADGE_CONFIG: Record<string, { color: string; label: string }> = {
  demo: { color: 'blue', label: 'DEMO' },
  staging: { color: 'orange', label: 'STAGING' },
  development: { color: 'green', label: 'DEV' },
};

function EnvironmentBadge() {
  const env = getEnvironment();
  if (env === 'production') {
    return null;
  }
  const config = ENV_BADGE_CONFIG[env] ?? { color: 'red', label: 'UNKNOWN ENV' };
  return (
    <Badge color={config.color} variant="filled" size="sm">
      {config.label}
    </Badge>
  );
}

export function Layout() {
  const [opened, { toggle }] = useDisclosure();
  const { setColorScheme } = useMantineColorScheme();
  const colorScheme = useComputedColorScheme();

  const toggleColorScheme = () => {
    setColorScheme(colorScheme === 'dark' ? 'light' : 'dark');
  };

  return (
    <AppShell
      navbar={{ width: 300, breakpoint: 'sm', collapsed: { mobile: !opened } }}
      header={{ height: 60 }}
      padding="md"
    >
      <AppShell.Header>
        <Group h="100%" px="md" gap="sm">
          <Burger opened={opened} onClick={toggle} hiddenFrom="sm" size="sm" />
          <Image
            src={colorScheme === 'dark' ? logoLight : logoDark}
            alt="TinyCongress logo"
            h={32}
            w="auto"
          />
          <Text fw={700}>TinyCongress</Text>
          <EnvironmentBadge />
          <ActionIcon
            variant="subtle"
            onClick={toggleColorScheme}
            ml="auto"
            size="lg"
            aria-label="Toggle color scheme"
          >
            {colorScheme === 'dark' ? <IconSun size={20} /> : <IconMoon size={20} />}
          </ActionIcon>
        </Group>
      </AppShell.Header>
      <AppShell.Navbar>
        <Navbar />
      </AppShell.Navbar>

      <AppShell.Main>
        <Outlet />
      </AppShell.Main>
    </AppShell>
  );
}
