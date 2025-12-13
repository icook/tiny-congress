/**
 * Device management screen
 * Add and revoke devices for account
 */

import { useState } from 'react';
import {
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { IconAlertTriangle, IconDevices, IconPlus } from '@tabler/icons-react';
import {
  canonicalizeToBytes,
  encodeBase64Url,
  generateKeyPair,
  getRootKey,
  keyPairToStored,
  sign,
  storeDeviceKey,
  storedToKeyPair,
} from '../keys';
import { useAddDevice } from '../api/queries';
import { useSession } from '../state/session';

export function Devices() {
  const { session } = useSession();
  const addDevice = useAddDevice();

  const [deviceName, setDeviceName] = useState('');
  const [isGeneratingKey, setIsGeneratingKey] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!session) {
    return (
      <Alert icon={<IconAlertTriangle size={16} />} title="Not authenticated" color="red">
        Please log in to manage devices
      </Alert>
    );
  }

  const handleAddDevice = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setIsGeneratingKey(true);

    try {
      // Load root key
      const storedRootKey = await getRootKey();
      if (!storedRootKey) {
        setError('Root key not found');
        return;
      }

      const rootKeyPair = storedToKeyPair(storedRootKey);

      // Generate new device key
      const newDeviceKeyPair = generateKeyPair();
      const newDeviceId = crypto.randomUUID();

      // Create delegation envelope
      const delegationPayload = {
        type: 'DeviceDelegation',
        device_id: newDeviceId,
        device_pubkey: encodeBase64Url(newDeviceKeyPair.publicKey),
        permissions: ['*'],
        created_at: new Date().toISOString(),
      };

      const canonicalPayload = canonicalizeToBytes(delegationPayload);
      const delegationSignature = sign(canonicalPayload, rootKeyPair.privateKey);

      const delegationEnvelope = {
        payload: delegationPayload,
        signer: {
          kid: rootKeyPair.kid,
          account_id: session.accountId,
        },
        signature: encodeBase64Url(delegationSignature),
      };

      // Call add device API
      await addDevice.mutateAsync({
        account_id: session.accountId,
        device_pubkey: encodeBase64Url(newDeviceKeyPair.publicKey),
        device_metadata: deviceName
          ? { name: deviceName, type: 'browser' }
          : { type: 'browser' },
        delegation_envelope: delegationEnvelope,
      });

      // Store new device key
      await storeDeviceKey(keyPairToStored(newDeviceKeyPair, deviceName || 'Device'));

      setDeviceName('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to add device');
    } finally {
      setIsGeneratingKey(false);
    }
  };

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <Group gap="xs">
        <IconDevices size={24} />
        <Title order={2}>Device Management</Title>
      </Group>

      <Text c="dimmed" size="sm">
        Manage devices authorized to access your account
      </Text>

      {/* Add Device Form */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <form onSubmit={handleAddDevice}>
          <Stack gap="md">
            <Group>
              <IconPlus size={16} />
              <Text fw={500}>Add New Device</Text>
            </Group>

            <TextInput
              label="Device Name"
              placeholder="My Phone"
              value={deviceName}
              onChange={(e) => setDeviceName(e.currentTarget.value)}
              disabled={addDevice.isPending || isGeneratingKey}
              description="Optional: Give this device a name"
            />

            {error && (
              <Alert icon={<IconAlertTriangle size={16} />} title="Error" color="red">
                {error}
              </Alert>
            )}

            {addDevice.isError && (
              <Alert icon={<IconAlertTriangle size={16} />} title="Failed to add device" color="red">
                {addDevice.error?.message || 'An error occurred'}
              </Alert>
            )}

            {addDevice.isSuccess && (
              <Alert color="green" title="Device added successfully">
                Your new device has been authorized
              </Alert>
            )}

            <Button
              type="submit"
              leftSection={<IconPlus size={16} />}
              loading={addDevice.isPending || isGeneratingKey}
            >
              {isGeneratingKey ? 'Generating key...' : 'Add Device'}
            </Button>
          </Stack>
        </form>
      </Card>

      {/* Device List */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Text fw={500}>Current Devices</Text>

          <Alert color="blue">
            <Text size="sm">
              Device list endpoint not yet implemented. This will show all authorized devices once
              the backend endpoint is ready.
            </Text>
          </Alert>

          {/* Placeholder current device */}
          <Card withBorder>
            <Group justify="space-between">
              <div>
                <Group gap="xs">
                  <Text fw={500}>Current Device</Text>
                  <Badge color="green">Active</Badge>
                </Group>
                <Text size="sm" c="dimmed">
                  Device ID: {session.deviceId}
                </Text>
              </div>
            </Group>
          </Card>
        </Stack>
      </Card>

      <Text size="xs" c="dimmed" ta="center">
        Each device has its own cryptographic key. Keep your devices secure.
      </Text>
    </Stack>
  );
}
