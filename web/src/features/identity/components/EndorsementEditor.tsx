/**
 * Endorsement editor component
 * Create endorsements with magnitude and confidence sliders
 */

import { useState } from 'react';
import { IconAlertTriangle, IconSend } from '@tabler/icons-react';
import {
  Alert,
  Button,
  Card,
  Group,
  Select,
  Slider,
  Stack,
  Text,
  Textarea,
  TextInput,
} from '@mantine/core';
import { useCreateEndorsement } from '../api/queries';
import { canonicalizeToBytes, encodeBase64Url, getDeviceKey, sign, storedToKeyPair } from '../keys';
import { useSession } from '../state/session';

export function EndorsementEditor() {
  const { session } = useSession();
  const createEndorsement = useCreateEndorsement();

  const [subjectType, setSubjectType] = useState<string>('account');
  const [subjectId, setSubjectId] = useState('');
  const [topic, setTopic] = useState('');
  const [magnitude, setMagnitude] = useState(0);
  const [confidence, setConfidence] = useState(0.5);
  const [context, setContext] = useState('');
  const [error, setError] = useState<string | null>(null);

  if (!session) {
    return (
      <Alert icon={<IconAlertTriangle size={16} />} title="Not authenticated" color="red">
        Please log in to create endorsements
      </Alert>
    );
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!subjectId || !topic) {
      setError('Subject ID and topic are required');
      return;
    }

    try {
      // Load device key
      const storedKey = await getDeviceKey();
      if (!storedKey) {
        setError('Device key not found');
        return;
      }

      const deviceKeyPair = storedToKeyPair(storedKey);

      // Create endorsement payload
      const endorsementPayload = {
        type: 'Endorsement',
        subject_type: subjectType,
        subject_id: subjectId,
        topic,
        magnitude,
        confidence,
        context: context || undefined,
        created_at: new Date().toISOString(),
      };

      const canonicalPayload = canonicalizeToBytes(endorsementPayload);
      const endorsementSignature = sign(canonicalPayload, deviceKeyPair.privateKey);

      const envelope = {
        payload: endorsementPayload,
        signer: {
          kid: deviceKeyPair.kid,
          account_id: session.accountId,
          device_id: session.deviceId,
        },
        signature: encodeBase64Url(endorsementSignature),
      };

      // Create endorsement
      await createEndorsement.mutateAsync({
        account_id: session.accountId,
        device_id: session.deviceId,
        envelope,
      });

      // Reset form
      setSubjectId('');
      setTopic('');
      setMagnitude(0);
      setConfidence(0.5);
      setContext('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create endorsement');
    }
  };

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <form onSubmit={handleSubmit}>
        <Stack gap="md">
          <Text fw={500} size="lg">
            Create Endorsement
          </Text>

          <Select
            label="Subject Type"
            value={subjectType}
            onChange={(value) => setSubjectType(value || 'account')}
            data={[
              { value: 'account', label: 'Account' },
              { value: 'proposal', label: 'Proposal' },
              { value: 'vote', label: 'Vote' },
            ]}
            disabled={createEndorsement.isPending}
          />

          <TextInput
            label="Subject ID"
            placeholder="UUID or identifier"
            required
            value={subjectId}
            onChange={(e) => setSubjectId(e.currentTarget.value)}
            disabled={createEndorsement.isPending}
          />

          <TextInput
            label="Topic"
            placeholder="e.g., climate-policy, software-engineering"
            required
            value={topic}
            onChange={(e) => setTopic(e.currentTarget.value)}
            disabled={createEndorsement.isPending}
            description="What domain or topic are you endorsing them on?"
          />

          <div>
            <Text size="sm" fw={500} mb="xs">
              Magnitude: {magnitude.toFixed(2)}
            </Text>
            <Slider
              value={magnitude}
              onChange={setMagnitude}
              min={-1}
              max={1}
              step={0.1}
              marks={[
                { value: -1, label: '-1 (Oppose)' },
                { value: 0, label: '0 (Neutral)' },
                { value: 1, label: '+1 (Support)' },
              ]}
              disabled={createEndorsement.isPending}
              color={magnitude < 0 ? 'red' : magnitude > 0 ? 'green' : 'gray'}
            />
          </div>

          <div>
            <Text size="sm" fw={500} mb="xs">
              Confidence: {(confidence * 100).toFixed(0)}%
            </Text>
            <Slider
              value={confidence}
              onChange={setConfidence}
              min={0}
              max={1}
              step={0.05}
              marks={[
                { value: 0, label: '0%' },
                { value: 0.5, label: '50%' },
                { value: 1, label: '100%' },
              ]}
              disabled={createEndorsement.isPending}
            />
            <Text size="xs" c="dimmed" mt="xs">
              How confident are you in this endorsement?
            </Text>
          </div>

          <Textarea
            label="Context (Optional)"
            placeholder="Reasoning or evidence for this endorsement"
            value={context}
            onChange={(e) => setContext(e.currentTarget.value)}
            disabled={createEndorsement.isPending}
            rows={3}
          />

          {error && (
            <Alert icon={<IconAlertTriangle size={16} />} title="Error" color="red">
              {error}
            </Alert>
          )}

          {createEndorsement.isError && (
            <Alert
              icon={<IconAlertTriangle size={16} />}
              title="Failed to create endorsement"
              color="red"
            >
              {createEndorsement.error?.message || 'An error occurred'}
            </Alert>
          )}

          {createEndorsement.isSuccess && (
            <Alert color="green" title="Endorsement created successfully">
              Your endorsement has been recorded
            </Alert>
          )}

          <Group justify="flex-end">
            <Button
              type="submit"
              leftSection={<IconSend size={16} />}
              loading={createEndorsement.isPending}
            >
              Create Endorsement
            </Button>
          </Group>
        </Stack>
      </form>
    </Card>
  );
}
