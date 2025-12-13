import { createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router';
import { ErrorBoundary } from './components/ErrorBoundary';
import { AboutPage } from './pages/About.page';
import { DashboardPage } from './pages/Dashboard.page';
import { HomePage } from './pages/Home.page';
import { Layout } from './pages/Layout';
import { ThreadedConversationPage } from './pages/ThreadedConversation.page';
import { Devices } from './features/identity/screens/Devices';
import { Endorsements } from './features/identity/screens/Endorsements';
import { Login } from './features/identity/screens/Login';
import { Profile } from './features/identity/screens/Profile';
import { Recovery } from './features/identity/screens/Recovery';
import { Signup } from './features/identity/screens/Signup';

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
  component: Signup,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'login',
  component: Login,
});

const accountRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'account',
  component: Profile,
});

const devicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'devices',
  component: Devices,
});

const endorsementsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'endorsements',
  component: Endorsements,
});

const recoveryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'recovery',
  component: Recovery,
});

const routeTree = rootRoute.addChildren([
  homeRoute,
  dashboardRoute,
  conversationsRoute,
  aboutRoute,
  signupRoute,
  loginRoute,
  accountRoute,
  devicesRoute,
  endorsementsRoute,
  recoveryRoute,
  createPlaceholderRoute('analytics', 'Analytics', 'Analytics page content'),
  createPlaceholderRoute('releases', 'Releases', 'Releases page content'),
  createPlaceholderRoute('security', 'Security', 'Security page content'),
  createPlaceholderRoute('settings', 'Settings', 'Settings page content'),
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
