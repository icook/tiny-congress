import { createBrowserRouter, RouterProvider } from 'react-router-dom';
import { DashboardPage } from './pages/Dashboard.page';
import { HomePage } from './pages/Home.page';
import { Layout } from './pages/Layout';
import { ThreadedConversationPage } from './pages/ThreadedConversation.page';

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
      {
        path: 'conversations',
        element: <ThreadedConversationPage />,
      },
      // Placeholder routes for the other nav items
      {
        path: 'analytics',
        element: (
          <div>
            <h1>Analytics</h1>
            <p>Analytics page content</p>
          </div>
        ),
      },
      {
        path: 'releases',
        element: (
          <div>
            <h1>Releases</h1>
            <p>Releases page content</p>
          </div>
        ),
      },
      {
        path: 'account',
        element: (
          <div>
            <h1>Account</h1>
            <p>Account page content</p>
          </div>
        ),
      },
      {
        path: 'security',
        element: (
          <div>
            <h1>Security</h1>
            <p>Security page content</p>
          </div>
        ),
      },
      {
        path: 'settings',
        element: (
          <div>
            <h1>Settings</h1>
            <p>Settings page content</p>
          </div>
        ),
      },
    ],
  },
]);

export function Router() {
  return <RouterProvider router={router} />;
}
