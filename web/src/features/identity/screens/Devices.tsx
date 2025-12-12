/**
 * Device management screen - list, add, and revoke devices
 */

import { useCallback, useEffect, useState } from 'react';
import { IconDevices, IconPlus, IconTrash } from '@tabler/icons-react';
import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Card,
  Container,
  Group,
  Modal,
  Paper,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { listDevices, type Device } from '../api/client';
import { getSession } from '../state/session';

export function Devices() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [addModalOpened, { open: openAddModal, close: closeAddModal }] = useDisclosure(false);

  const fetchDevices = useCallback(async () => {
    const session = getSession();
    if (!session?.sessionToken) {
      setError('Please login to view devices');
      setLoading(false);
      return;
    }

    try {
      const deviceList = await listDevices(session.sessionToken);
      setDevices(deviceList);
      setError(null);
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to load devices');
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDevices();
  }, [fetchDevices]);

  const activeDevices = devices.filter((d) => !d.revoked_at);
  const revokedDevices = devices.filter((d) => d.revoked_at);

  return (
    <Container size="md" mt="xl">
      <Paper withBorder shadow="md" p="xl" radius="md">
        <Group justify="space-between" mb="lg">
          <Group>
            <IconDevices size={28} />
            <Title order={2}>Devices</Title>
          </Group>
          <Button leftSection={<IconPlus size={16} />} onClick={openAddModal}>
            Add Device
          </Button>
        </Group>

        {error && (
          <Alert color="red" mb="md">
            {error}
          </Alert>
        )}

        {loading ? (
          <Text>Loading devices...</Text>
        ) : (
          <Stack gap="md">
            {activeDevices.length === 0 && revokedDevices.length === 0 && (
              <Text c="dimmed">No devices found</Text>
            )}

            {activeDevices.map((device) => (
              <DeviceCard key={device.device_id} device={device} onRefresh={fetchDevices} />
            ))}

            {revokedDevices.length > 0 && (
              <>
                <Title order={4} mt="lg">
                  Revoked Devices
                </Title>
                {revokedDevices.map((device) => (
                  <DeviceCard key={device.device_id} device={device} onRefresh={fetchDevices} />
                ))}
              </>
            )}
          </Stack>
        )}
      </Paper>

      <AddDeviceModal opened={addModalOpened} onClose={closeAddModal} onSuccess={fetchDevices} />
    </Container>
  );
}

interface DeviceCardProps {
  device: Device;
  onRefresh: () => void;
}

function DeviceCard({ device, onRefresh }: DeviceCardProps) {
  const isRevoked = !!device.revoked_at;
  const [revoking, setRevoking] = useState(false);
  const [confirmOpened, { open: openConfirm, close: closeConfirm }] = useDisclosure(false);

  const handleRevoke = async () => {
    setRevoking(true);
    // Note: Full revocation requires root key signing - simplified for now
    // eslint-disable-next-line no-console
    console.log('Device revocation requires root key signing. Not yet implemented.');
    setRevoking(false);
    closeConfirm();
    onRefresh();
  };

  return (
    <Card withBorder padding="md" radius="md" style={{ opacity: isRevoked ? 0.6 : 1 }}>
      <Group justify="space-between">
        <Stack gap="xs">
          <Group gap="sm">
            <Text fw={500}>{device.device_metadata.name}</Text>
            {isRevoked && (
              <Badge color="red" size="sm">
                Revoked
              </Badge>
            )}
          </Group>
          <Text size="sm" c="dimmed">
            {device.device_metadata.type} • {device.device_metadata.os || 'Unknown OS'}
          </Text>
          <Text size="xs" c="dimmed">
            Added: {new Date(device.created_at).toLocaleDateString()}
            {device.last_seen && ` • Last seen: ${new Date(device.last_seen).toLocaleDateString()}`}
          </Text>
        </Stack>

        {!isRevoked && (
          <ActionIcon
            color="red"
            variant="subtle"
            onClick={openConfirm}
            loading={revoking}
            title="Revoke device"
          >
            <IconTrash size={18} />
          </ActionIcon>
        )}
      </Group>

      <Modal opened={confirmOpened} onClose={closeConfirm} title="Confirm Revocation">
        <Stack>
          <Text>Are you sure you want to revoke this device? This action cannot be undone.</Text>
          <Group justify="flex-end">
            <Button variant="outline" onClick={closeConfirm}>
              Cancel
            </Button>
            <Button color="red" onClick={handleRevoke} loading={revoking}>
              Revoke Device
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Card>
  );
}

interface AddDeviceModalProps {
  opened: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

function AddDeviceModal({ opened, onClose, onSuccess }: AddDeviceModalProps) {
  const [deviceName, setDeviceName] = useState('');

  const handleAdd = () => {
    // Note: Full add device flow requires QR code generation and root key signing
    // eslint-disable-next-line no-console
    console.log(
      'Device addition requires QR code handoff and root key signing. Not yet implemented.'
    );
    onClose();
    onSuccess();
  };

  return (
    <Modal opened={opened} onClose={onClose} title="Add New Device">
      <Stack>
        <Text size="sm" c="dimmed">
          To add a new device, you'll need to scan a QR code from the new device and sign a
          delegation with your root key.
        </Text>

        <TextInput
          label="Device Name"
          placeholder="e.g., Work Laptop"
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
        />

        <Button onClick={handleAdd} disabled={!deviceName}>
          Generate QR Code
        </Button>
      </Stack>
    </Modal>
  );
}
