import { render, screen } from '@test-utils';
import { describe, expect, it, vi } from 'vitest';
import { Welcome } from './Welcome';

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...props }: { children: React.ReactNode; to: string }) => (
    <a href={to} {...props}>
      {children}
    </a>
  ),
}));

const mockUseDevice = vi.fn();

vi.mock('../../providers/DeviceProvider', () => ({
  useDevice: (...args: unknown[]) => mockUseDevice(...args),
}));

vi.mock('../../features/verification', () => ({
  useVerificationStatus: vi.fn(() => ({ data: undefined, isLoading: false })),
  buildVerifierUrl: vi.fn(() => null),
}));

describe('Welcome component', () => {
  it('renders TinyCongress heading', () => {
    mockUseDevice.mockReturnValue({ deviceKid: null, username: null, isLoading: false });
    render(<Welcome />);
    expect(screen.getByText('TinyCongress')).toBeInTheDocument();
  });

  it('shows Sign Up and Browse Rooms for logged-out users', () => {
    mockUseDevice.mockReturnValue({ deviceKid: null, username: null, isLoading: false });
    render(<Welcome />);
    expect(screen.getByRole('link', { name: 'Sign Up' })).toHaveAttribute('href', '/signup');
    expect(screen.getByRole('link', { name: 'Browse Rooms' })).toHaveAttribute('href', '/rooms');
  });

  it('shows Browse Rooms and welcome message for logged-in users', () => {
    mockUseDevice.mockReturnValue({
      deviceKid: 'test-kid-123',
      username: 'alice',
      isLoading: false,
      privateKey: null,
    });
    render(<Welcome />);
    expect(screen.getByRole('link', { name: 'Browse Rooms' })).toHaveAttribute('href', '/rooms');
    expect(screen.getByText('alice')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Sign Up' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Go to Settings' })).not.toBeInTheDocument();
  });
});
