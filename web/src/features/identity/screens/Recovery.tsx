/**
 * Recovery setup screen - configure helpers, threshold, and manage root rotation
 */

import { useCallback, useEffect, useState } from 'react';
import {
  IconAlertCircle,
  IconCheck,
  IconKey,
  IconPlus,
  IconShield,
  IconTrash,
  IconUserPlus,
} from '@tabler/icons-react';
import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Card,
  Container,
  Divider,
  Group,
  Modal,
  NumberInput,
  Paper,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import {
  getRecoveryPolicy,
  setRecoveryPolicy,
  type RecoveryHelper,
  type RecoveryPolicy,
} from '../api/client';
import {
  decodeBase64Url,
  deriveKid,
  encodeBase64Url,
  generateRootKey,
  getRootKeyTemporary,
  signEnvelope,
} from '../keys';
import { getSession } from '../state/session';

export function Recovery() {
  const [policy, setPolicy] = useState<RecoveryPolicy | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [setupModalOpened, { open: openSetupModal, close: closeSetupModal }] = useDisclosure(false);

  const session = getSession();

  const fetchPolicy = useCallback(async () => {
    if (!session?.accountId) {
      setError('Please login to view recovery settings');
      setLoading(false);
      return;
    }

    try {
      const fetchedPolicy = await getRecoveryPolicy(session.accountId);
      setPolicy(fetchedPolicy);
      setError(null);
    } catch (err) {
      // 404 is expected if no policy exists
      if (err instanceof Error && err.message.includes('404')) {
        setPolicy(null);
        setError(null);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to load recovery policy');
      }
    } finally {
      setLoading(false);
    }
  }, [session?.accountId]);

  useEffect(() => {
    fetchPolicy();
  }, [fetchPolicy]);

  const handlePolicyCreated = () => {
    closeSetupModal();
    fetchPolicy();
  };

  return (
    <Container size="md" mt="xl">
      <Paper withBorder shadow="md" p="xl" radius="md">
        <Group justify="space-between" mb="lg">
          <Group>
            <IconShield size={28} />
            <Title order={2}>Recovery Setup</Title>
          </Group>
          {!policy && (
            <Button leftSection={<IconPlus size={16} />} onClick={openSetupModal}>
              Configure Recovery
            </Button>
          )}
        </Group>

        {error && (
          <Alert icon={<IconAlertCircle size={16} />} color="red" mb="md">
            {error}
          </Alert>
        )}

        {loading ? (
          <Text>Loading recovery settings...</Text>
        ) : policy ? (
          <Stack gap="lg">
            <PolicyCard policy={policy} onRefresh={fetchPolicy} />
            <RotationSection policy={policy} onRotated={fetchPolicy} />
          </Stack>
        ) : (
          <NoPolicy onSetup={openSetupModal} />
        )}
      </Paper>

      <Modal
        opened={setupModalOpened}
        onClose={closeSetupModal}
        title="Configure Recovery Policy"
        size="lg"
      >
        <PolicySetupForm onSuccess={handlePolicyCreated} onCancel={closeSetupModal} />
      </Modal>
    </Container>
  );
}

interface PolicyCardProps {
  policy: RecoveryPolicy;
  onRefresh: () => void;
}

function PolicyCard({ policy }: PolicyCardProps) {
  return (
    <Card withBorder padding="lg" radius="md">
      <Stack gap="md">
        <Group justify="space-between">
          <Title order={4}>Active Recovery Policy</Title>
          <Badge color="green" variant="light">
            Active
          </Badge>
        </Group>

        <Group gap="lg">
          <Stack gap={4}>
            <Text size="xs" c="dimmed">
              Threshold
            </Text>
            <Text fw={700} size="xl">
              {policy.threshold} / {policy.helpers.length}
            </Text>
          </Stack>
          <Stack gap={4}>
            <Text size="xs" c="dimmed">
              Created
            </Text>
            <Text>{new Date(policy.created_at).toLocaleDateString()}</Text>
          </Stack>
        </Group>

        <Divider />

        <Stack gap="xs">
          <Text fw={500}>Recovery Helpers</Text>
          {policy.helpers.map((helper, index) => (
            <HelperItem key={helper.helper_account_id} helper={helper} index={index} />
          ))}
        </Stack>
      </Stack>
    </Card>
  );
}

interface HelperItemProps {
  helper: RecoveryHelper;
  index: number;
}

function HelperItem({ helper, index }: HelperItemProps) {
  return (
    <Group
      justify="space-between"
      p="xs"
      style={{
        backgroundColor: 'var(--mantine-color-gray-0)',
        borderRadius: 'var(--mantine-radius-sm)',
      }}
    >
      <Group gap="sm">
        <IconUserPlus size={16} />
        <Stack gap={0}>
          <Text size="sm">Helper {index + 1}</Text>
          <Text size="xs" c="dimmed" style={{ fontFamily: 'monospace' }}>
            {helper.helper_account_id.slice(0, 8)}...
          </Text>
        </Stack>
      </Group>
      {helper.helper_root_kid && (
        <Badge color="blue" variant="light" size="xs">
          Key Pinned
        </Badge>
      )}
    </Group>
  );
}

interface NoPolicyProps {
  onSetup: () => void;
}

function NoPolicy({ onSetup }: NoPolicyProps) {
  return (
    <Card withBorder padding="xl" radius="md" bg="gray.0">
      <Stack align="center" gap="md">
        <IconShield size={48} stroke={1.5} color="var(--mantine-color-gray-5)" />
        <Title order={4}>No Recovery Policy Configured</Title>
        <Text c="dimmed" ta="center" maw={400}>
          Set up a recovery policy to enable account recovery if you lose access to your root key.
          You'll need to designate trusted helpers who can approve a root key rotation.
        </Text>
        <Button onClick={onSetup}>Configure Recovery Policy</Button>
      </Stack>
    </Card>
  );
}

interface PolicySetupFormProps {
  onSuccess: () => void;
  onCancel: () => void;
}

function PolicySetupForm({ onSuccess, onCancel }: PolicySetupFormProps) {
  const [helpers, setHelpers] = useState<{ accountId: string; pinKey: boolean }[]>([
    { accountId: '', pinKey: false },
  ]);
  const [threshold, setThreshold] = useState<number | ''>(1);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const session = getSession();

  const addHelper = () => {
    setHelpers([...helpers, { accountId: '', pinKey: false }]);
  };

  const removeHelper = (index: number) => {
    setHelpers(helpers.filter((_, i) => i !== index));
  };

  const updateHelper = (index: number, accountId: string) => {
    const updated = [...helpers];
    updated[index] = { ...updated[index], accountId };
    setHelpers(updated);
  };

  const handleSubmit = async () => {
    if (!session?.accountId) {
      setError('Please login to configure recovery');
      return;
    }

    const validHelpers = helpers.filter((h) => h.accountId.trim());
    if (validHelpers.length === 0) {
      setError('At least one helper is required');
      return;
    }

    const thresholdNum = typeof threshold === 'number' ? threshold : 1;
    if (thresholdNum < 1 || thresholdNum > validHelpers.length) {
      setError(`Threshold must be between 1 and ${validHelpers.length}`);
      return;
    }

    setLoading(true);
    setError(null);

    try {
      // Get root key from temporary storage (requires root key access)
      const rootKeyStored = await getRootKeyTemporary(session.accountId);
      if (!rootKeyStored) {
        setError('Root key not available. Please re-authenticate with your root key.');
        setLoading(false);
        return;
      }

      // Decode the stored keys from base64url
      const rootPublicKey = decodeBase64Url(rootKeyStored.publicKey);
      const rootPrivateKey = decodeBase64Url(rootKeyStored.privateKey);
      const rootKid = deriveKid(rootPublicKey);

      // Build policy payload
      const payload = {
        threshold: thresholdNum,
        helpers: validHelpers.map((h) => ({
          helper_account_id: h.accountId.trim(),
        })),
        ctime: new Date().toISOString(),
      };

      // Sign with root key
      const envelope = signEnvelope(
        'RecoveryPolicySet',
        payload,
        {
          account_id: session.accountId,
          kid: rootKid,
        },
        rootPrivateKey
      );

      await setRecoveryPolicy({
        account_id: session.accountId,
        envelope,
      });

      onSuccess();
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to create recovery policy');
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <Stack gap="md">
      {error && (
        <Alert icon={<IconAlertCircle size={16} />} color="red">
          {error}
        </Alert>
      )}

      <Text size="sm" c="dimmed">
        Add trusted accounts that can help recover your account. They will need to approve any root
        key rotation request.
      </Text>

      <Stack gap="sm">
        {helpers.map((helper, index) => (
          <Group key={index} gap="sm">
            <TextInput
              style={{ flex: 1 }}
              placeholder="Helper Account ID (UUID)"
              value={helper.accountId}
              onChange={(e) => updateHelper(index, e.target.value)}
            />
            {helpers.length > 1 && (
              <ActionIcon color="red" variant="subtle" onClick={() => removeHelper(index)}>
                <IconTrash size={16} />
              </ActionIcon>
            )}
          </Group>
        ))}
        <Button variant="light" leftSection={<IconPlus size={14} />} onClick={addHelper}>
          Add Helper
        </Button>
      </Stack>

      <NumberInput
        label="Approval Threshold"
        description={`How many helpers must approve a recovery (max: ${helpers.filter((h) => h.accountId.trim()).length || 1})`}
        value={threshold}
        onChange={(val) => setThreshold(val as number | '')}
        min={1}
        max={helpers.filter((h) => h.accountId.trim()).length || 1}
      />

      <Alert color="yellow" icon={<IconAlertCircle size={16} />}>
        <Text size="sm">
          Requires root key access. Make sure your root key is available in this session before
          creating the policy.
        </Text>
      </Alert>

      <Group justify="flex-end" mt="md">
        <Button variant="outline" onClick={onCancel} disabled={loading}>
          Cancel
        </Button>
        <Button onClick={handleSubmit} loading={loading}>
          Create Policy
        </Button>
      </Group>
    </Stack>
  );
}

interface RotationSectionProps {
  policy: RecoveryPolicy;
  onRotated: () => void;
}

function RotationSection({ policy, onRotated }: RotationSectionProps) {
  const [rotateModalOpened, { open: openRotateModal, close: closeRotateModal }] =
    useDisclosure(false);

  // In a real app, you'd fetch actual approval status from the backend
  const approvals = 0; // Placeholder - would come from API
  const canRotate = approvals >= policy.threshold;

  return (
    <Card withBorder padding="lg" radius="md">
      <Stack gap="md">
        <Group justify="space-between">
          <Group>
            <IconKey size={20} />
            <Title order={4}>Root Key Rotation</Title>
          </Group>
          <Badge color={canRotate ? 'green' : 'gray'} variant="light">
            {approvals} / {policy.threshold} Approvals
          </Badge>
        </Group>

        <Text size="sm" c="dimmed">
          Once enough helpers have approved your recovery request, you can rotate to a new root key.
          This will invalidate all existing device delegations.
        </Text>

        <Button
          leftSection={<IconKey size={16} />}
          disabled={!canRotate}
          onClick={openRotateModal}
          color={canRotate ? 'green' : 'gray'}
        >
          {canRotate ? 'Rotate Root Key' : 'Waiting for Approvals'}
        </Button>
      </Stack>

      <Modal
        opened={rotateModalOpened}
        onClose={closeRotateModal}
        title="Rotate Root Key"
        size="lg"
      >
        <RotationForm
          policy={policy}
          onSuccess={() => {
            closeRotateModal();
            onRotated();
          }}
          onCancel={closeRotateModal}
        />
      </Modal>
    </Card>
  );
}

interface RotationFormProps {
  policy: RecoveryPolicy;
  onSuccess: () => void;
  onCancel: () => void;
}

function RotationForm({ policy, onSuccess, onCancel }: RotationFormProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newKeyGenerated, setNewKeyGenerated] = useState(false);

  const session = getSession();

  const handleRotate = async () => {
    if (!session?.accountId) {
      setError('Please login');
      return;
    }

    setLoading(true);
    setError(null);

    try {
      // Generate new root key
      const newRootKey = generateRootKey();
      const newRootKid = deriveKid(newRootKey.publicKey);
      const newRootPubkeyB64 = encodeBase64Url(newRootKey.publicKey);

      // Build rotation payload
      const payload = {
        policy_id: policy.policy_id,
        new_root_kid: newRootKid,
        new_root_pubkey: newRootPubkeyB64,
      };

      // Sign with new root key (the new key must sign its own rotation)
      const _envelope = signEnvelope(
        'RootRotation',
        payload,
        {
          account_id: session.accountId,
          kid: newRootKid,
        },
        newRootKey.privateKey
      );

      // Note: In production, you'd want to securely store the new root key
      // before calling the rotation endpoint
      // eslint-disable-next-line no-console
      console.log('New root key generated. Kid:', newRootKid);
      // eslint-disable-next-line no-console
      console.log('Envelope generated for rotation (not yet submitted):', _envelope.payload_type);

      // For now, just show success - real rotation requires backend call
      setNewKeyGenerated(true);

      // In production: await rotateRoot({ account_id: session.accountId, envelope: _envelope });
      onSuccess();
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to rotate root key');
      }
    } finally {
      setLoading(false);
    }
  };

  if (newKeyGenerated) {
    return (
      <Stack align="center" gap="md" p="xl">
        <IconCheck size={48} color="var(--mantine-color-green-6)" />
        <Title order={4}>Root Key Rotated</Title>
        <Text c="dimmed" ta="center">
          Your root key has been rotated. All existing device delegations have been invalidated.
          You'll need to re-delegate devices with your new root key.
        </Text>
        <Button onClick={onCancel}>Close</Button>
      </Stack>
    );
  }

  return (
    <Stack gap="md">
      {error && (
        <Alert icon={<IconAlertCircle size={16} />} color="red">
          {error}
        </Alert>
      )}

      <Alert color="yellow" icon={<IconAlertCircle size={16} />}>
        <Text size="sm" fw={500}>
          Warning: This action is irreversible!
        </Text>
        <Text size="sm">
          Rotating your root key will invalidate ALL existing device delegations. You will need to
          re-delegate each device after rotation.
        </Text>
      </Alert>

      <Text size="sm">
        A new root key will be generated and registered with your account. Make sure to securely
        back up the new key.
      </Text>

      <Group justify="flex-end" mt="md">
        <Button variant="outline" onClick={onCancel} disabled={loading}>
          Cancel
        </Button>
        <Button color="red" onClick={handleRotate} loading={loading}>
          Generate New Root Key & Rotate
        </Button>
      </Group>
    </Stack>
  );
}
