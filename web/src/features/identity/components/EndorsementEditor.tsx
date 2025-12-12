/**
 * Endorsement editor component
 * Create and preview endorsements with magnitude/confidence sliders
 */

import { useCallback, useState } from 'react';
import { IconAlertCircle, IconCheck, IconClock } from '@tabler/icons-react';
import {
  Alert,
  Button,
  Card,
  Group,
  Paper,
  Progress,
  Select,
  Slider,
  Stack,
  Text,
  Textarea,
  TextInput,
  Title,
} from '@mantine/core';
import { ApiError, createEndorsement } from '../api/client';
import { deriveKid, getDevicePrivateKey, getDevicePublicKey, signEnvelope } from '../keys';
import { getSession } from '../state/session';

// Common endorsement topics
const TOPICS = [
  { value: 'trustworthy', label: 'Trustworthy' },
  { value: 'is_real_person', label: 'Is Real Person' },
  { value: 'expertise', label: 'Expertise' },
  { value: 'helpful', label: 'Helpful' },
  { value: 'reliable', label: 'Reliable' },
  { value: 'accurate', label: 'Accurate' },
];

export interface EndorsementEditorProps {
  subjectAccountId: string;
  subjectUsername?: string;
  onSuccess?: (endorsementId: string) => void;
  onCancel?: () => void;
}

interface RateLimitInfo {
  retryAfter: number;
  message: string;
}

export function EndorsementEditor({
  subjectAccountId,
  subjectUsername,
  onSuccess,
  onCancel,
}: EndorsementEditorProps) {
  const [topic, setTopic] = useState<string | null>(null);
  const [magnitude, setMagnitude] = useState(0);
  const [confidence, setConfidence] = useState(0.5);
  const [context, setContext] = useState('');
  const [evidenceUrl, setEvidenceUrl] = useState('');
  const [tags, setTags] = useState('');

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const [rateLimit, setRateLimit] = useState<RateLimitInfo | null>(null);

  // Compute weighted contribution preview
  const weightedContribution = magnitude * confidence;
  const contributionColor =
    weightedContribution > 0 ? 'green' : weightedContribution < 0 ? 'red' : 'gray';

  const handleSubmit = useCallback(async () => {
    if (!topic) {
      setError('Please select a topic');
      return;
    }

    const session = getSession();
    if (!session?.sessionToken) {
      setError('Please login to create endorsements');
      return;
    }

    setLoading(true);
    setError(null);
    setRateLimit(null);

    try {
      // Get device private key and public key for kid derivation
      const privateKey = await getDevicePrivateKey(session.accountId);
      const deviceKeyData = await getDevicePublicKey(session.accountId);
      const deviceKid = deriveKid(deviceKeyData);

      // Build endorsement payload
      const payload: Record<string, unknown> = {
        subject_type: 'account',
        subject_id: subjectAccountId,
        topic,
        magnitude,
        confidence,
      };

      if (context.trim()) {
        payload.context = context.trim();
      }

      if (evidenceUrl.trim()) {
        payload.evidence_url = evidenceUrl.trim();
      }

      if (tags.trim()) {
        payload.tags = tags
          .split(',')
          .map((t) => t.trim())
          .filter((t) => t);
      }

      // Sign the envelope
      const envelope = signEnvelope(
        'EndorsementCreated',
        payload,
        {
          account_id: session.accountId,
          device_id: session.deviceId,
          kid: deviceKid,
        },
        privateKey
      );

      // Submit to API
      const response = await createEndorsement(session.sessionToken, {
        account_id: session.accountId,
        device_id: session.deviceId,
        envelope,
      });

      setSuccess(true);
      onSuccess?.(response.endorsement_id);
    } catch (err) {
      if (err instanceof ApiError && err.status === 429) {
        // Rate limited
        const retryAfter = 60; // Default to 60 seconds
        setRateLimit({
          retryAfter,
          message: err.message || 'Rate limit exceeded',
        });
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to create endorsement');
      }
    } finally {
      setLoading(false);
    }
  }, [topic, magnitude, confidence, context, evidenceUrl, tags, subjectAccountId, onSuccess]);

  if (success) {
    return (
      <Card withBorder padding="lg" radius="md">
        <Stack align="center" gap="md">
          <IconCheck size={48} color="var(--mantine-color-green-6)" />
          <Title order={4}>Endorsement Created</Title>
          <Text c="dimmed">Your endorsement has been recorded.</Text>
          {onCancel && (
            <Button variant="outline" onClick={onCancel}>
              Close
            </Button>
          )}
        </Stack>
      </Card>
    );
  }

  return (
    <Card withBorder padding="lg" radius="md">
      <Stack gap="md">
        <Title order={4}>Create Endorsement</Title>

        <Text size="sm" c="dimmed">
          Endorsing: {subjectUsername || subjectAccountId}
        </Text>

        {error && (
          <Alert icon={<IconAlertCircle size={16} />} color="red">
            {error}
          </Alert>
        )}

        {rateLimit && (
          <Alert icon={<IconClock size={16} />} color="yellow" title="Rate Limited">
            {rateLimit.message}. Please wait before creating another endorsement.
          </Alert>
        )}

        <Select
          label="Topic"
          placeholder="Select endorsement topic"
          data={TOPICS}
          value={topic}
          onChange={setTopic}
          required
        />

        <Stack gap="xs">
          <Text size="sm" fw={500}>
            Magnitude: {magnitude.toFixed(2)}
          </Text>
          <Text size="xs" c="dimmed">
            -1 (strongly negative) to +1 (strongly positive)
          </Text>
          <Slider
            value={magnitude}
            onChange={setMagnitude}
            min={-1}
            max={1}
            step={0.1}
            marks={[
              { value: -1, label: '-1' },
              { value: 0, label: '0' },
              { value: 1, label: '+1' },
            ]}
            color={magnitude >= 0 ? 'green' : 'red'}
          />
        </Stack>

        <Stack gap="xs">
          <Text size="sm" fw={500}>
            Confidence: {confidence.toFixed(2)}
          </Text>
          <Text size="xs" c="dimmed">
            How confident are you in this assessment? (0 = unsure, 1 = certain)
          </Text>
          <Slider
            value={confidence}
            onChange={setConfidence}
            min={0}
            max={1}
            step={0.05}
            marks={[
              { value: 0, label: '0' },
              { value: 0.5, label: '0.5' },
              { value: 1, label: '1' },
            ]}
          />
        </Stack>

        <Textarea
          label="Context (optional)"
          placeholder="Provide context for this endorsement..."
          value={context}
          onChange={(e) => setContext(e.target.value)}
          minRows={2}
        />

        <TextInput
          label="Evidence URL (optional)"
          placeholder="https://..."
          value={evidenceUrl}
          onChange={(e) => setEvidenceUrl(e.target.value)}
        />

        <TextInput
          label="Tags (optional)"
          placeholder="Comma-separated tags..."
          value={tags}
          onChange={(e) => setTags(e.target.value)}
        />

        {/* Live Preview */}
        <Paper withBorder p="md" radius="sm" bg="gray.0">
          <Stack gap="xs">
            <Text size="sm" fw={500}>
              Weighted Contribution Preview
            </Text>
            <Group>
              <Text size="sm" c="dimmed">
                {magnitude.toFixed(2)} × {confidence.toFixed(2)} =
              </Text>
              <Text size="sm" fw={700} c={contributionColor}>
                {weightedContribution.toFixed(3)}
              </Text>
            </Group>
            <Progress
              value={((weightedContribution + 1) / 2) * 100}
              color={contributionColor}
              size="sm"
            />
            <Text size="xs" c="dimmed">
              This will be combined with other endorsements to compute the aggregate score.
            </Text>
          </Stack>
        </Paper>

        <Group justify="flex-end" mt="md">
          {onCancel && (
            <Button variant="outline" onClick={onCancel} disabled={loading}>
              Cancel
            </Button>
          )}
          <Button onClick={handleSubmit} loading={loading} disabled={!topic}>
            Create Endorsement
          </Button>
        </Group>
      </Stack>
    </Card>
  );
}

// Endorsement list item with revocation
export interface EndorsementItemProps {
  endorsement: {
    id: string;
    topic: string;
    magnitude: number;
    confidence: number;
    context?: string;
    created_at: string;
  };
  canRevoke?: boolean;
  onRevoke?: (endorsementId: string) => void;
}

export function EndorsementItem({ endorsement, canRevoke, onRevoke }: EndorsementItemProps) {
  const [revoking, setRevoking] = useState(false);

  const handleRevoke = async () => {
    if (!onRevoke) {
      return;
    }
    setRevoking(true);
    try {
      await onRevoke(endorsement.id);
    } finally {
      setRevoking(false);
    }
  };

  const weighted = endorsement.magnitude * endorsement.confidence;
  const color = weighted > 0 ? 'green' : weighted < 0 ? 'red' : 'gray';

  return (
    <Card withBorder padding="sm" radius="sm">
      <Group justify="space-between">
        <Stack gap={4}>
          <Group gap="xs">
            <Text fw={500} tt="capitalize">
              {endorsement.topic.replace(/_/g, ' ')}
            </Text>
            <Text size="xs" c={color} fw={700}>
              {weighted >= 0 ? '+' : ''}
              {weighted.toFixed(2)}
            </Text>
          </Group>
          {endorsement.context && (
            <Text size="xs" c="dimmed" lineClamp={2}>
              {endorsement.context}
            </Text>
          )}
          <Text size="xs" c="dimmed">
            {new Date(endorsement.created_at).toLocaleDateString()}
          </Text>
        </Stack>

        {canRevoke && (
          <Button size="xs" color="red" variant="subtle" onClick={handleRevoke} loading={revoking}>
            Revoke
          </Button>
        )}
      </Group>
    </Card>
  );
}
