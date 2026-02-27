/**
 * Settings page - Device management
 */

import { IconAlertTriangle } from '@tabler/icons-react';
import { Alert, Card, Loader, Stack, Text, Title } from '@mantine/core';
import { useListDevices, useRenameDevice, useRevokeDevice } from '@/features/identity/api/queries';
import { DeviceList } from '@/features/identity/components';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

export function SettingsPage() {
  const { deviceKid, privateKey } = useDevice();
  const { crypto } = useCrypto();

  const devicesQuery = useListDevices(deviceKid, privateKey, crypto);
  const revokeMutation = useRevokeDevice(deviceKid, privateKey, crypto);
  const renameMutation = useRenameDevice(deviceKid, privateKey, crypto);

  if (!deviceKid) {
    return (
      <Stack gap="md" maw={800} mx="auto" mt="xl">
        <Title order={2}>Settings</Title>
        <Alert icon={<IconAlertTriangle size={16} />} color="yellow">
          You need to sign up or log in to manage devices.
        </Alert>
      </Stack>
    );
  }

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <div>
        <Title order={2}>Settings</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Manage your devices and signing keys
        </Text>
      </div>

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Title order={4}>Devices</Title>

          {devicesQuery.isLoading ? <Loader size="sm" /> : null}

          {devicesQuery.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to load devices: {devicesQuery.error.message}
            </Alert>
          ) : null}

          {revokeMutation.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to revoke device: {revokeMutation.error.message}
            </Alert>
          ) : null}

          {renameMutation.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to rename device: {renameMutation.error.message}
            </Alert>
          ) : null}

          {devicesQuery.data ? (
            <DeviceList
              devices={devicesQuery.data.devices}
              currentDeviceKid={deviceKid}
              onRevoke={(kid) => {
                revokeMutation.mutate(kid);
              }}
              onRename={(kid, name) => {
                renameMutation.mutate({ targetKid: kid, name });
              }}
              revokingKid={revokeMutation.isPending ? revokeMutation.variables : null}
              renamingKid={renameMutation.isPending ? renameMutation.variables.targetKid : null}
            />
          ) : null}
        </Stack>
      </Card>
    </Stack>
  );
}
