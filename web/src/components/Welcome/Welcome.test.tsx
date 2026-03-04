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

describe('Welcome component', () => {
  it('renders TinyCongress heading', () => {
    mockUseDevice.mockReturnValue({ deviceKid: null, isLoading: false });
    render(<Welcome />);
    expect(screen.getByText('TinyCongress')).toBeInTheDocument();
  });

  it('shows Sign Up and Browse Rooms for logged-out users', () => {
    mockUseDevice.mockReturnValue({ deviceKid: null, isLoading: false });
    render(<Welcome />);
    expect(screen.getByRole('link', { name: 'Sign Up' })).toHaveAttribute('href', '/signup');
    expect(screen.getByRole('link', { name: 'Browse Rooms' })).toHaveAttribute('href', '/rooms');
  });

  it('shows Browse Rooms and Go to Settings for logged-in users', () => {
    mockUseDevice.mockReturnValue({ deviceKid: 'test-kid-123', isLoading: false });
    render(<Welcome />);
    expect(screen.getByRole('link', { name: 'Browse Rooms' })).toHaveAttribute('href', '/rooms');
    expect(screen.getByRole('link', { name: 'Go to Settings' })).toHaveAttribute(
      'href',
      '/settings'
    );
    expect(screen.queryByRole('link', { name: 'Sign Up' })).not.toBeInTheDocument();
  });
});
