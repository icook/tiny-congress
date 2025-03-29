import { Outlet } from 'react-router-dom';
import { AppShell } from '@mantine/core';
import { DoubleNavbar } from '../components/DoubleNavbar/DoubleNavbar';

export function Layout() {
  return (
    <AppShell
      navbar={{ width: 300, breakpoint: 'sm' }}
      padding="md"
    >
      <AppShell.Navbar>
        <DoubleNavbar />
      </AppShell.Navbar>

      <AppShell.Main>
        <Outlet />
      </AppShell.Main>
    </AppShell>
  );
}