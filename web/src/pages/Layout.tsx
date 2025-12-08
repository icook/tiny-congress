import { Outlet, useRouterState } from '@tanstack/react-router';
import { AppShell, Burger, Group, Image, Text } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { UserMenu } from '../components/Auth/UserMenu';
import { Navbar } from '../components/Navbar/Navbar';

export function Layout() {
  const [opened, { toggle }] = useDisclosure();
  const currentPath = useRouterState({
    select: (state) => state.location.pathname,
  });

  const isAuthPage = currentPath.startsWith('/login');

  return (
    <AppShell
      navbar={
        isAuthPage ? undefined : { width: 300, breakpoint: 'sm', collapsed: { mobile: !opened } }
      }
      header={{ height: 60 }}
      padding="md"
    >
      <AppShell.Header>
        <Group h="100%" px="md" gap="sm" justify="space-between">
          <Group gap="sm">
            {!isAuthPage && (
              <Burger opened={opened} onClick={toggle} hiddenFrom="sm" size="sm" />
            )}
            <Image src="/src/logo.png" alt="TinyCongress logo" h={32} w="auto" />
            <Text fw={700}>TinyCongress</Text>
          </Group>

          {!isAuthPage && <UserMenu />}
        </Group>
      </AppShell.Header>
      {!isAuthPage && (
        <AppShell.Navbar>
          <Navbar />
        </AppShell.Navbar>
      )}

      <AppShell.Main>
        <Outlet />
      </AppShell.Main>
    </AppShell>
  );
}
