/**
 * Signup screen
 * Creates new account with root key and first device
 */

import { useState } from 'react';
import { IconAlertTriangle } from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import { Alert, Button, Card, Group, Stack, Text, TextInput, Title } from '@mantine/core';
import { useSignup } from '../api/queries';
import {
  canonicalizeToBytes,
  encodeBase64Url,
  generateKeyPair,
  keyPairToStored,
  sign,
  storeDeviceKey,
  storeRootKey,
} from '../keys';

export function Signup() {
  const navigate = useNavigate();
  const signup = useSignup();

  const [username, setUsername] = useState('');
  const [deviceName, setDeviceName] = useState('');
  const [isGeneratingKeys, setIsGeneratingKeys] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!username.trim()) {
      return;
    }

    setIsGeneratingKeys(true);

    try {
      // Generate root and device keys
      const rootKeyPair = generateKeyPair();
      const deviceKeyPair = generateKeyPair();

      // Create delegation envelope (root delegates to device)
      const accountId = crypto.randomUUID();
      const deviceId = crypto.randomUUID();

      const delegationPayload = {
        device_id: deviceId,
        device_pubkey: encodeBase64Url(deviceKeyPair.publicKey),
        permissions: ['*'], // Full permissions for first device
        created_at: new Date().toISOString(),
      };

      const signer = {
        kid: rootKeyPair.kid,
        account_id: accountId,
      };

      // Canonical signing bytes: payload_type + payload + signer
      const signingTarget = {
        payload_type: 'DeviceDelegation',
        payload: delegationPayload,
        signer,
      };
      const canonicalBytes = canonicalizeToBytes(signingTarget);
      const delegationSignature = sign(canonicalBytes, rootKeyPair.privateKey);

      const delegationEnvelope = {
        v: 1,
        payload_type: 'DeviceDelegation',
        payload: delegationPayload,
        signer,
        sig: encodeBase64Url(delegationSignature),
      };

      // Call signup API
      await signup.mutateAsync({
        username: username.trim(),
        root_pubkey: encodeBase64Url(rootKeyPair.publicKey),
        device_pubkey: encodeBase64Url(deviceKeyPair.publicKey),
        device_metadata: deviceName ? { name: deviceName, type: 'browser' } : { type: 'browser' },
        delegation_envelope: delegationEnvelope,
      });

      // Store keys locally
      await storeRootKey(keyPairToStored(rootKeyPair, 'Root Key'));
      await storeDeviceKey(keyPairToStored(deviceKeyPair, deviceName || 'My Device'));

      // Navigate to login to complete authentication
      navigate({ to: '/login' });
    } catch {
      // Error is handled by TanStack Query mutation state
    } finally {
      setIsGeneratingKeys(false);
    }
  };

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

            <TextInput
              label="Device Name"
              placeholder="My Laptop"
              value={deviceName}
              onChange={(e) => setDeviceName(e.currentTarget.value)}
              disabled={signup.isPending || isGeneratingKeys}
              description="Optional: Give this device a name"
            />

            {signup.isError && (
              <Alert icon={<IconAlertTriangle size={16} />} title="Signup failed" color="red">
                {signup.error?.message || 'An error occurred'}
              </Alert>
            )}

            <Group justify="space-between">
              <Button
                variant="subtle"
                onClick={() => navigate({ to: '/login' })}
                disabled={signup.isPending || isGeneratingKeys}
              >
                Already have an account?
              </Button>

              <Button type="submit" loading={signup.isPending || isGeneratingKeys}>
                {isGeneratingKeys ? 'Generating keys...' : 'Sign Up'}
              </Button>
            </Group>
          </Stack>
        </form>
      </Card>

      <Text size="xs" c="dimmed" ta="center">
        Your keys are generated locally and never leave your device.
        <br />
        Make sure to back up your root key after signup.
      </Text>
    </Stack>
  );
}
