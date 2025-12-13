/**
 * Login screen
 * Challenge/response authentication with device key
 */

import { useState } from 'react';
import { IconAlertTriangle } from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import { Alert, Button, Card, Group, Stack, Text, TextInput, Title } from '@mantine/core';
import { useIssueChallenge, useVerifyChallenge } from '../api/queries';
import { canonicalizeToBytes, encodeBase64Url, getDeviceKey, sign, storedToKeyPair } from '../keys';
import { useSession } from '../state/session';

export function Login() {
  const navigate = useNavigate();
  const { setSession } = useSession();
  const issueChallenge = useIssueChallenge();
  const verifyChallenge = useVerifyChallenge();

  const [accountId, setAccountId] = useState('');
  const [deviceId, setDeviceId] = useState('');
  const [challengeId, setChallengeId] = useState<string | null>(null);
  const [nonce, setNonce] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleRequestChallenge = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!accountId.trim() || !deviceId.trim()) {
      setError('Account ID and Device ID are required');
      return;
    }

    try {
      const result = await issueChallenge.mutateAsync({
        account_id: accountId.trim(),
        device_id: deviceId.trim(),
      });

      setChallengeId(result.challenge_id);
      setNonce(result.nonce);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to issue challenge');
    }
  };

  const handleVerifyChallenge = async () => {
    if (!challengeId || !nonce) {
      setError('No active challenge');
      return;
    }

    setError(null);

    try {
      // Load device key from storage
      const storedKey = await getDeviceKey();
      if (!storedKey) {
        setError('No device key found. Please sign up first.');
        return;
      }

      const deviceKeyPair = storedToKeyPair(storedKey);

      // Create challenge response payload
      const payload = {
        challenge_id: challengeId,
        nonce,
        account_id: accountId.trim(),
        device_id: deviceId.trim(),
      };

      // Sign canonical JSON
      const canonical = canonicalizeToBytes(payload);
      const signatureBytes = sign(canonical, deviceKeyPair.privateKey);
      const signature = encodeBase64Url(signatureBytes);

      // Verify challenge
      const result = await verifyChallenge.mutateAsync({
        challenge_id: challengeId,
        account_id: accountId.trim(),
        device_id: deviceId.trim(),
        signature,
      });

      // Store session
      setSession({
        accountId: accountId.trim(),
        deviceId: deviceId.trim(),
        sessionId: result.session_id,
        expiresAt: new Date(result.expires_at),
      });

      // Navigate to account profile
      navigate({ to: '/account' });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to verify challenge');
    }
  };

  const isLoading = issueChallenge.isPending || verifyChallenge.isPending;

  return (
    <Stack gap="md" maw={500} mx="auto" mt="xl">
      <div>
        <Title order={2}>Login</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Sign in with your cryptographic device key
        </Text>
      </div>

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        {!challengeId ? (
          <form onSubmit={handleRequestChallenge}>
            <Stack gap="md">
              <TextInput
                label="Account ID"
                placeholder="00000000-0000-0000-0000-000000000000"
                required
                value={accountId}
                onChange={(e) => setAccountId(e.currentTarget.value)}
                disabled={isLoading}
              />

              <TextInput
                label="Device ID"
                placeholder="00000000-0000-0000-0000-000000000000"
                required
                value={deviceId}
                onChange={(e) => setDeviceId(e.currentTarget.value)}
                disabled={isLoading}
              />

              {error && (
                <Alert icon={<IconAlertTriangle size={16} />} title="Error" color="red">
                  {error}
                </Alert>
              )}

              <Group justify="space-between">
                <Button variant="subtle" onClick={() => navigate({ to: '/' })} disabled={isLoading}>
                  Back to Home
                </Button>

                <Button type="submit" loading={isLoading}>
                  Request Challenge
                </Button>
              </Group>
            </Stack>
          </form>
        ) : (
          <Stack gap="md">
            <Alert color="blue" title="Challenge Issued">
              <Text size="sm">Challenge ID: {challengeId}</Text>
              <Text size="sm" mt="xs">
                Click below to sign the challenge with your device key and complete login.
              </Text>
            </Alert>

            {error && (
              <Alert icon={<IconAlertTriangle size={16} />} title="Error" color="red">
                {error}
              </Alert>
            )}

            <Group justify="space-between">
              <Button
                variant="subtle"
                onClick={() => {
                  setChallengeId(null);
                  setNonce(null);
                  setError(null);
                }}
                disabled={isLoading}
              >
                Cancel
              </Button>

              <Button onClick={handleVerifyChallenge} loading={isLoading}>
                Sign & Verify
              </Button>
            </Group>
          </Stack>
        )}
      </Card>

      <Text size="xs" c="dimmed" ta="center">
        Don't have an account?{' '}
        <Text span c="blue" style={{ cursor: 'pointer' }} onClick={() => navigate({ to: '/' })}>
          Sign up
        </Text>
      </Text>
    </Stack>
  );
}
