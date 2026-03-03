/**
 * Login page - Route-level container
 * Handles hooks, crypto, and API calls for logging in with an existing account.
 * The root private key is recovered by decrypting the backup envelope with the
 * user's password. It then signs the device public key + timestamp to produce
 * a time-bound certificate that prevents replay attacks.
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
  generateDeviceKeyPair,
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
  const [isGeneratingKeys, setIsGeneratingKeys] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLocalError(null);
    loginMutation.reset();

    if (!username.trim() || !password) {
      return;
    }

    setIsGeneratingKeys(true);

    try {
      // 1. Fetch the encrypted backup envelope from the server
      const backupResponse = await fetchBackup(username.trim());
      const envelopeBytes = crypto.decode_base64url(backupResponse.encrypted_backup);

      // 2. Decrypt the envelope with the user's password to recover the root private key
      const rootPrivateKey = await decryptBackupEnvelope(envelopeBytes, password);

      // 2b. Defense-in-depth: verify the recovered key matches the account's root_kid.
      // Catches tampered or swapped backup envelopes client-side.
      const recoveredPubkey = ed25519.getPublicKey(rootPrivateKey);
      const recoveredKid = crypto.derive_kid(recoveredPubkey);
      if (recoveredKid !== backupResponse.root_kid) {
        throw new Error('Backup integrity check failed: recovered key does not match account');
      }

      // 3. Generate a new device key pair (non-extractable CryptoKey via Web Crypto)
      const deviceKeyPair = await generateDeviceKeyPair();

      // 4. Build the timestamp-bound signed payload using the RECOVERED root key
      const timestamp = Math.floor(Date.now() / 1000);
      const timestampBytes = new Uint8Array(8);
      new DataView(timestampBytes.buffer).setBigInt64(0, BigInt(timestamp), true);

      const signedPayload = new Uint8Array(deviceKeyPair.publicKey.length + 8);
      signedPayload.set(deviceKeyPair.publicKey, 0);
      signedPayload.set(timestampBytes, deviceKeyPair.publicKey.length);

      const certificate = signMessage(signedPayload, rootPrivateKey);

      const response = await loginMutation.mutateAsync({
        username: username.trim(),
        timestamp,
        device: {
          pubkey: crypto.encode_base64url(deviceKeyPair.publicKey),
          name: getDeviceName(),
          certificate: crypto.encode_base64url(certificate),
        },
      });

      // Store device credentials in session context
      setDevice(response.device_kid, deviceKeyPair.privateKey);

      // Navigate to settings page to show device list
      void navigate({ to: '/settings' });
    } catch (err) {
      if (err instanceof Error) {
        setLocalError(err.message);
      }
    } finally {
      setIsGeneratingKeys(false);
    }
  };

  const displayError = localError;

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

            <PasswordInput
              label="Backup password"
              placeholder="Enter your backup password"
              required
              value={password}
              onChange={(e) => {
                setPassword(e.currentTarget.value);
              }}
              disabled={loginMutation.isPending || isGeneratingKeys}
            />

            {displayError ? (
              <Alert icon={<IconAlertTriangle size={16} />} title="Login failed" color="red">
                {displayError}
              </Alert>
            ) : null}

            <Group justify="flex-end">
              <Button type="submit" loading={loginMutation.isPending || isGeneratingKeys}>
                {isGeneratingKeys ? 'Decrypting backup...' : 'Log In'}
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
