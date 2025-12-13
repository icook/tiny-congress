/**
 * Recovery screen
 * Set up account recovery policy and perform root rotation
 */

import { useState } from 'react';
import { IconAlertTriangle, IconKey, IconPlus, IconShield } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import {
  Alert,
  Badge,
  Button,
  Card,
  Group,
  NumberInput,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { recoveryPolicyQuery, useSetRecoveryPolicy } from '../api/queries';
import { canonicalizeToBytes, encodeBase64Url, getRootKey, sign, storedToKeyPair } from '../keys';
import { useSession } from '../state/session';

export function Recovery() {
  const { session } = useSession();
  const setPolicy = useSetRecoveryPolicy();

  const [threshold, setThreshold] = useState(2);
  const [helpers, setHelpers] = useState<string[]>(['']);
  const [error, setError] = useState<string | null>(null);

  const policyQuery = useQuery({
    ...recoveryPolicyQuery(session?.accountId || ''),
    enabled: !!session?.accountId,
  });

  if (!session) {
    return (
      <Alert icon={<IconAlertTriangle size={16} />} title="Not authenticated" color="red">
        Please log in to manage recovery settings
      </Alert>
    );
  }

  const handleAddHelper = () => {
    setHelpers([...helpers, '']);
  };

  const handleHelperChange = (index: number, value: string) => {
    const newHelpers = [...helpers];
    newHelpers[index] = value;
    setHelpers(newHelpers);
  };

  const handleRemoveHelper = (index: number) => {
    setHelpers(helpers.filter((_, i) => i !== index));
  };

  const handleSetPolicy = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    const validHelpers = helpers.filter((h) => h.trim());
    if (validHelpers.length === 0) {
      setError('At least one helper is required');
      return;
    }

    if (threshold > validHelpers.length) {
      setError('Threshold cannot exceed number of helpers');
      return;
    }

    try {
      // Load root key
      const storedRootKey = await getRootKey();
      if (!storedRootKey) {
        setError('Root key not found');
        return;
      }

      const rootKeyPair = storedToKeyPair(storedRootKey);

      // Create recovery policy payload
      const policyPayload = {
        type: 'RecoveryPolicy',
        threshold,
        helpers: validHelpers.map((helper_account_id) => ({ helper_account_id })),
        created_at: new Date().toISOString(),
      };

      const canonicalPayload = canonicalizeToBytes(policyPayload);
      const policySignature = sign(canonicalPayload, rootKeyPair.privateKey);

      const envelope = {
        payload: policyPayload,
        signer: {
          kid: rootKeyPair.kid,
          account_id: session.accountId,
        },
        signature: encodeBase64Url(policySignature),
      };

      // Set recovery policy
      await setPolicy.mutateAsync({
        account_id: session.accountId,
        envelope,
      });

      setHelpers(['']);
      setThreshold(2);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set recovery policy');
    }
  };

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <Group gap="xs">
        <IconShield size={24} />
        <Title order={2}>Account Recovery</Title>
      </Group>

      <Text c="dimmed" size="sm">
        Set up account recovery with trusted helpers
      </Text>

      {/* Current Policy */}
      {policyQuery.data && (
        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="md">
            <Group justify="space-between">
              <Text fw={500}>Current Recovery Policy</Text>
              <Badge color="green">Active</Badge>
            </Group>

            <div>
              <Text size="sm" c="dimmed">
                Threshold
              </Text>
              <Text fw={500}>{policyQuery.data.threshold} approvals required</Text>
            </div>

            <div>
              <Text size="sm" c="dimmed" mb="xs">
                Helpers ({policyQuery.data.helpers.length})
              </Text>
              {policyQuery.data.helpers.map((helper, idx) => (
                <Text key={idx} size="sm" ff="monospace">
                  {helper.helper_account_id}
                </Text>
              ))}
            </div>

            <Text size="xs" c="dimmed">
              Created: {new Date(policyQuery.data.created_at).toLocaleString()}
            </Text>
          </Stack>
        </Card>
      )}

      {/* Set Policy Form */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <form onSubmit={handleSetPolicy}>
          <Stack gap="md">
            <Group gap="xs">
              <IconKey size={20} />
              <Text fw={500}>Set New Recovery Policy</Text>
            </Group>

            <NumberInput
              label="Threshold"
              description="Number of helper approvals required for recovery"
              value={threshold}
              onChange={(val) => setThreshold(Number(val))}
              min={1}
              max={helpers.length}
              disabled={setPolicy.isPending}
            />

            <div>
              <Group justify="space-between" mb="xs">
                <Text size="sm" fw={500}>
                  Helper Accounts
                </Text>
                <Button
                  size="xs"
                  variant="light"
                  leftSection={<IconPlus size={14} />}
                  onClick={handleAddHelper}
                  disabled={setPolicy.isPending}
                >
                  Add Helper
                </Button>
              </Group>

              <Stack gap="xs">
                {helpers.map((helper, index) => (
                  <Group key={index} gap="xs">
                    <TextInput
                      placeholder="Helper account ID (UUID)"
                      value={helper}
                      onChange={(e) => handleHelperChange(index, e.currentTarget.value)}
                      disabled={setPolicy.isPending}
                      style={{ flex: 1 }}
                    />
                    {helpers.length > 1 && (
                      <Button
                        size="xs"
                        color="red"
                        variant="subtle"
                        onClick={() => handleRemoveHelper(index)}
                        disabled={setPolicy.isPending}
                      >
                        Remove
                      </Button>
                    )}
                  </Group>
                ))}
              </Stack>
            </div>

            {error && (
              <Alert icon={<IconAlertTriangle size={16} />} title="Error" color="red">
                {error}
              </Alert>
            )}

            {setPolicy.isError && (
              <Alert
                icon={<IconAlertTriangle size={16} />}
                title="Failed to set policy"
                color="red"
              >
                {setPolicy.error?.message || 'An error occurred'}
              </Alert>
            )}

            {setPolicy.isSuccess && (
              <Alert color="green" title="Recovery policy set successfully">
                Your recovery policy has been updated
              </Alert>
            )}

            <Button type="submit" loading={setPolicy.isPending}>
              Set Recovery Policy
            </Button>
          </Stack>
        </form>
      </Card>

      <Alert color="yellow">
        <Text size="sm" fw={500} mb="xs">
          Important
        </Text>
        <Text size="sm">
          Recovery helpers can approve a root key rotation if you lose access to your account.
          Choose trusted individuals and keep this list up to date.
        </Text>
      </Alert>
    </Stack>
  );
}
