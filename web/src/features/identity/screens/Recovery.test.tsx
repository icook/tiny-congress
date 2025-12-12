/**
 * Tests for Recovery component
 */

import { screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { render } from '../../../../test-utils';
import { getRecoveryPolicy, type RecoveryPolicy } from '../api/client';
import { Recovery } from './Recovery';

// Mock the API client
vi.mock('../api/client', () => ({
  getRecoveryPolicy: vi.fn(),
  setRecoveryPolicy: vi.fn(),
  approveRecovery: vi.fn(),
  rotateRoot: vi.fn(),
}));

// Mock session state
vi.mock('../state/session', () => ({
  getSession: vi.fn(() => ({
    accountId: 'test-account-id',
    deviceId: 'test-device-id',
    sessionToken: 'test-token',
    expiresAt: new Date(Date.now() + 86400000).toISOString(),
    username: 'testuser',
  })),
}));

// Mock keys module
vi.mock('../keys', () => ({
  deriveKid: vi.fn(() => 'test-kid'),
  encodeBase64Url: vi.fn(() => 'base64-encoded'),
  generateRootKey: vi.fn(() => ({
    publicKey: new Uint8Array(32),
    privateKey: new Uint8Array(32),
  })),
  getRootKeyTemporary: vi.fn(() => ({
    publicKey: new Uint8Array(32),
    privateKey: new Uint8Array(32),
  })),
  signEnvelope: vi.fn(() => ({
    v: 1,
    payload_type: 'RecoveryPolicySet',
    payload: {},
    signer: { account_id: 'test-account-id', kid: 'test-kid' },
    sig: 'test-signature',
  })),
}));

describe('Recovery', () => {
  const mockPolicy: RecoveryPolicy = {
    policy_id: 'policy-123',
    threshold: 2,
    helpers: [
      { helper_account_id: 'helper-1', helper_root_kid: 'kid-1' },
      { helper_account_id: 'helper-2', helper_root_kid: null },
      { helper_account_id: 'helper-3', helper_root_kid: 'kid-3' },
    ],
    created_at: '2024-01-01T00:00:00Z',
    revoked_at: null,
  };

  it('should render recovery page title', async () => {
    vi.mocked(getRecoveryPolicy).mockRejectedValue(new Error('404 Not Found'));

    render(<Recovery />);

    await waitFor(() => {
      expect(screen.getByText('Recovery Setup')).toBeInTheDocument();
    });
  });

  it('should show no policy message when none exists', async () => {
    vi.mocked(getRecoveryPolicy).mockRejectedValue(new Error('404 Not Found'));

    render(<Recovery />);

    await waitFor(() => {
      expect(screen.getByText('No Recovery Policy Configured')).toBeInTheDocument();
    });
  });

  it('should show configure button when no policy exists', async () => {
    vi.mocked(getRecoveryPolicy).mockRejectedValue(new Error('404 Not Found'));

    render(<Recovery />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Configure Recovery/i })).toBeInTheDocument();
    });
  });

  it('should display active policy when one exists', async () => {
    vi.mocked(getRecoveryPolicy).mockResolvedValue(mockPolicy);

    render(<Recovery />);

    await waitFor(() => {
      expect(screen.getByText('Active Recovery Policy')).toBeInTheDocument();
    });

    expect(screen.getByText('2 / 3')).toBeInTheDocument(); // threshold / helpers
  });

  it('should display helpers in the policy', async () => {
    vi.mocked(getRecoveryPolicy).mockResolvedValue(mockPolicy);

    render(<Recovery />);

    await waitFor(() => {
      expect(screen.getByText('Recovery Helpers')).toBeInTheDocument();
    });

    expect(screen.getByText('Helper 1')).toBeInTheDocument();
    expect(screen.getByText('Helper 2')).toBeInTheDocument();
    expect(screen.getByText('Helper 3')).toBeInTheDocument();
  });

  it('should show root rotation section when policy exists', async () => {
    vi.mocked(getRecoveryPolicy).mockResolvedValue(mockPolicy);

    render(<Recovery />);

    await waitFor(() => {
      expect(screen.getByText('Root Key Rotation')).toBeInTheDocument();
    });
  });

  it('should disable rotation button when insufficient approvals', async () => {
    vi.mocked(getRecoveryPolicy).mockResolvedValue(mockPolicy);

    render(<Recovery />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Waiting for Approvals/i })).toBeDisabled();
    });
  });
});
