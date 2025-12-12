/**
 * Tests for Login component
 */

import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { render } from '../../../../test-utils';
import { issueChallenge, verifyChallenge } from '../api/client';
import { Login } from './Login';

// Mock the API client
vi.mock('../api/client', () => ({
  issueChallenge: vi.fn(),
  verifyChallenge: vi.fn(),
}));

// Mock the keys module
vi.mock('../keys', () => ({
  hasDeviceKey: vi.fn(() => Promise.resolve(true)),
  getDevicePrivateKey: vi.fn(() => Promise.resolve(new Uint8Array(32).fill(1))),
  signChallenge: vi.fn(() => 'mock-signature'),
}));

// Mock session state
vi.mock('../state/session', () => ({
  getSession: vi.fn(() => null),
  saveSession: vi.fn(),
}));

// Mock router
const mockNavigate = vi.fn();
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
}));

describe('Login', () => {
  it('should render login form', () => {
    render(<Login />);

    expect(screen.getByText('Login')).toBeInTheDocument();
    expect(screen.getByLabelText(/Account ID/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Device ID/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Request Challenge/i })).toBeInTheDocument();
  });

  it('should request challenge on button click', async () => {
    const user = userEvent.setup();

    vi.mocked(issueChallenge).mockResolvedValue({
      challenge_id: 'test-challenge-id',
      nonce: 'test-nonce',
      expires_at: new Date(Date.now() + 300000).toISOString(), // 5 minutes from now
    });

    render(<Login />);

    await user.type(screen.getByLabelText(/Account ID/i), 'test-account-id');
    await user.type(screen.getByLabelText(/Device ID/i), 'test-device-id');
    await user.click(screen.getByRole('button', { name: /Request Challenge/i }));

    await waitFor(() => {
      expect(issueChallenge).toHaveBeenCalledWith({
        account_id: 'test-account-id',
        device_id: 'test-device-id',
      });
    });

    // Should show verify button after challenge is received
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Verify & Login/i })).toBeInTheDocument();
    });
  });

  it('should verify and login successfully', async () => {
    const user = userEvent.setup();

    vi.mocked(issueChallenge).mockResolvedValue({
      challenge_id: 'test-challenge-id',
      nonce: 'test-nonce',
      expires_at: new Date(Date.now() + 300000).toISOString(),
    });

    vi.mocked(verifyChallenge).mockResolvedValue({
      session_id: 'test-session-id',
      token: 'test-token',
      expires_at: new Date(Date.now() + 86400000).toISOString(),
    });

    render(<Login />);

    // Fill in form and request challenge
    await user.type(screen.getByLabelText(/Account ID/i), 'test-account-id');
    await user.type(screen.getByLabelText(/Device ID/i), 'test-device-id');
    await user.click(screen.getByRole('button', { name: /Request Challenge/i }));

    // Wait for challenge to be received
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Verify & Login/i })).toBeInTheDocument();
    });

    // Click verify
    await user.click(screen.getByRole('button', { name: /Verify & Login/i }));

    await waitFor(() => {
      expect(verifyChallenge).toHaveBeenCalled();
    });

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith({ to: '/dashboard' });
    });
  });

  it('should handle challenge request error', async () => {
    const user = userEvent.setup();

    vi.mocked(issueChallenge).mockRejectedValue(new Error('Account not found'));

    render(<Login />);

    await user.type(screen.getByLabelText(/Account ID/i), 'bad-account');
    await user.type(screen.getByLabelText(/Device ID/i), 'test-device-id');
    await user.click(screen.getByRole('button', { name: /Request Challenge/i }));

    await waitFor(() => {
      expect(screen.getByText('Account not found')).toBeInTheDocument();
    });
  });

  it('should handle verify error', async () => {
    const user = userEvent.setup();

    vi.mocked(issueChallenge).mockResolvedValue({
      challenge_id: 'test-challenge-id',
      nonce: 'test-nonce',
      expires_at: new Date(Date.now() + 300000).toISOString(),
    });

    vi.mocked(verifyChallenge).mockRejectedValue(new Error('Invalid signature'));

    render(<Login />);

    await user.type(screen.getByLabelText(/Account ID/i), 'test-account-id');
    await user.type(screen.getByLabelText(/Device ID/i), 'test-device-id');
    await user.click(screen.getByRole('button', { name: /Request Challenge/i }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Verify & Login/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Verify & Login/i }));

    await waitFor(() => {
      expect(screen.getByText('Invalid signature')).toBeInTheDocument();
    });
  });
});
