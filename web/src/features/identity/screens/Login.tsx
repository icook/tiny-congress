/**
 * Login screen - device-key login using challenge/response
 */

import { useEffect, useState } from 'react';
import { IconAlertCircle, IconClock } from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import { Alert, Button, Container, Paper, Stack, Text, TextInput, Title } from '@mantine/core';
import { useForm } from '@mantine/form';
import { issueChallenge, verifyChallenge } from '../api/client';
import { getDevicePrivateKey, hasDeviceKey, signChallenge } from '../keys';
import { getSession, saveSession } from '../state/session';

interface LoginFormValues {
  accountId: string;
  deviceId: string;
}

interface ChallengeState {
  challengeId: string;
  nonce: string;
  expiresAt: Date;
}

export function Login() {
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [challenge, setChallenge] = useState<ChallengeState | null>(null);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [timeLeft, setTimeLeft] = useState<number>(0);

  const form = useForm<LoginFormValues>({
    initialValues: {
      accountId: '',
      deviceId: '',
    },
    validate: {
      accountId: (value) => {
        if (!value) {
          return 'Account ID is required';
        }
        return null;
      },
      deviceId: (value) => {
        if (!value) {
          return 'Device ID is required';
        }
        return null;
      },
    },
  });

  // Pre-populate from session if available (only on mount)
  useEffect(() => {
    const session = getSession();
    if (session) {
      form.setValues({
        accountId: session.accountId,
        deviceId: session.deviceId,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Check if device key exists when account ID changes
  useEffect(() => {
    const checkKey = async () => {
      if (form.values.accountId) {
        const exists = await hasDeviceKey(form.values.accountId);
        setHasKey(exists);
      } else {
        setHasKey(null);
      }
    };
    checkKey();
  }, [form.values.accountId]);

  // Countdown timer for challenge expiry
  useEffect(() => {
    if (!challenge) {
      return;
    }

    const updateTimeLeft = () => {
      const remaining = Math.max(
        0,
        Math.floor((challenge.expiresAt.getTime() - Date.now()) / 1000)
      );
      setTimeLeft(remaining);
    };

    updateTimeLeft();
    const interval = setInterval(updateTimeLeft, 1000);
    return () => clearInterval(interval);
  }, [challenge]);

  const handleRequestChallenge = async () => {
    setLoading(true);
    setError(null);

    try {
      const response = await issueChallenge({
        account_id: form.values.accountId,
        device_id: form.values.deviceId,
      });

      setChallenge({
        challengeId: response.challenge_id,
        nonce: response.nonce,
        expiresAt: new Date(response.expires_at),
      });
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to request challenge');
      }
    } finally {
      setLoading(false);
    }
  };

  const handleVerify = async () => {
    if (!challenge) {
      return;
    }

    setLoading(true);
    setError(null);

    try {
      // Get device private key
      const privateKey = await getDevicePrivateKey(form.values.accountId);

      // Sign the challenge
      const signature = signChallenge(
        challenge.challengeId,
        challenge.nonce,
        form.values.accountId,
        form.values.deviceId,
        privateKey
      );

      // Verify with backend
      const response = await verifyChallenge({
        challenge_id: challenge.challengeId,
        account_id: form.values.accountId,
        device_id: form.values.deviceId,
        signature,
      });

      // Save session
      saveSession({
        accountId: form.values.accountId,
        deviceId: form.values.deviceId,
        sessionToken: response.token,
        expiresAt: response.expires_at,
        username: '', // Will be fetched from profile
      });

      // Navigate to dashboard
      navigate({ to: '/dashboard' });
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Login failed');
      }
      setChallenge(null); // Clear challenge on failure
    } finally {
      setLoading(false);
    }
  };

  const isExpired = challenge && timeLeft === 0;
  const canVerify = challenge && !isExpired && hasKey;

  return (
    <Container size="xs" mt="xl">
      <Paper withBorder shadow="md" p="xl" radius="md">
        <Title order={2} mb="md">
          Login
        </Title>

        <Stack>
          <TextInput
            label="Account ID"
            placeholder="Enter your account ID"
            required
            {...form.getInputProps('accountId')}
            disabled={loading || !!challenge}
          />

          <TextInput
            label="Device ID"
            placeholder="Enter your device ID"
            required
            {...form.getInputProps('deviceId')}
            disabled={loading || !!challenge}
          />

          {hasKey === false && (
            <Alert icon={<IconAlertCircle size={16} />} color="yellow">
              No device key found for this account. You may need to add this device first.
            </Alert>
          )}

          {error && (
            <Alert icon={<IconAlertCircle size={16} />} color="red">
              {error}
            </Alert>
          )}

          {!challenge ? (
            <Button onClick={handleRequestChallenge} loading={loading} disabled={hasKey === false}>
              Request Challenge
            </Button>
          ) : (
            <>
              <Alert
                icon={<IconClock size={16} />}
                color={isExpired ? 'red' : 'blue'}
                title={isExpired ? 'Challenge Expired' : 'Challenge Ready'}
              >
                {isExpired ? (
                  'Challenge has expired. Please request a new one.'
                ) : (
                  <Text size="sm">Sign and verify within {timeLeft} seconds</Text>
                )}
              </Alert>

              <Button onClick={handleVerify} loading={loading} disabled={!canVerify}>
                Verify & Login
              </Button>

              {isExpired && (
                <Button variant="outline" onClick={() => setChallenge(null)}>
                  Request New Challenge
                </Button>
              )}
            </>
          )}
        </Stack>
      </Paper>
    </Container>
  );
}
