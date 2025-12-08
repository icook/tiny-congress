import { createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router';
import { AuthGate } from './auth/AuthGate';
import { DashboardPage } from './pages/Dashboard.page';
import { HomePage } from './pages/Home.page';
import { Layout } from './pages/Layout';
import { LoginPage } from './pages/Login.page';
import { OAuthCallbackPage } from './pages/OAuthCallback.page';
import { ThreadedConversationPage } from './pages/ThreadedConversation.page';

const rootRoute = createRootRoute({
  component: Layout,
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: HomePage,
});

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'dashboard',
  component: () => (
    <AuthGate>
      <DashboardPage />
    </AuthGate>
  ),
});

const conversationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'conversations',
  component: () => (
    <AuthGate>
      <ThreadedConversationPage />
    </AuthGate>
  ),
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'login',
  component: LoginPage,
});

const loginCallbackRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'login/callback',
  component: OAuthCallbackPage,
});

export const routeTree = rootRoute.addChildren([
  homeRoute,
  dashboardRoute,
  conversationsRoute,
  loginRoute,
  loginCallbackRoute,
  createPlaceholderRoute('analytics', 'Analytics', 'Analytics page content'),
  createPlaceholderRoute('releases', 'Releases', 'Releases page content'),
  createPlaceholderRoute('account', 'Account', 'Account page content'),
  createPlaceholderRoute('security', 'Security', 'Security page content'),
  createPlaceholderRoute('settings', 'Settings', 'Settings page content'),
]);

export const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

export function Router() {
  return <RouterProvider router={router} />;
}

function createPlaceholderRoute(path: string, title: string, description: string) {
  return createRoute({
    getParentRoute: () => rootRoute,
    path,
    component: () => (
      <AuthGate>
        <PlaceholderPage title={title} description={description} />
      </AuthGate>
    ),
  });
}

function PlaceholderPage({ title, description }: { title: string; description: string }) {
  return (
    <div>
      <h1>{title}</h1>
      <p>{description}</p>
    </div>
  );
}
