import { IconLogout, IconMoon, IconSettings, IconSun, IconUser } from '@tabler/icons-react';
import { Outlet, useNavigate } from '@tanstack/react-router';
import {
  ActionIcon,
  AppShell,
  Badge,
  Burger,
  Group,
  Image,
  Menu,
  Text,
  UnstyledButton,
  useComputedColorScheme,
  useMantineColorScheme,
} from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import logoDark from '@/logo-dark.svg';
import logoLight from '@/logo-light.svg';
import { Navbar } from '../components/Navbar/Navbar';
import { getEnvironment } from '../config';
import { useDevice } from '../providers/DeviceProvider';

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

function UserMenu() {
  const { username, clearDevice } = useDevice();
  const navigate = useNavigate();

  const handleLogout = () => {
    clearDevice();
    void navigate({ to: '/' });
  };

  if (!username) {
    return null;
  }

  return (
    <Menu shadow="md" width={180} position="bottom-end">
      <Menu.Target>
        <UnstyledButton>
          <Group gap="xs">
            <IconUser size={18} />
            <Text size="sm" fw={500}>
              {username}
            </Text>
          </Group>
        </UnstyledButton>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Item
          leftSection={<IconSettings size={16} />}
          onClick={() => void navigate({ to: '/settings' })}
        >
          Settings
        </Menu.Item>
        <Menu.Divider />
        <Menu.Item leftSection={<IconLogout size={16} />} color="red" onClick={handleLogout}>
          Logout
        </Menu.Item>
      </Menu.Dropdown>
    </Menu>
  );
}

export function Layout() {
  const [opened, { toggle, close }] = useDisclosure();
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
          <Burger
            opened={opened}
            onClick={toggle}
            hiddenFrom="sm"
            size="sm"
            aria-label="Toggle navigation"
          />
          <Image
            src={colorScheme === 'dark' ? logoLight : logoDark}
            alt="TinyCongress logo"
            h={32}
            w="auto"
          />
          <Text fw={700} visibleFrom="sm">
            TinyCongress
          </Text>
          <EnvironmentBadge />
          <Group gap="sm" ml="auto">
            <ActionIcon
              variant="subtle"
              onClick={toggleColorScheme}
              size="lg"
              aria-label="Toggle color scheme"
            >
              {colorScheme === 'dark' ? <IconSun size={20} /> : <IconMoon size={20} />}
            </ActionIcon>
            <UserMenu />
          </Group>
        </Group>
      </AppShell.Header>
      <AppShell.Navbar>
        <Navbar onNavigate={close} />
      </AppShell.Navbar>

      <AppShell.Main>
        <Outlet />
      </AppShell.Main>
    </AppShell>
  );
}
