/**
 * SignupForm - Presentational component for signup
 * Renders the signup form UI based on props, no hooks or side effects
 */

import { IconAlertTriangle, IconCheck, IconKey, IconShield } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import {
  Alert,
  Anchor,
  Button,
  Card,
  Group,
  List,
  PasswordInput,
  Stack,
  Text,
  TextInput,
  ThemeIcon,
  Title,
} from '@mantine/core';

export interface SignupFormProps {
  // Form state
  username: string;
  password: string;
  passwordConfirm: string;
  onUsernameChange: (value: string) => void;
  onPasswordChange: (value: string) => void;
  onPasswordConfirmChange: (value: string) => void;
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
  passwordConfirm,
  onUsernameChange,
  onPasswordChange,
  onPasswordConfirmChange,
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
        <Alert icon={<IconCheck size={16} />} title={`Welcome, ${username}!`} color="green">
          Your account has been created successfully.
        </Alert>

        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="sm">
            <Title order={4}>Your keys were issued</Title>
            <List spacing="sm" size="sm">
              <List.Item
                icon={
                  <ThemeIcon color="blue" size={20} radius="xl">
                    <IconShield size={12} />
                  </ThemeIcon>
                }
              >
                <Text size="sm">
                  <strong>Root key</strong> — your account recovery key. It was generated locally,
                  encrypted with your backup password, and stored on the server. You&apos;ll need
                  your backup password to log in on a new browser or device.
                </Text>
              </List.Item>
              <List.Item
                icon={
                  <ThemeIcon color="teal" size={20} radius="xl">
                    <IconKey size={12} />
                  </ThemeIcon>
                }
              >
                <Text size="sm">
                  <strong>Device key</strong> — a key specific to this browser, approved by your
                  root key. This device will stay approved until you revoke it.
                </Text>
              </List.Item>
            </List>
            <Anchor
              href="https://github.com/icook/tiny-congress/blob/master/docs/domain-model.md"
              target="_blank"
              rel="noopener noreferrer"
              size="xs"
            >
              Learn more about how TinyCongress keys work →
            </Anchor>
          </Stack>
        </Card>

        <Stack gap="sm">
          <Title order={3}>What&apos;s next?</Title>
          {verifierUrl ? (
            <Text size="sm">
              Verify your identity so we know you&apos;re a real person, not a bot. Without
              verification, you can browse rooms but cannot vote.
            </Text>
          ) : (
            <Text size="sm">You&apos;re all set! Start exploring rooms and voting on polls.</Text>
          )}
          <Group mt="xs">
            {verifierUrl ? (
              <Button component="a" href={verifierUrl}>
                Verify Identity
              </Button>
            ) : null}
            <Button component={Link} to="/rooms" variant={verifierUrl ? 'outline' : 'filled'}>
              Browse Rooms
            </Button>
          </Group>
        </Stack>
      </Stack>
    );
  }

  const passwordMismatch =
    passwordConfirm.length > 0 && password !== passwordConfirm
      ? 'Passwords do not match'
      : undefined;

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
              description="Used to encrypt your root key backup. You'll need this to log in on new browsers or devices."
              required
              value={password}
              onChange={(e) => {
                onPasswordChange(e.currentTarget.value);
              }}
              disabled={isLoading}
            />

            <PasswordInput
              label="Confirm Backup Password"
              required
              value={passwordConfirm}
              onChange={(e) => {
                onPasswordConfirmChange(e.currentTarget.value);
              }}
              error={passwordMismatch}
              disabled={isLoading}
            />

            {error ? (
              <Alert icon={<IconAlertTriangle size={16} />} title="Signup failed" color="red">
                {error}
              </Alert>
            ) : null}

            <Group justify="flex-end">
              <Button type="submit" loading={isLoading} disabled={!!passwordMismatch}>
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
