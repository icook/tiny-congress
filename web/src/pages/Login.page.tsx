/**
 * Login page - Route-level container
 * Handles hooks, crypto, and API calls for logging in with an existing account.
 * The root private key signs the device public key + timestamp to produce
 * a time-bound certificate that prevents replay attacks.
 */

import { useState } from 'react';
import { IconAlertTriangle, IconCheck } from '@tabler/icons-react';
import { Alert, Button, Card, Code, Group, Stack, Text, TextInput, Title } from '@mantine/core';
import { generateKeyPair, signMessage, useLogin } from '@/features/identity';
import { useCryptoRequired } from '@/providers/CryptoProvider';

function getDeviceName(): string {
  const uaParts = navigator.userAgent.split(' ');
  const lastPart = uaParts[uaParts.length - 1] ?? 'Unknown';
  return `Browser - ${lastPart}`;
}

export function LoginPage() {
  const crypto = useCryptoRequired();
  const loginMutation = useLogin();

  const [username, setUsername] = useState('');
  const [isGeneratingKeys, setIsGeneratingKeys] = useState(false);
  const [loggedInAccount, setLoggedInAccount] = useState<{
    account_id: string;
    root_kid: string;
    device_kid: string;
  } | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!username.trim()) {
      return;
    }

    setIsGeneratingKeys(true);

    try {
      // The user must provide their root private key to log in.
      // For now, we generate a fresh root key pair — this is a placeholder
      // until backup restore is implemented. In production, the root private
      // key comes from decrypting the backup envelope.
      const rootKeyPair = generateKeyPair(crypto);

      // Generate a new device key pair for this session
      const deviceKeyPair = generateKeyPair(crypto);

      // Build the timestamp-bound signed payload
      const timestamp = Math.floor(Date.now() / 1000);
      const timestampBytes = new Uint8Array(8);
      new DataView(timestampBytes.buffer).setBigInt64(0, BigInt(timestamp), true);

      const signedPayload = new Uint8Array(deviceKeyPair.publicKey.length + 8);
      signedPayload.set(deviceKeyPair.publicKey, 0);
      signedPayload.set(timestampBytes, deviceKeyPair.publicKey.length);

      const certificate = signMessage(signedPayload, rootKeyPair.privateKey);

      const response = await loginMutation.mutateAsync({
        username: username.trim(),
        timestamp,
        device: {
          pubkey: crypto.encode_base64url(deviceKeyPair.publicKey),
          name: getDeviceName(),
          certificate: crypto.encode_base64url(certificate),
        },
      });

      setLoggedInAccount(response);
    } catch {
      // Error is handled by TanStack Query mutation state
    } finally {
      setIsGeneratingKeys(false);
    }
  };

  if (loggedInAccount) {
    return (
      <Stack gap="md" maw={500} mx="auto" mt="xl">
        <Alert icon={<IconCheck size={16} />} title="Logged In" color="green">
          You have been logged in successfully.
        </Alert>

        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="sm">
            <Text fw={500}>Session Details</Text>
            <Text size="sm">
              <strong>Account ID:</strong> <Code>{loggedInAccount.account_id}</Code>
            </Text>
            <Text size="sm">
              <strong>Root Key ID:</strong> <Code>{loggedInAccount.root_kid}</Code>
            </Text>
            <Text size="sm">
              <strong>Device Key ID:</strong> <Code>{loggedInAccount.device_kid}</Code>
            </Text>
          </Stack>
        </Card>
      </Stack>
    );
  }

  return (
    <Stack gap="md" maw={500} mx="auto" mt="xl">
      <div>
        <Title order={2}>Log In</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Sign in to your TinyCongress account
        </Text>
      </div>

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <form
          onSubmit={(e) => {
            void handleSubmit(e);
          }}
        >
          <Stack gap="md">
            <TextInput
              label="Username"
              placeholder="alice"
              required
              value={username}
              onChange={(e) => {
                setUsername(e.currentTarget.value);
              }}
              disabled={loginMutation.isPending || isGeneratingKeys}
            />

            {loginMutation.isError ? (
              <Alert icon={<IconAlertTriangle size={16} />} title="Login failed" color="red">
                {loginMutation.error.message}
              </Alert>
            ) : null}

            <Group justify="flex-end">
              <Button type="submit" loading={loginMutation.isPending || isGeneratingKeys}>
                {isGeneratingKeys ? 'Generating keys...' : 'Log In'}
              </Button>
            </Group>
          </Stack>
        </form>
      </Card>
    </Stack>
  );
}
