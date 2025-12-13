import { createRootRoute, createRoute, createRouter, lazyRouteComponent, RouterProvider } from '@tanstack/react-router';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Layout } from './pages/Layout';

const rootRoute = createRootRoute({
  component: Layout,
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: lazyRouteComponent(() => import('./pages/Home.page').then(m => m.HomePage)),
});

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'dashboard',
  component: lazyRouteComponent(() => import('./pages/Dashboard.page').then(m => m.DashboardPage)),
});

const conversationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'conversations',
  component: lazyRouteComponent(() => import('./pages/ThreadedConversation.page').then(m => m.ThreadedConversationPage)),
});

const aboutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'about',
  component: lazyRouteComponent(() => import('./pages/About.page').then(m => m.AboutPage)),
});

const routeTree = rootRoute.addChildren([
  homeRoute,
  dashboardRoute,
  conversationsRoute,
  aboutRoute,
  createPlaceholderRoute('analytics', 'Analytics', 'Analytics page content'),
  createPlaceholderRoute('releases', 'Releases', 'Releases page content'),
  createPlaceholderRoute('account', 'Account', 'Account page content'),
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
