/**
 * Tests for Devices component
 */

import { screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { render } from '../../../../test-utils';
import { listDevices, type Device } from '../api/client';
import { Devices } from './Devices';

// Mock the API client
vi.mock('../api/client', () => ({
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

describe('Devices', () => {
  const mockDevices: Device[] = [
    {
      device_id: 'device-1',
      device_kid: 'kid-1',
      device_metadata: {
        name: 'Test Device 1',
        type: 'browser',
        os: 'macOS',
      },
      created_at: '2024-01-01T00:00:00Z',
      last_seen: '2024-01-02T00:00:00Z',
    },
    {
      device_id: 'device-2',
      device_kid: 'kid-2',
      device_metadata: {
        name: 'Test Device 2',
        type: 'mobile',
        os: 'iOS',
      },
      created_at: '2024-01-01T00:00:00Z',
      revoked_at: '2024-01-03T00:00:00Z',
    },
  ];

  it('should render devices page', async () => {
    vi.mocked(listDevices).mockResolvedValue(mockDevices);

    render(<Devices />);

    expect(screen.getByText('Devices')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Add Device/i })).toBeInTheDocument();
  });

  it('should display active and revoked devices', async () => {
    vi.mocked(listDevices).mockResolvedValue(mockDevices);

    render(<Devices />);

    await waitFor(() => {
      expect(screen.getByText('Test Device 1')).toBeInTheDocument();
    });

    expect(screen.getByText('Test Device 2')).toBeInTheDocument();
    expect(screen.getByText('Revoked')).toBeInTheDocument();
  });

  it('should show loading state', () => {
    vi.mocked(listDevices).mockImplementation(
      () => new Promise(() => {}) // Never resolves
    );

    render(<Devices />);

    expect(screen.getByText('Loading devices...')).toBeInTheDocument();
  });
});
