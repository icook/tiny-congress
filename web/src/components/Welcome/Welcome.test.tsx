import { createMemoryHistory, createRouter, RouterProvider } from '@tanstack/react-router';
import { render, screen } from '@test-utils';
import { routeTree } from '../../Router';
import { Welcome } from './Welcome';

const router = createRouter({
  routeTree,
  history: createMemoryHistory({ initialEntries: ['/'] }),
});

describe('Welcome component', () => {
  it('renders OAuth call-to-action', async () => {
    render(<RouterProvider router={router} />);

    expect(await screen.findByRole('link', { name: /sign in with oauth/i })).toBeInTheDocument();
    expect(await screen.findByText(/trusted provider/i, { exact: false })).toBeInTheDocument();
  });
});
