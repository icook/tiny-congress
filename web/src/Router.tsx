import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  RouterProvider,
  useParams,
} from '@tanstack/react-router';
import { Button, Group, Stack, Text, Title } from '@mantine/core';
import { AuthRequiredOutlet } from './components/AuthRequiredOutlet';
import { ErrorBoundary } from './components/ErrorBoundary';
import { AboutPage } from './pages/About.page';
import { HomePage } from './pages/Home.page';
import { Layout } from './pages/Layout';
import { LoginPage } from './pages/Login.page';
import { PollPage } from './pages/Poll.page';
import { RoomsPage } from './pages/Rooms.page';
import { SettingsPage } from './pages/Settings.page';
import { SignupPage } from './pages/Signup.page';
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

const aboutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'about',
  component: AboutPage,
});

// Layout route for guest-only pages (login, signup)
// Redirects authenticated users to /rooms
const guestOnlyLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: 'guest-only',
  component: Outlet,
  beforeLoad: ({ context }) => {
    if (context.auth.deviceKid) {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- TanStack Router redirect API
      throw redirect({ to: '/rooms' });
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

// Layout route for auth-required pages
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
  aboutRoute,
  guestOnlyLayout.addChildren([signupRoute, loginRoute]),
  authRequiredLayout.addChildren([settingsRoute, verifyCallbackRoute]),
  roomsRoute,
  pollRoute,
]);

function NotFoundPage() {
  return (
    <Stack gap="md" maw={600} mx="auto" mt={100} ta="center">
      <Title order={2}>Page not found</Title>
      <Text c="dimmed">The page you're looking for doesn't exist.</Text>
      <Group justify="center">
        <Button component={Link} to="/rooms">
          Browse Rooms
        </Button>
        <Button component={Link} to="/" variant="outline">
          Go Home
        </Button>
      </Group>
    </Stack>
  );
}

const router = createRouter({
  routeTree,
  // eslint-disable-next-line @typescript-eslint/no-non-null-assertion -- context provided at render time via RouterProvider
  context: undefined!,
  defaultNotFoundComponent: NotFoundPage,
});

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
