import { render, screen } from '@test-utils';
import { beforeEach, describe, expect, test, vi } from 'vitest';
import { AuthRequiredOutlet } from './AuthRequiredOutlet';

const mockNavigate = vi.fn();
vi.mock('@tanstack/react-router', () => ({
  Outlet: () => <div data-testid="outlet">outlet content</div>,
  useNavigate: vi.fn(() => mockNavigate),
}));

const mockUseDevice = vi.fn();
vi.mock('@/providers/DeviceProvider', () => ({
  useDevice: (...args: unknown[]) => mockUseDevice(...args),
}));

describe('AuthRequiredOutlet', () => {
  beforeEach(() => {
    mockNavigate.mockReset();
  });

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
