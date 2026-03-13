/**
 * Settings page - Device management
 */

import { IconAlertTriangle, IconCheck, IconShieldOff } from '@tabler/icons-react';
import { Alert, Badge, Button, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useListDevices, useRenameDevice, useRevokeDevice } from '@/features/identity/api/queries';
import { DeviceList } from '@/features/identity/components';
import { TrustScoreCard } from '@/features/trust';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

export function SettingsPage() {
  const { deviceKid, privateKey, username } = useDevice();
  const { crypto } = useCrypto();

  const devicesQuery = useListDevices(deviceKid, privateKey, crypto);
  const revokeMutation = useRevokeDevice(deviceKid, privateKey, crypto);
  const renameMutation = useRenameDevice(deviceKid, privateKey, crypto);
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;
  const verifiedAt = verificationQuery.data?.verifiedAt;

  const currentDevice = devicesQuery.data?.devices.find((d) => d.device_kid === deviceKid);

  // Defensive safety net — route guard guarantees deviceKid is set
  if (!deviceKid) {
    return null;
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
        <Stack gap="xs">
          <Title order={4}>Account</Title>
          <Group gap="xs">
            <Text size="sm" c="dimmed" w={120}>
              Username
            </Text>
            <Text size="sm" fw={500}>
              {username}
            </Text>
          </Group>
          {currentDevice ? (
            <Group gap="xs">
              <Text size="sm" c="dimmed" w={120}>
                Current device
              </Text>
              <Text size="sm" fw={500}>
                {currentDevice.device_name}
              </Text>
            </Group>
          ) : null}
        </Stack>
      </Card>

      <TrustScoreCard deviceKid={deviceKid} privateKey={privateKey} wasmCrypto={crypto} />

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
                revokeMutation.mutate(kid, {
                  onSuccess: () => {
                    notifications.show({
                      title: 'Device revoked',
                      message: 'The device can no longer sign requests.',
                      color: 'green',
                      icon: <IconCheck size={16} />,
                      autoClose: 3000,
                    });
                  },
                });
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

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Title order={4}>Verification</Title>

          {verificationQuery.isLoading ? <Loader size="sm" /> : null}

          {verificationQuery.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to check verification status: {verificationQuery.error.message}
            </Alert>
          ) : null}

          {isVerified ? (
            <Stack gap="xs">
              <Group gap="sm">
                <Badge color="green" leftSection={<IconCheck size={12} />} variant="light">
                  Verified
                </Badge>
                {verifiedAt ? (
                  <Text size="sm" c="dimmed">
                    Since {new Date(verifiedAt).toLocaleDateString()}
                  </Text>
                ) : null}
              </Group>
              <Text size="sm" c="dimmed">
                Your identity is verified — you can vote in any room.
              </Text>
            </Stack>
          ) : null}

          {!isVerified && !verificationQuery.isLoading ? (
            <>
              <Group gap="sm">
                <Badge color="yellow" leftSection={<IconShieldOff size={12} />} variant="light">
                  Not Verified
                </Badge>
              </Group>
              <Text size="sm" c="dimmed">
                Verify your identity to unlock voting. This is a one-time step.
              </Text>
              {(() => {
                const url = buildVerifierUrl(username ?? '');
                if (url) {
                  return (
                    <Button component="a" href={url} variant="light" size="sm" w="fit-content">
                      Verify Identity
                    </Button>
                  );
                }
                return null;
              })()}
            </>
          ) : null}
        </Stack>
      </Card>
    </Stack>
  );
}
