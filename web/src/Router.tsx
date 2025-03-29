import { createBrowserRouter, RouterProvider } from 'react-router-dom';
import { HomePage } from './pages/Home.page';
import { DashboardPage } from './pages/Dashboard.page';
import { Layout } from './pages/Layout';

const router = createBrowserRouter([
  {
    path: '/',
    element: <Layout />,
    children: [
      {
        path: '',
        element: <HomePage />,
      },
      {
        path: 'dashboard',
        element: <DashboardPage />,
      },
      // Placeholder routes for the other nav items
      {
        path: 'analytics',
        element: <div><h1>Analytics</h1><p>Analytics page content</p></div>,
      },
      {
        path: 'releases',
        element: <div><h1>Releases</h1><p>Releases page content</p></div>,
      },
      {
        path: 'account',
        element: <div><h1>Account</h1><p>Account page content</p></div>,
      },
      {
        path: 'security',
        element: <div><h1>Security</h1><p>Security page content</p></div>,
      },
      {
        path: 'settings',
        element: <div><h1>Settings</h1><p>Settings page content</p></div>,
      },
      {
        path: 'orders',
        element: <div><h1>Orders</h1><p>Orders page content</p></div>,
      },
      {
        path: 'clients',
        element: <div><h1>Clients</h1><p>Clients page content</p></div>,
      },
      {
        path: 'databases',
        element: <div><h1>Databases</h1><p>Databases page content</p></div>,
      },
      {
        path: 'pull-requests',
        element: <div><h1>Pull Requests</h1><p>Pull Requests page content</p></div>,
      },
      {
        path: 'issues',
        element: <div><h1>Open Issues</h1><p>Issues page content</p></div>,
      },
      {
        path: 'wiki',
        element: <div><h1>Wiki</h1><p>Wiki pages content</p></div>,
      },
    ],
  },
]);

export function Router() {
  return <RouterProvider router={router} />;
}
