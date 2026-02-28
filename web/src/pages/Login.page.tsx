/**
 * Login page — recover account on a new device
 * Fetches encrypted backup, decrypts with password, authorizes device
 */

import { useState } from 'react';
import { ed25519 } from '@noble/curves/ed25519.js';
import { IconAlertTriangle } from '@tabler/icons-react';
import { Link, useNavigate } from '@tanstack/react-router';
import {
  Alert,
  Button,
  Card,
  Group,
  PasswordInput,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import {
  decryptBackupEnvelope,
  fetchBackup,
  generateKeyPair,
  getDeviceName,
  signMessage,
  useLogin,
} from '@/features/identity';
import { useCryptoRequired } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

export function LoginPage() {
  const crypto = useCryptoRequired();
  const loginMutation = useLogin();
  const { setDevice } = useDevice();
  const navigate = useNavigate();

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [isDecrypting, setIsDecrypting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!username.trim() || !password) {
      return;
    }

    setIsDecrypting(true);

    try {
      // Fetch encrypted backup
      const backupResponse = await fetchBackup(username.trim());

      // Decode and decrypt backup
      const envelopeBytes = crypto.decode_base64url(backupResponse.encrypted_backup);
      const rootPrivateKey = await decryptBackupEnvelope(envelopeBytes, password);

      // Verify recovered key matches the account's root_kid.
      // Detects tampered or swapped backup envelopes.
      const recoveredPubkey = ed25519.getPublicKey(rootPrivateKey);
      const recoveredKid = crypto.derive_kid(recoveredPubkey);
      if (recoveredKid !== backupResponse.root_kid) {
        throw new Error('Backup integrity check failed: recovered key does not match account');
      }

      // Generate new device keypair
      const deviceKeyPair = generateKeyPair(crypto);

      // Sign device certificate with root key
      const certificate = signMessage(deviceKeyPair.publicKey, rootPrivateKey);

      setIsDecrypting(false);

      // Authorize new device
      const response = await loginMutation.mutateAsync({
        username: username.trim(),
        device: {
          pubkey: crypto.encode_base64url(deviceKeyPair.publicKey),
          name: getDeviceName(),
          certificate: crypto.encode_base64url(certificate),
        },
      });

      // Store device credentials
      setDevice(response.device_kid, deviceKeyPair.privateKey);

      // Navigate to settings
      void navigate({ to: '/settings' });
    } catch (err) {
      setIsDecrypting(false);
      if (err instanceof Error) {
        // Distinguish decryption failure from API errors
        if (err.message.includes('decrypt') || err.message.includes('tag')) {
          setError('Wrong password or corrupted backup');
        } else {
          setError(err.message);
        }
      } else {
        setError('Login failed');
      }
    }
  };

  const isLoading = isDecrypting || loginMutation.isPending;

  return (
    <Stack gap="md" maw={500} mx="auto" mt="xl">
      <div>
        <Title order={2}>Log In</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Recover your account on this device
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
              disabled={isLoading}
            />

            <PasswordInput
              label="Backup Password"
              required
              value={password}
              onChange={(e) => {
                setPassword(e.currentTarget.value);
              }}
              disabled={isLoading}
            />

            {error ? (
              <Alert icon={<IconAlertTriangle size={16} />} title="Login failed" color="red">
                {error}
              </Alert>
            ) : null}

            <Group justify="flex-end">
              <Button type="submit" loading={isLoading}>
                {isDecrypting ? 'Decrypting backup...' : 'Log In'}
              </Button>
            </Group>
          </Stack>
        </form>
      </Card>

      <Text size="xs" c="dimmed" ta="center">
        Don&apos;t have an account? <Link to="/signup">Sign up</Link>
      </Text>
    </Stack>
  );
}
