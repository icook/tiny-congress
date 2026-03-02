import { createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router';
import { ErrorBoundary } from './components/ErrorBoundary';
import { AboutPage } from './pages/About.page';
import { DashboardPage } from './pages/Dashboard.page';
import { HomePage } from './pages/Home.page';
import { Layout } from './pages/Layout';
import { LoginPage } from './pages/Login.page';
import { SettingsPage } from './pages/Settings.page';
import { SignupPage } from './pages/Signup.page';
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
  component: DashboardPage,
});

const conversationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'conversations',
  component: ThreadedConversationPage,
});

const aboutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'about',
  component: AboutPage,
});

const signupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'signup',
  component: SignupPage,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'login',
  component: LoginPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'settings',
  component: SettingsPage,
});

const routeTree = rootRoute.addChildren([
  homeRoute,
  dashboardRoute,
  conversationsRoute,
  aboutRoute,
  signupRoute,
  loginRoute,
  settingsRoute,
  createPlaceholderRoute('analytics', 'Analytics', 'Analytics page content'),
  createPlaceholderRoute('releases', 'Releases', 'Releases page content'),
  createPlaceholderRoute('account', 'Account', 'Account page content'),
  createPlaceholderRoute('security', 'Security', 'Security page content'),
]);

const router = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

export function Router() {
  return (
    <ErrorBoundary context="Router">
      <RouterProvider router={router} />
    </ErrorBoundary>
  );
}

function createPlaceholderRoute(path: string, title: string, description: string) {
  return createRoute({
    getParentRoute: () => rootRoute,
    path,
    component: () => <PlaceholderPage title={title} description={description} />,
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
