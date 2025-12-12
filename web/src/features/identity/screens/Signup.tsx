/**
 * Signup screen - creates account with root key + first device delegation
 */

import { useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { Button, Container, Paper, Stack, TextInput, Title } from '@mantine/core';
import { useForm } from '@mantine/form';
import { signup, type SignupRequest } from '../api/client';
import {
  deriveKid,
  generateDeviceKey,
  generateRootKey,
  signEnvelope,
  storeDeviceKey,
  storeRootKeyTemporary,
} from '../keys';
import { saveSession } from '../state/session';

interface SignupFormValues {
  username: string;
  deviceName: string;
}

export function Signup() {
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const form = useForm<SignupFormValues>({
    initialValues: {
      username: '',
      deviceName: '',
    },
    validate: {
      username: (value) => {
        if (!value) {
          return 'Username is required';
        }
        if (value.length < 3) {
          return 'Username must be at least 3 characters';
        }
        if (!/^[a-zA-Z0-9_-]+$/.test(value)) {
          return 'Username can only contain letters, numbers, hyphens, and underscores';
        }
        return null;
      },
      deviceName: (value) => {
        if (!value) {
          return 'Device name is required';
        }
        if (value.length < 1) {
          return 'Device name is required';
        }
        return null;
      },
    },
  });

  const handleSubmit = async (values: SignupFormValues) => {
    setLoading(true);
    setError(null);

    try {
      // Generate root and device keys
      const rootKeyPair = generateRootKey();
      const rootKid = deriveKid(rootKeyPair.publicKey);

      const deviceKey = generateDeviceKey(values.deviceName);
      const deviceKid = deviceKey.kid;

      // Create device delegation payload
      const delegationPayload = {
        device_id: null, // Will be assigned by backend
        device_pubkey: deviceKey.publicKey,
        capabilities: ['read', 'write', 'sign'],
        device_metadata: {
          name: values.deviceName,
          type: 'browser',
          os: navigator.platform,
        },
        ctime: new Date().toISOString(),
        prev_hash: null,
      };

      // Sign delegation envelope with root key
      const delegationEnvelope = signEnvelope(
        'DeviceDelegation',
        delegationPayload,
        {
          kid: rootKid,
        },
        rootKeyPair.privateKey
      );

      // Call backend signup endpoint
      const signupRequest: SignupRequest = {
        username: values.username,
        root_kid: rootKid,
        root_pubkey: Buffer.from(rootKeyPair.publicKey).toString('base64url'),
        device_kid: deviceKid,
        device_pubkey: deviceKey.publicKey,
        device_metadata: {
          name: values.deviceName,
          type: 'browser',
          os: navigator.platform,
        },
        delegation_envelope: delegationEnvelope,
      };

      const response = await signup(signupRequest);

      // Store device key and session
      await storeDeviceKey(response.account_id, {
        ...deviceKey,
        label: values.deviceName,
      });

      // Temporarily store root key for recovery kit export
      await storeRootKeyTemporary(response.account_id, {
        kid: rootKid,
        publicKey: Buffer.from(rootKeyPair.publicKey).toString('base64url'),
        privateKey: Buffer.from(rootKeyPair.privateKey).toString('base64url'),
        createdAt: new Date().toISOString(),
      });

      // Save session (note: backend doesn't return token on signup yet,
      // so we'll need to login separately)
      saveSession({
        accountId: response.account_id,
        deviceId: response.device_id,
        sessionToken: '', // Will be set after login
        expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
        username: response.username,
      });

      // Navigate to dashboard on success
      navigate({ to: '/dashboard' });
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Signup failed. Please try again.');
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <Container size="xs" mt="xl">
      <Paper withBorder shadow="md" p="xl" radius="md">
        <Title order={2} mb="md">
          Create Account
        </Title>

        <form onSubmit={form.onSubmit(handleSubmit)}>
          <Stack>
            <TextInput
              label="Username"
              placeholder="Enter your username"
              required
              {...form.getInputProps('username')}
              disabled={loading}
            />

            <TextInput
              label="Device Name"
              placeholder="e.g., My Laptop"
              required
              {...form.getInputProps('deviceName')}
              disabled={loading}
            />

            {error && <div style={{ color: 'red', fontSize: '0.875rem' }}>{error}</div>}

            <Button type="submit" fullWidth loading={loading}>
              Sign Up
            </Button>
          </Stack>
        </form>
      </Paper>
    </Container>
  );
}
