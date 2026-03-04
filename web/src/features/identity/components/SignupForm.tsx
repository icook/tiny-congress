/**
 * SignupForm - Presentational component for signup
 * Renders the signup form UI based on props, no hooks or side effects
 */

import { IconAlertTriangle, IconCheck } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
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

export interface SignupFormProps {
  // Form state
  username: string;
  password: string;
  onUsernameChange: (value: string) => void;
  onPasswordChange: (value: string) => void;
  onSubmit: (e: React.FormEvent) => void;

  // Loading states
  isLoading: boolean;
  loadingText?: string;

  // Error state
  error?: string | null;

  // Success state
  successData?: {
    account_id: string;
    root_kid: string;
    device_kid: string;
  } | null;

  // Optional verifier URL for post-signup verification
  verifierUrl?: string | null;
}

export function SignupForm({
  username,
  password,
  onUsernameChange,
  onPasswordChange,
  onSubmit,
  isLoading,
  loadingText,
  error,
  successData,
  verifierUrl,
}: SignupFormProps) {
  if (successData) {
    return (
      <Stack gap="md" maw={500} mx="auto" mt="xl">
        <Alert icon={<IconCheck size={16} />} title="Account Created" color="green">
          Your account has been created successfully.
        </Alert>

        <Stack gap="sm">
          <Title order={3}>What&apos;s next?</Title>
          <Text size="sm">
            {verifierUrl
              ? 'Verify your identity to start voting in rooms.'
              : 'You\u0027re all set! Start exploring rooms and voting on polls.'}
          </Text>
          <Group mt="xs">
            {verifierUrl ? (
              <Button component="a" href={verifierUrl}>
                Verify Identity
              </Button>
            ) : null}
            <Button component={Link} to="/rooms" variant={verifierUrl ? 'outline' : 'filled'}>
              Browse Rooms
            </Button>
            <Button component={Link} to="/settings" variant="outline">
              View Settings
            </Button>
          </Group>
        </Stack>

        <Text size="xs" c="dimmed" ta="center">
          Your keys were generated locally and stored in this browser session.
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
        <form onSubmit={onSubmit}>
          <Stack gap="md">
            <TextInput
              label="Username"
              placeholder="alice"
              required
              value={username}
              onChange={(e) => {
                onUsernameChange(e.currentTarget.value);
              }}
              disabled={isLoading}
            />

            <PasswordInput
              label="Backup Password"
              description="Used to encrypt your root key backup. You'll need this to log in on new devices."
              required
              value={password}
              onChange={(e) => {
                onPasswordChange(e.currentTarget.value);
              }}
              disabled={isLoading}
            />

            {error ? (
              <Alert icon={<IconAlertTriangle size={16} />} title="Signup failed" color="red">
                {error}
              </Alert>
            ) : null}

            <Group justify="flex-end">
              <Button type="submit" loading={isLoading}>
                {loadingText ?? 'Sign Up'}
              </Button>
            </Group>
          </Stack>
        </form>
      </Card>

      <Text size="xs" c="dimmed" ta="center">
        Your keys are generated locally and never leave your device.
      </Text>

      <Text size="xs" c="dimmed" ta="center">
        Already have an account? <Link to="/login">Log in</Link>
      </Text>
    </Stack>
  );
}
