/**
 * Tests for Profile component
 */

import { screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { render } from '../../../../test-utils';
import {
  getEndorsements,
  getReputationScore,
  getSecurityPosture,
  listDevices,
  type Endorsement,
} from '../api/client';
import { Profile } from './Profile';

// Mock the API client
vi.mock('../api/client', () => ({
  getEndorsements: vi.fn(),
  getReputationScore: vi.fn(),
  getSecurityPosture: vi.fn(),
  listDevices: vi.fn(),
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

describe('Profile', () => {
  const mockEndorsements: Endorsement[] = [
    {
      id: 'end-1',
      author_account_id: 'author-1',
      author_device_id: 'device-1',
      subject_type: 'account',
      subject_id: 'test-account-id',
      topic: 'trustworthy',
      magnitude: 0.8,
      confidence: 0.9,
      created_at: '2024-01-01T00:00:00Z',
    },
    {
      id: 'end-2',
      author_account_id: 'author-2',
      author_device_id: 'device-2',
      subject_type: 'account',
      subject_id: 'test-account-id',
      topic: 'is_real_person',
      magnitude: 1.0,
      confidence: 0.95,
      created_at: '2024-01-02T00:00:00Z',
    },
  ];

  it('should render profile page with username', async () => {
    vi.mocked(listDevices).mockResolvedValue([]);
    vi.mocked(getSecurityPosture).mockRejectedValue(new Error('Not found'));
    vi.mocked(getEndorsements).mockResolvedValue([[], null]);
    vi.mocked(getReputationScore).mockRejectedValue(new Error('Not found'));

    render(<Profile />);

    await waitFor(() => {
      expect(screen.getByText('testuser')).toBeInTheDocument();
    });
  });

  it('should display security posture summary', async () => {
    vi.mocked(listDevices).mockResolvedValue([
      {
        device_id: 'd1',
        device_kid: 'kid1',
        device_metadata: { name: 'Device 1', type: 'browser' },
        created_at: '2024-01-01T00:00:00Z',
      },
      {
        device_id: 'd2',
        device_kid: 'kid2',
        device_metadata: { name: 'Device 2', type: 'mobile' },
        created_at: '2024-01-01T00:00:00Z',
      },
    ]);
    vi.mocked(getSecurityPosture).mockRejectedValue(new Error('Not found'));
    vi.mocked(getEndorsements).mockResolvedValue([[], null]);
    vi.mocked(getReputationScore).mockRejectedValue(new Error('Not found'));

    render(<Profile />);

    await waitFor(() => {
      expect(screen.getByText('Security Posture')).toBeInTheDocument();
    });

    expect(screen.getByText('2 active / 2 total')).toBeInTheDocument();
  });

  it('should display endorsements by topic', async () => {
    vi.mocked(listDevices).mockResolvedValue([]);
    vi.mocked(getSecurityPosture).mockRejectedValue(new Error('Not found'));
    vi.mocked(getEndorsements).mockResolvedValue([mockEndorsements, null]);
    vi.mocked(getReputationScore).mockRejectedValue(new Error('Not found'));

    render(<Profile />);

    await waitFor(() => {
      expect(screen.getByText('trustworthy')).toBeInTheDocument();
    });

    expect(screen.getByText('is real person')).toBeInTheDocument();
  });

  it('should display reputation score', async () => {
    vi.mocked(listDevices).mockResolvedValue([]);
    vi.mocked(getSecurityPosture).mockRejectedValue(new Error('Not found'));
    vi.mocked(getEndorsements).mockResolvedValue([[], null]);
    vi.mocked(getReputationScore).mockResolvedValue({
      account_id: 'test-account-id',
      score: 0.75,
      updated_at: '2024-01-01T00:00:00Z',
    });

    render(<Profile />);

    await waitFor(() => {
      // Check for the Reputation card title
      expect(screen.getByText('Reputation')).toBeInTheDocument();
    });

    // There should be at least two 75% elements (pill and card)
    const percentageElements = screen.getAllByText('75%');
    expect(percentageElements.length).toBeGreaterThanOrEqual(2);
  });

  it('should show tier badge', async () => {
    vi.mocked(listDevices).mockResolvedValue([]);
    vi.mocked(getSecurityPosture).mockRejectedValue(new Error('Not found'));
    vi.mocked(getEndorsements).mockResolvedValue([[], null]);
    vi.mocked(getReputationScore).mockRejectedValue(new Error('Not found'));

    render(<Profile />);

    await waitFor(() => {
      expect(screen.getByText('Anonymous')).toBeInTheDocument();
    });
  });
});
