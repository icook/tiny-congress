import { render, screen } from '@test-utils';
import { describe, expect, test, vi } from 'vitest';

const mockNavigate = vi.fn();

// Mock the full @tanstack/react-router API surface that Router.tsx uses at module scope
vi.mock('@tanstack/react-router', () => {
  const mockRoute = { addChildren: vi.fn().mockReturnThis() };
  const createRoute = vi.fn(() => mockRoute);
  return {
    createRootRouteWithContext: vi.fn(() => vi.fn(() => mockRoute)),
    createRoute,
    createRouter: vi.fn(() => ({})),
    Outlet: () => <div data-testid="outlet">outlet content</div>,
    redirect: vi.fn(),
    RouterProvider: vi.fn(() => null),
    useNavigate: vi.fn(() => mockNavigate),
    useParams: vi.fn(() => ({})),
  };
});

const mockUseDevice = vi.fn();
vi.mock('./providers/DeviceProvider', () => ({
  useDevice: (...args: unknown[]) => mockUseDevice(...args),
}));

// Stub other imports Router.tsx pulls in at module scope
vi.mock('./components/ErrorBoundary', () => ({
  ErrorBoundary: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));
vi.mock('./pages/About.page', () => ({ AboutPage: () => null }));
vi.mock('./pages/Dashboard.page', () => ({ DashboardPage: () => null }));
vi.mock('./pages/Home.page', () => ({ HomePage: () => null }));
vi.mock('./pages/Layout', () => ({ Layout: () => null }));
vi.mock('./pages/Login.page', () => ({ LoginPage: () => null }));
vi.mock('./pages/Poll.page', () => ({ PollPage: () => null }));
vi.mock('./pages/Rooms.page', () => ({ RoomsPage: () => null }));
vi.mock('./pages/Settings.page', () => ({ SettingsPage: () => null }));
vi.mock('./pages/Signup.page', () => ({ SignupPage: () => null }));
vi.mock('./pages/ThreadedConversation.page', () => ({ ThreadedConversationPage: () => null }));

// Import after mocks are set up
const { AuthRequiredOutlet } = await import('./Router');

describe('AuthRequiredOutlet', () => {
  test('renders Outlet when deviceKid is set', () => {
    mockUseDevice.mockReturnValue({ deviceKid: 'kid-123' });

    render(<AuthRequiredOutlet />);

    expect(screen.getByTestId('outlet')).toBeInTheDocument();
    expect(mockNavigate).not.toHaveBeenCalled();
  });

  test('navigates to /login when deviceKid is null', () => {
    mockUseDevice.mockReturnValue({ deviceKid: null });

    render(<AuthRequiredOutlet />);

    expect(screen.queryByTestId('outlet')).not.toBeInTheDocument();
    expect(mockNavigate).toHaveBeenCalledWith({ to: '/login' });
  });
});
