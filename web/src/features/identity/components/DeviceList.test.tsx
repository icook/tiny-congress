import { render, screen, userEvent } from '@test-utils';
import { describe, expect, test, vi } from 'vitest';
import type { DeviceInfo } from '../api';
import { DeviceList } from './DeviceList';

function makeDevice(overrides?: Partial<DeviceInfo>): DeviceInfo {
  return {
    device_kid: 'kid-1',
    device_name: 'Test Device',
    created_at: '2026-01-15T10:00:00Z',
    last_used_at: '2026-01-16T12:00:00Z',
    revoked_at: null,
    ...overrides,
  };
}

describe('DeviceList', () => {
  test('renders device table with column headers', () => {
    render(
      <DeviceList
        devices={[makeDevice()]}
        currentDeviceKid={null}
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.getByText('Name')).toBeInTheDocument();
    expect(screen.getByText('KID')).toBeInTheDocument();
    expect(screen.getByText('Status')).toBeInTheDocument();
    expect(screen.getByText('Actions')).toBeInTheDocument();
  });

  test('shows device name and truncated KID', () => {
    render(
      <DeviceList
        devices={[makeDevice({ device_kid: 'abcdef1234567890', device_name: 'My Laptop' })]}
        currentDeviceKid={null}
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.getByText('My Laptop')).toBeInTheDocument();
    expect(screen.getByText('abcdef12...')).toBeInTheDocument();
  });

  test('shows Current badge for the active device', () => {
    render(
      <DeviceList
        devices={[makeDevice({ device_kid: 'current-kid' })]}
        currentDeviceKid="current-kid"
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.getByText('Current')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
  });

  test('shows Active badge for non-revoked device', () => {
    render(
      <DeviceList
        devices={[makeDevice()]}
        currentDeviceKid={null}
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.getByText('Active')).toBeInTheDocument();
  });

  test('shows Revoked badge for revoked device', () => {
    render(
      <DeviceList
        devices={[makeDevice({ revoked_at: '2026-01-17T00:00:00Z' })]}
        currentDeviceKid={null}
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.getByText('Revoked')).toBeInTheDocument();
  });

  test('hides action buttons for current device', () => {
    render(
      <DeviceList
        devices={[makeDevice({ device_kid: 'my-kid' })]}
        currentDeviceKid="my-kid"
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.queryByLabelText('Rename')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Revoke')).not.toBeInTheDocument();
  });

  test('hides action buttons for revoked device', () => {
    render(
      <DeviceList
        devices={[makeDevice({ revoked_at: '2026-01-17T00:00:00Z' })]}
        currentDeviceKid={null}
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.queryByLabelText('Rename')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Revoke')).not.toBeInTheDocument();
  });

  test('renders multiple devices', () => {
    render(
      <DeviceList
        devices={[
          makeDevice({ device_kid: 'kid-1', device_name: 'Laptop' }),
          makeDevice({ device_kid: 'kid-2', device_name: 'Phone' }),
        ]}
        currentDeviceKid="kid-1"
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.getByText('Laptop')).toBeInTheDocument();
    expect(screen.getByText('Phone')).toBeInTheDocument();
  });

  test('clicking rename icon opens inline edit', async () => {
    const user = userEvent.setup();
    render(
      <DeviceList
        devices={[makeDevice({ device_kid: 'other-kid', device_name: 'Old Name' })]}
        currentDeviceKid="my-kid"
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    await user.click(screen.getByLabelText('Rename'));
    expect(screen.getByDisplayValue('Old Name')).toBeInTheDocument();
  });

  test('submitting rename calls onRename', async () => {
    const user = userEvent.setup();
    const onRename = vi.fn();
    render(
      <DeviceList
        devices={[makeDevice({ device_kid: 'other-kid', device_name: 'Old Name' })]}
        currentDeviceKid="my-kid"
        onRevoke={vi.fn()}
        onRename={onRename}
        isRevoking={false}
        isRenaming={false}
      />
    );

    await user.click(screen.getByLabelText('Rename'));
    const input = screen.getByDisplayValue('Old Name');
    await user.clear(input);
    await user.type(input, 'New Name{Enter}');

    expect(onRename).toHaveBeenCalledWith('other-kid', 'New Name');
  });

  test('pressing Escape cancels rename', async () => {
    const user = userEvent.setup();
    const onRename = vi.fn();
    render(
      <DeviceList
        devices={[makeDevice({ device_kid: 'other-kid', device_name: 'Old Name' })]}
        currentDeviceKid="my-kid"
        onRevoke={vi.fn()}
        onRename={onRename}
        isRevoking={false}
        isRenaming={false}
      />
    );

    await user.click(screen.getByLabelText('Rename'));
    const input = screen.getByDisplayValue('Old Name');
    await user.type(input, '{Escape}');

    expect(screen.getByText('Old Name')).toBeInTheDocument();
    expect(onRename).not.toHaveBeenCalled();
  });

  test('clicking revoke calls onRevoke', async () => {
    const user = userEvent.setup();
    const onRevoke = vi.fn();
    render(
      <DeviceList
        devices={[makeDevice({ device_kid: 'other-kid' })]}
        currentDeviceKid="my-kid"
        onRevoke={onRevoke}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    await user.click(screen.getByLabelText('Revoke'));
    expect(onRevoke).toHaveBeenCalledWith('other-kid');
  });

  test('displays em dash for null last_used_at', () => {
    render(
      <DeviceList
        devices={[makeDevice({ last_used_at: null })]}
        currentDeviceKid={null}
        onRevoke={vi.fn()}
        onRename={vi.fn()}
        isRevoking={false}
        isRenaming={false}
      />
    );

    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
  });
});
