/**
 * Tests for Signup component
 */

import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { render } from '../../../../test-utils';
import { signup, type SignupResponse } from '../api/client';
import { Signup } from './Signup';

// Mock the API client
vi.mock('../api/client', () => ({
  signup: vi.fn(),
  ApiError: class ApiError extends Error {
    constructor(
      message: string,
      public status: number,
      public body?: unknown
    ) {
      super(message);
      this.name = 'ApiError';
    }
  },
}));

// Mock the keys module
vi.mock('../keys', () => ({
  generateRootKey: vi.fn(() => ({
    privateKey: new Uint8Array(32).fill(1),
    publicKey: new Uint8Array(32).fill(2),
  })),
  generateDeviceKey: vi.fn((label?: string) => ({
    kid: 'test-device-kid',
    publicKey: 'test-device-pubkey',
    privateKey: 'test-device-privkey',
    createdAt: new Date().toISOString(),
    label,
  })),
  deriveKid: vi.fn(() => 'test-root-kid'),
  signEnvelope: vi.fn(() => ({
    v: 1,
    payload_type: 'DeviceDelegation',
    payload: {},
    signer: { kid: 'test-root-kid' },
    sig: 'test-signature',
  })),
  storeDeviceKey: vi.fn(),
  storeRootKeyTemporary: vi.fn(),
}));

// Mock session state
vi.mock('../state/session', () => ({
  saveSession: vi.fn(),
}));

// Mock router
const mockNavigate = vi.fn();
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
}));

describe('Signup', () => {
  it('should render signup form', () => {
    render(<Signup />);

    expect(screen.getByText('Create Account')).toBeInTheDocument();
    expect(screen.getByLabelText(/Username/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Device Name/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Sign Up/i })).toBeInTheDocument();
  });

  it('should not submit with empty fields', async () => {
    const user = userEvent.setup();
    render(<Signup />);

    const submitButton = screen.getByRole('button', { name: /Sign Up/i });
    await user.click(submitButton);

    // Signup should not be called when form validation fails
    expect(signup).not.toHaveBeenCalled();
  });

  it('should submit signup successfully', async () => {
    const user = userEvent.setup();
    const mockResponse: SignupResponse = {
      account_id: 'test-account-id',
      device_id: 'test-device-id',
      username: 'testuser',
    };

    vi.mocked(signup).mockResolvedValue(mockResponse);

    render(<Signup />);

    await user.type(screen.getByLabelText(/Username/i), 'testuser');
    await user.type(screen.getByLabelText(/Device Name/i), 'Test Device');
    await user.click(screen.getByRole('button', { name: /Sign Up/i }));

    await waitFor(() => {
      expect(signup).toHaveBeenCalled();
    });

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith({ to: '/dashboard' });
    });
  });

  it('should handle signup error', async () => {
    const user = userEvent.setup();
    vi.mocked(signup).mockRejectedValue(new Error('Username already exists'));

    render(<Signup />);

    await user.type(screen.getByLabelText(/Username/i), 'testuser');
    await user.type(screen.getByLabelText(/Device Name/i), 'Test Device');
    await user.click(screen.getByRole('button', { name: /Sign Up/i }));

    await waitFor(() => {
      expect(screen.getByText('Username already exists')).toBeInTheDocument();
    });
  });

  it('should disable form while submitting', async () => {
    const user = userEvent.setup();
    vi.mocked(signup).mockImplementation(() => new Promise((resolve) => setTimeout(resolve, 1000)));

    render(<Signup />);

    await user.type(screen.getByLabelText(/Username/i), 'testuser');
    await user.type(screen.getByLabelText(/Device Name/i), 'Test Device');

    const submitButton = screen.getByRole('button', { name: /Sign Up/i });
    await user.click(submitButton);

    // Form fields should be disabled
    expect(screen.getByLabelText(/Username/i)).toBeDisabled();
    expect(screen.getByLabelText(/Device Name/i)).toBeDisabled();
  });
});
