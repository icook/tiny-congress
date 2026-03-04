import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Outlet,
  redirect,
  RouterProvider,
  useParams,
} from '@tanstack/react-router';
import { AuthRequiredOutlet } from './components/AuthRequiredOutlet';
import { ErrorBoundary } from './components/ErrorBoundary';
import { AboutPage } from './pages/About.page';
import { DashboardPage } from './pages/Dashboard.page';
import { HomePage } from './pages/Home.page';
import { Layout } from './pages/Layout';
import { LoginPage } from './pages/Login.page';
import { PollPage } from './pages/Poll.page';
import { RoomsPage } from './pages/Rooms.page';
import { SettingsPage } from './pages/Settings.page';
import { SignupPage } from './pages/Signup.page';
import { ThreadedConversationPage } from './pages/ThreadedConversation.page';
import { VerifyCallbackPage } from './pages/VerifyCallback.page';
import { useDevice } from './providers/DeviceProvider';

interface RouterContext {
  auth: { deviceKid: string | null };
}

const rootRoute = createRootRouteWithContext<RouterContext>()({
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

// Layout route for guest-only pages (login, signup)
// Redirects authenticated users to /settings
const guestOnlyLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: 'guest-only',
  component: Outlet,
  beforeLoad: ({ context }) => {
    if (context.auth.deviceKid) {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- TanStack Router redirect API
      throw redirect({ to: '/settings' });
    }
  },
});

const signupRoute = createRoute({
  getParentRoute: () => guestOnlyLayout,
  path: 'signup',
  component: SignupPage,
});

const loginRoute = createRoute({
  getParentRoute: () => guestOnlyLayout,
  path: 'login',
  component: LoginPage,
});

// Layout route for auth-required pages (settings, account, security)
// beforeLoad handles navigation-time guard; AuthRequiredOutlet handles reactive logout
const authRequiredLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: 'auth-required',
  component: AuthRequiredOutlet,
  beforeLoad: ({ context }) => {
    if (!context.auth.deviceKid) {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- TanStack Router redirect API
      throw redirect({ to: '/login' });
    }
  },
});

const settingsRoute = createRoute({
  getParentRoute: () => authRequiredLayout,
  path: 'settings',
  component: SettingsPage,
});

const verifyCallbackRoute = createRoute({
  getParentRoute: () => authRequiredLayout,
  path: 'verify/callback',
  component: VerifyCallbackPage,
  validateSearch: (
    search: Record<string, unknown>
  ): { verification?: string; method?: string; message?: string } => ({
    verification: typeof search.verification === 'string' ? search.verification : undefined,
    method: typeof search.method === 'string' ? search.method : undefined,
    message: typeof search.message === 'string' ? search.message : undefined,
  }),
});

const roomsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'rooms',
  component: RoomsPage,
});

const pollRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'rooms/$roomId/polls/$pollId',
  component: PollPageWrapper,
});

const routeTree = rootRoute.addChildren([
  homeRoute,
  dashboardRoute,
  conversationsRoute,
  aboutRoute,
  guestOnlyLayout.addChildren([signupRoute, loginRoute]),
  authRequiredLayout.addChildren([
    settingsRoute,
    verifyCallbackRoute,
    createPlaceholderRoute(authRequiredLayout, 'account', 'Account', 'Account page content'),
    createPlaceholderRoute(authRequiredLayout, 'security', 'Security', 'Security page content'),
  ]),
  roomsRoute,
  pollRoute,
  createPlaceholderRoute(rootRoute, 'analytics', 'Analytics', 'Analytics page content'),
  createPlaceholderRoute(rootRoute, 'releases', 'Releases', 'Releases page content'),
]);

// eslint-disable-next-line @typescript-eslint/no-non-null-assertion -- context provided at render time via RouterProvider
const router = createRouter({ routeTree, context: undefined! });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

export function Router() {
  const { deviceKid, isLoading } = useDevice();

  if (isLoading) {
    return null;
  }

  return (
    <ErrorBoundary context="Router">
      <RouterProvider router={router} context={{ auth: { deviceKid } }} />
    </ErrorBoundary>
  );
}

function PollPageWrapper() {
  const { roomId, pollId } = useParams({ from: '/rooms/$roomId/polls/$pollId' });
  return <PollPage roomId={roomId} pollId={pollId} />;
}

function createPlaceholderRoute(
  parent: typeof rootRoute | typeof authRequiredLayout,
  path: string,
  title: string,
  description: string
) {
  return createRoute({
    getParentRoute: () => parent,
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
