/**
 * Signup screen
 * Creates new account with Ed25519 key pair
 */

import { useState } from 'react';
import { IconAlertTriangle, IconCheck } from '@tabler/icons-react';
import { Alert, Button, Card, Code, Group, Stack, Text, TextInput, Title } from '@mantine/core';
import { useCryptoRequired } from '@/providers/CryptoProvider';
import { useSignup } from '../api/queries';
import { generateKeyPair } from '../keys';

export function Signup() {
  const crypto = useCryptoRequired();
  const signup = useSignup();

  const [username, setUsername] = useState('');
  const [isGeneratingKeys, setIsGeneratingKeys] = useState(false);
  const [createdAccount, setCreatedAccount] = useState<{
    account_id: string;
    root_kid: string;
  } | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!username.trim()) {
      return;
    }

    setIsGeneratingKeys(true);

    try {
      // Generate key pair (uses WASM for KID derivation)
      const keyPair = generateKeyPair(crypto);

      // Call signup API
      const response = await signup.mutateAsync({
        username: username.trim(),
        root_pubkey: crypto.encode_base64url(keyPair.publicKey),
      });

      setCreatedAccount(response);
    } catch {
      // Error is handled by TanStack Query mutation state
    } finally {
      setIsGeneratingKeys(false);
    }
  };

  if (createdAccount) {
    return (
      <Stack gap="md" maw={500} mx="auto" mt="xl">
        <Alert icon={<IconCheck size={16} />} title="Account Created" color="green">
          Your account has been created successfully.
        </Alert>

        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="sm">
            <Text fw={500}>Account Details</Text>
            <Text size="sm">
              <strong>Account ID:</strong> <Code>{createdAccount.account_id}</Code>
            </Text>
            <Text size="sm">
              <strong>Root Key ID:</strong> <Code>{createdAccount.root_kid}</Code>
            </Text>
          </Stack>
        </Card>

        <Text size="xs" c="dimmed" ta="center">
          Your keys were generated locally.
          <br />
          (Key persistence will be added in a future update)
        </Text>
      </Stack>
    );
  }

  return (
    <Stack gap="md" maw={500} mx="auto" mt="xl">
      <div>
        <Title order={2}>Create Account</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Sign up for TinyCongress with cryptographic identity
        </Text>
      </div>

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <form onSubmit={handleSubmit}>
          <Stack gap="md">
            <TextInput
              label="Username"
              placeholder="alice"
              required
              value={username}
              onChange={(e) => setUsername(e.currentTarget.value)}
              disabled={signup.isPending || isGeneratingKeys}
            />

            {signup.isError && (
              <Alert icon={<IconAlertTriangle size={16} />} title="Signup failed" color="red">
                {signup.error?.message || 'An error occurred'}
              </Alert>
            )}

            <Group justify="flex-end">
              <Button type="submit" loading={signup.isPending || isGeneratingKeys}>
                {isGeneratingKeys ? 'Generating keys...' : 'Sign Up'}
              </Button>
            </Group>
          </Stack>
        </form>
      </Card>

      <Text size="xs" c="dimmed" ta="center">
        Your keys are generated locally and never leave your device.
      </Text>
    </Stack>
  );
}
