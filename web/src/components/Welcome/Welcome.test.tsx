import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from '@tanstack/react-router';
import { render, screen } from '@test-utils';
import { Welcome } from './Welcome';

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const welcomeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: Welcome,
});

const routeTree = rootRoute.addChildren([welcomeRoute]);

const router = createRouter({
  routeTree,
  history: createMemoryHistory({ initialEntries: ['/'] }),
});

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

describe('Welcome component', () => {
  it('renders OAuth call-to-action', async () => {
    render(<RouterProvider router={router} />);

    expect(await screen.findByRole('link', { name: /sign in with oauth/i })).toBeInTheDocument();
    expect(await screen.findByText(/trusted provider/i, { exact: false })).toBeInTheDocument();
  });
});
