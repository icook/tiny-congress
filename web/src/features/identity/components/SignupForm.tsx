/**
 * SignupForm - Presentational component for signup
 * Renders the signup form UI based on props, no hooks or side effects
 */

import { IconAlertTriangle, IconCheck } from '@tabler/icons-react';
import { Alert, Button, Card, Code, Group, Stack, Text, TextInput, Title } from '@mantine/core';

export type SignupFormProps = {
  // Form state
  username: string;
  onUsernameChange: (value: string) => void;
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
  } | null;
};

export function SignupForm({
  username,
  onUsernameChange,
  onSubmit,
  isLoading,
  loadingText,
  error,
  successData,
}: SignupFormProps) {
  if (successData) {
    return (
      <Stack gap="md" maw={500} mx="auto" mt="xl">
        <Alert icon={<IconCheck size={16} />} title="Account Created" color="green">
          Your account has been created successfully.
        </Alert>

        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="sm">
            <Text fw={500}>Account Details</Text>
            <Text size="sm">
              <strong>Account ID:</strong> <Code>{successData.account_id}</Code>
            </Text>
            <Text size="sm">
              <strong>Root Key ID:</strong> <Code>{successData.root_kid}</Code>
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
        <form onSubmit={onSubmit}>
          <Stack gap="md">
            <TextInput
              label="Username"
              placeholder="alice"
              required
              value={username}
              onChange={(e) => onUsernameChange(e.currentTarget.value)}
              disabled={isLoading}
            />

            {error && (
              <Alert icon={<IconAlertTriangle size={16} />} title="Signup failed" color="red">
                {error}
              </Alert>
            )}

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
    </Stack>
  );
}
