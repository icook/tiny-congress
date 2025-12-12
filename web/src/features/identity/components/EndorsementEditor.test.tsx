/**
 * Tests for EndorsementEditor component
 */

import { fireEvent, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { render } from '../../../../test-utils';
import { createEndorsement } from '../api/client';
import { EndorsementEditor, EndorsementItem } from './EndorsementEditor';

// Mock the API client
vi.mock('../api/client', () => ({
  createEndorsement: vi.fn(),
  ApiError: class ApiError extends Error {
    status: number;
    constructor(message: string, status: number) {
      super(message);
      this.status = status;
    }
  },
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
  getDevicePrivateKey: vi.fn(() => new Uint8Array(32)),
  getDevicePublicKey: vi.fn(() => new Uint8Array(32)),
  deriveKid: vi.fn(() => 'test-kid'),
  signEnvelope: vi.fn(() => ({
    v: 1,
    payload_type: 'EndorsementCreated',
    payload: {},
    signer: { account_id: 'test-account-id', device_id: 'test-device-id', kid: 'test-kid' },
    sig: 'test-signature',
  })),
}));

describe('EndorsementEditor', () => {
  it('should render endorsement form', () => {
    render(<EndorsementEditor subjectAccountId="subject-123" subjectUsername="targetuser" />);

    // Title and button both say "Create Endorsement", so use getByRole for heading
    expect(screen.getByRole('heading', { name: 'Create Endorsement' })).toBeInTheDocument();
    expect(screen.getByText('Endorsing: targetuser')).toBeInTheDocument();
    expect(screen.getByText('Topic')).toBeInTheDocument();
  });

  it('should display magnitude and confidence sliders', () => {
    render(<EndorsementEditor subjectAccountId="subject-123" />);

    expect(screen.getByText(/Magnitude:/)).toBeInTheDocument();
    expect(screen.getByText(/Confidence:/)).toBeInTheDocument();
  });

  it('should show weighted contribution preview', () => {
    render(<EndorsementEditor subjectAccountId="subject-123" />);

    expect(screen.getByText('Weighted Contribution Preview')).toBeInTheDocument();
  });

  it('should require topic selection before submit', async () => {
    render(<EndorsementEditor subjectAccountId="subject-123" />);

    const submitButton = screen.getByRole('button', { name: /Create Endorsement/i });
    expect(submitButton).toBeDisabled();
  });

  it('should call createEndorsement on submit with topic selected', async () => {
    vi.mocked(createEndorsement).mockResolvedValue({ endorsement_id: 'new-endorsement-123' });
    const onSuccess = vi.fn();

    render(<EndorsementEditor subjectAccountId="subject-123" onSuccess={onSuccess} />);

    // Select a topic using the combobox
    const topicInput = screen.getByPlaceholderText('Select endorsement topic');
    fireEvent.click(topicInput);

    // Wait for dropdown to appear
    await waitFor(() => {
      expect(screen.getByText('Trustworthy')).toBeInTheDocument();
    });

    // Click the option
    const trustworthyOption = screen.getByText('Trustworthy');
    fireEvent.click(trustworthyOption);

    // Submit
    const submitButton = screen.getByRole('button', { name: /Create Endorsement/i });
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(createEndorsement).toHaveBeenCalled();
    });
  });

  it('should show success message after creation', async () => {
    vi.mocked(createEndorsement).mockResolvedValue({ endorsement_id: 'new-endorsement-123' });

    render(<EndorsementEditor subjectAccountId="subject-123" />);

    // Select topic
    const topicInput = screen.getByPlaceholderText('Select endorsement topic');
    fireEvent.click(topicInput);

    // Wait for dropdown to appear
    await waitFor(() => {
      expect(screen.getByText('Trustworthy')).toBeInTheDocument();
    });

    // Click the option
    const trustworthyOption = screen.getByText('Trustworthy');
    fireEvent.click(trustworthyOption);

    // Submit
    const submitButton = screen.getByRole('button', { name: /Create Endorsement/i });
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(screen.getByText('Endorsement Created')).toBeInTheDocument();
    });
  });
});

describe('EndorsementItem', () => {
  const mockEndorsement = {
    id: 'end-1',
    topic: 'trustworthy',
    magnitude: 0.8,
    confidence: 0.9,
    context: 'Great person to work with',
    created_at: '2024-01-01T00:00:00Z',
  };

  it('should render endorsement details', () => {
    render(<EndorsementItem endorsement={mockEndorsement} />);

    expect(screen.getByText('trustworthy')).toBeInTheDocument();
    expect(screen.getByText('+0.72')).toBeInTheDocument(); // 0.8 * 0.9
    expect(screen.getByText('Great person to work with')).toBeInTheDocument();
  });

  it('should show revoke button when canRevoke is true', () => {
    render(<EndorsementItem endorsement={mockEndorsement} canRevoke />);

    expect(screen.getByRole('button', { name: /Revoke/i })).toBeInTheDocument();
  });

  it('should not show revoke button when canRevoke is false', () => {
    render(<EndorsementItem endorsement={mockEndorsement} canRevoke={false} />);

    expect(screen.queryByRole('button', { name: /Revoke/i })).not.toBeInTheDocument();
  });

  it('should call onRevoke when revoke button is clicked', async () => {
    const onRevoke = vi.fn().mockResolvedValue(undefined);
    render(<EndorsementItem endorsement={mockEndorsement} canRevoke onRevoke={onRevoke} />);

    const revokeButton = screen.getByRole('button', { name: /Revoke/i });
    fireEvent.click(revokeButton);

    await waitFor(() => {
      expect(onRevoke).toHaveBeenCalledWith('end-1');
    });
  });
});
