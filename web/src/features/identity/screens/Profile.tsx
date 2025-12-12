/**
 * Profile page - displays account info, tier badge, security posture,
 * endorsements by topic, and reputation score
 */

import { useCallback, useEffect, useState } from 'react';
import {
  IconAlertCircle,
  IconDevices,
  IconShield,
  IconShieldCheck,
  IconStar,
  IconUser,
} from '@tabler/icons-react';
import {
  Alert,
  Badge,
  Card,
  Container,
  Grid,
  Group,
  Paper,
  Progress,
  Skeleton,
  Stack,
  Text,
  Title,
  Tooltip,
} from '@mantine/core';
import {
  getEndorsements,
  getReputationScore,
  getSecurityPosture,
  listDevices,
  type Endorsement,
  type EndorsementAggregate,
  type ReputationScore,
  type SecurityPosture,
} from '../api/client';
import { getSession } from '../state/session';

type Tier = 'anonymous' | 'verified' | 'bonded' | 'vouched';

interface ProfileData {
  accountId: string;
  username: string;
  tier: Tier;
  createdAt: string;
}

interface EndorsementBin {
  topic: string;
  endorsements: Endorsement[];
  aggregate: EndorsementAggregate | null;
}

export function Profile() {
  const [profile, setProfile] = useState<ProfileData | null>(null);
  const [posture, setPosture] = useState<SecurityPosture | null>(null);
  const [endorsementBins, setEndorsementBins] = useState<EndorsementBin[]>([]);
  const [reputation, setReputation] = useState<ReputationScore | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchProfileData = useCallback(async () => {
    const session = getSession();
    if (!session?.sessionToken) {
      setError('Please login to view your profile');
      setLoading(false);
      return;
    }

    // Set profile from session (username is available from session)
    setProfile({
      accountId: session.accountId,
      username: session.username || 'Unknown User',
      tier: 'anonymous', // Default tier - would come from backend
      createdAt: new Date().toISOString(), // Placeholder
    });

    try {
      // Fetch data in parallel where possible
      const [postureResult, endorsementsResult, reputationResult, devicesResult] =
        await Promise.allSettled([
          getSecurityPosture(session.sessionToken, session.accountId).catch(() => null),
          getEndorsements(session.accountId).catch(() => [[], null] as [Endorsement[], null]),
          getReputationScore(session.accountId).catch(() => null),
          listDevices(session.sessionToken).catch(() => []),
        ]);

      // Process security posture (fallback to derived from devices)
      if (postureResult.status === 'fulfilled' && postureResult.value) {
        setPosture(postureResult.value);
      } else if (devicesResult.status === 'fulfilled') {
        const devices = devicesResult.value || [];
        const activeDevices = devices.filter((d) => !d.revoked_at);
        setPosture({
          device_count: devices.length,
          active_device_count: activeDevices.length,
          mfa_enabled: false,
          recovery_policy_configured: false,
          posture_label: activeDevices.length > 1 ? 'ok' : 'weak',
        });
      }

      // Process endorsements
      if (endorsementsResult.status === 'fulfilled') {
        const [endorsements] = endorsementsResult.value || [[], null];
        const bins = groupEndorsementsByTopic(endorsements);
        setEndorsementBins(bins);
      }

      // Process reputation
      if (reputationResult.status === 'fulfilled' && reputationResult.value) {
        setReputation(reputationResult.value);
      } else {
        // Default reputation
        setReputation({
          account_id: session.accountId,
          score: 0.5,
          updated_at: new Date().toISOString(),
        });
      }

      setError(null);
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to load profile data');
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchProfileData();
  }, [fetchProfileData]);

  if (loading) {
    return (
      <Container size="md" mt="xl">
        <Paper withBorder shadow="md" p="xl" radius="md">
          <Stack gap="md">
            <Skeleton height={60} />
            <Grid>
              <Grid.Col span={6}>
                <Skeleton height={150} />
              </Grid.Col>
              <Grid.Col span={6}>
                <Skeleton height={150} />
              </Grid.Col>
            </Grid>
            <Skeleton height={200} />
          </Stack>
        </Paper>
      </Container>
    );
  }

  if (error) {
    return (
      <Container size="md" mt="xl">
        <Alert icon={<IconAlertCircle size={16} />} color="red" title="Error">
          {error}
        </Alert>
      </Container>
    );
  }

  return (
    <Container size="md" mt="xl">
      <Stack gap="lg">
        {/* Profile Header */}
        <Paper withBorder shadow="md" p="xl" radius="md">
          <Group justify="space-between" align="flex-start">
            <Group>
              <IconUser size={48} stroke={1.5} />
              <Stack gap={4}>
                <Group gap="sm">
                  <Title order={2}>{profile?.username}</Title>
                  <TierBadge tier={profile?.tier || 'anonymous'} />
                </Group>
                <Text size="sm" c="dimmed">
                  Account ID: {profile?.accountId}
                </Text>
              </Stack>
            </Group>
            <ReputationPill score={reputation?.score ?? 0.5} />
          </Group>
        </Paper>

        <Grid>
          {/* Security Posture Card */}
          <Grid.Col span={{ base: 12, md: 6 }}>
            <SecurityPostureCard posture={posture} />
          </Grid.Col>

          {/* Reputation Details Card */}
          <Grid.Col span={{ base: 12, md: 6 }}>
            <ReputationCard reputation={reputation} />
          </Grid.Col>
        </Grid>

        {/* Endorsements Section */}
        <EndorsementsSection bins={endorsementBins} />
      </Stack>
    </Container>
  );
}

interface TierBadgeProps {
  tier: Tier;
}

function TierBadge({ tier }: TierBadgeProps) {
  const tierConfig: Record<Tier, { color: string; label: string }> = {
    anonymous: { color: 'gray', label: 'Anonymous' },
    verified: { color: 'blue', label: 'Verified' },
    bonded: { color: 'green', label: 'Bonded' },
    vouched: { color: 'violet', label: 'Vouched' },
  };

  const config = tierConfig[tier];

  return (
    <Badge color={config.color} variant="filled" size="lg">
      {config.label}
    </Badge>
  );
}

interface ReputationPillProps {
  score: number;
}

function ReputationPill({ score }: ReputationPillProps) {
  const percentage = Math.round(score * 100);
  const color = score >= 0.7 ? 'green' : score >= 0.4 ? 'yellow' : 'red';

  return (
    <Tooltip
      label={
        <Text size="xs">
          Reputation score based on endorsements.
          <br />
          Calculated from trustworthy and is_real_person topics.
        </Text>
      }
      multiline
      w={220}
    >
      <Badge
        color={color}
        variant="light"
        size="xl"
        leftSection={<IconStar size={14} />}
        style={{ cursor: 'help' }}
      >
        {percentage}%
      </Badge>
    </Tooltip>
  );
}

interface SecurityPostureCardProps {
  posture: SecurityPosture | null;
}

function SecurityPostureCard({ posture }: SecurityPostureCardProps) {
  const postureConfig: Record<
    'weak' | 'ok' | 'strong',
    { color: string; icon: typeof IconShield }
  > = {
    weak: { color: 'red', icon: IconShield },
    ok: { color: 'yellow', icon: IconShield },
    strong: { color: 'green', icon: IconShieldCheck },
  };

  const config = posture ? postureConfig[posture.posture_label] : postureConfig.weak;
  const PostureIcon = config.icon;

  return (
    <Card withBorder padding="lg" radius="md" h="100%">
      <Group mb="md">
        <PostureIcon size={24} color={`var(--mantine-color-${config.color}-6)`} />
        <Title order={4}>Security Posture</Title>
      </Group>

      {posture ? (
        <Stack gap="sm">
          <Group justify="space-between">
            <Text size="sm">Status</Text>
            <Badge color={config.color} variant="light">
              {posture.posture_label.toUpperCase()}
            </Badge>
          </Group>

          <Group justify="space-between">
            <Group gap="xs">
              <IconDevices size={16} />
              <Text size="sm">Devices</Text>
            </Group>
            <Text size="sm" fw={500}>
              {posture.active_device_count} active / {posture.device_count} total
            </Text>
          </Group>

          <Group justify="space-between">
            <Text size="sm">MFA Enabled</Text>
            <Badge color={posture.mfa_enabled ? 'green' : 'gray'} variant="light">
              {posture.mfa_enabled ? 'Yes' : 'No'}
            </Badge>
          </Group>

          <Group justify="space-between">
            <Text size="sm">Recovery Policy</Text>
            <Badge color={posture.recovery_policy_configured ? 'green' : 'gray'} variant="light">
              {posture.recovery_policy_configured ? 'Configured' : 'Not Set'}
            </Badge>
          </Group>
        </Stack>
      ) : (
        <Text c="dimmed" size="sm">
          Unable to load security posture
        </Text>
      )}
    </Card>
  );
}

interface ReputationCardProps {
  reputation: ReputationScore | null;
}

function ReputationCard({ reputation }: ReputationCardProps) {
  const score = reputation?.score ?? 0.5;
  const percentage = Math.round(score * 100);

  return (
    <Card withBorder padding="lg" radius="md" h="100%">
      <Group mb="md">
        <IconStar size={24} />
        <Title order={4}>Reputation</Title>
      </Group>

      <Stack gap="md">
        <Stack gap="xs">
          <Group justify="space-between">
            <Text size="sm">Overall Score</Text>
            <Text size="sm" fw={700}>
              {percentage}%
            </Text>
          </Group>
          <Progress
            value={percentage}
            color={score >= 0.7 ? 'green' : score >= 0.4 ? 'yellow' : 'red'}
            size="lg"
          />
        </Stack>

        <Text size="xs" c="dimmed">
          Score is calculated based on endorsements received in key topics like trustworthiness and
          identity verification. Higher scores indicate more positive community endorsements.
        </Text>

        {reputation?.updated_at && (
          <Text size="xs" c="dimmed">
            Last updated: {new Date(reputation.updated_at).toLocaleDateString()}
          </Text>
        )}
      </Stack>
    </Card>
  );
}

interface EndorsementsSectionProps {
  bins: EndorsementBin[];
}

function EndorsementsSection({ bins }: EndorsementsSectionProps) {
  if (bins.length === 0) {
    return (
      <Paper withBorder shadow="md" p="xl" radius="md">
        <Title order={4} mb="md">
          Endorsements Received
        </Title>
        <Text c="dimmed">No endorsements received yet.</Text>
      </Paper>
    );
  }

  return (
    <Paper withBorder shadow="md" p="xl" radius="md">
      <Title order={4} mb="lg">
        Endorsements Received
      </Title>

      <Grid>
        {bins.map((bin) => (
          <Grid.Col key={bin.topic} span={{ base: 12, sm: 6, md: 4 }}>
            <EndorsementTopicCard bin={bin} />
          </Grid.Col>
        ))}
      </Grid>
    </Paper>
  );
}

interface EndorsementTopicCardProps {
  bin: EndorsementBin;
}

function EndorsementTopicCard({ bin }: EndorsementTopicCardProps) {
  const aggregate = bin.aggregate;
  const weightedMean = aggregate?.weighted_mean ?? 0;
  const sentiment = weightedMean >= 0 ? 'positive' : 'negative';
  const color = weightedMean >= 0.3 ? 'green' : weightedMean >= -0.3 ? 'gray' : 'red';

  return (
    <Card withBorder padding="md" radius="md">
      <Stack gap="xs">
        <Group justify="space-between">
          <Text fw={500} tt="capitalize">
            {bin.topic.replace(/_/g, ' ')}
          </Text>
          <Badge color={color} variant="light" size="sm">
            {sentiment}
          </Badge>
        </Group>

        <Group gap="xs">
          <Text size="xs" c="dimmed">
            {aggregate?.n_total ?? bin.endorsements.length} endorsements
          </Text>
          {aggregate && (
            <>
              <Text size="xs" c="dimmed">
                •
              </Text>
              <Text size="xs" c="green">
                +{aggregate.n_pos}
              </Text>
              <Text size="xs" c="red">
                -{aggregate.n_neg}
              </Text>
            </>
          )}
        </Group>

        {aggregate?.weighted_mean != null && (
          <Progress value={((aggregate.weighted_mean + 1) / 2) * 100} color={color} size="sm" />
        )}
      </Stack>
    </Card>
  );
}

function groupEndorsementsByTopic(endorsements: Endorsement[]): EndorsementBin[] {
  const byTopic = new Map<string, Endorsement[]>();

  for (const endorsement of endorsements) {
    const existing = byTopic.get(endorsement.topic) || [];
    existing.push(endorsement);
    byTopic.set(endorsement.topic, existing);
  }

  return Array.from(byTopic.entries()).map(([topic, items]) => ({
    topic,
    endorsements: items,
    aggregate: computeLocalAggregate(topic, items),
  }));
}

function computeLocalAggregate(
  topic: string,
  endorsements: Endorsement[]
): EndorsementAggregate | null {
  if (endorsements.length === 0) {
    return null;
  }

  const nPos = endorsements.filter((e) => e.magnitude > 0).length;
  const nNeg = endorsements.filter((e) => e.magnitude < 0).length;
  const sumWeight = endorsements.reduce((acc, e) => acc + e.confidence, 0);
  const weightedSum = endorsements.reduce((acc, e) => acc + e.magnitude * e.confidence, 0);
  const weightedMean = sumWeight > 0 ? weightedSum / sumWeight : null;

  return {
    subject_type: 'account',
    subject_id: endorsements[0]?.subject_id || '',
    topic,
    n_total: endorsements.length,
    n_pos: nPos,
    n_neg: nNeg,
    sum_weight: sumWeight,
    weighted_mean: weightedMean,
  };
}
