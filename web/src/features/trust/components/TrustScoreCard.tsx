import { IconShield, IconUsers } from '@tabler/icons-react';
import { Badge, Card, Group, Loader, Progress, Stack, Text, Title } from '@mantine/core';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { useTrustBudget, useTrustScores } from '../api';

interface TrustScoreCardProps {
  deviceKid: string | null;
  privateKey: CryptoKey | null;
  wasmCrypto: CryptoModule | null;
}

function getTierInfo(distance: number, diversity: number): { label: string; color: string } | null {
  if (distance <= 3.0 && diversity >= 2) {
    return { label: 'Congress', color: 'blue' };
  }
  if (distance <= 6.0 && diversity >= 1) {
    return { label: 'Community', color: 'teal' };
  }
  return null;
}

export function TrustScoreCard({ deviceKid, privateKey, wasmCrypto }: TrustScoreCardProps) {
  const scoresQuery = useTrustScores(deviceKid, privateKey, wasmCrypto);
  const budgetQuery = useTrustBudget(deviceKid, privateKey, wasmCrypto);

  if (!deviceKid) {
    return null;
  }

  if (scoresQuery.isLoading || budgetQuery.isLoading) {
    return (
      <Card withBorder padding="lg">
        <Group justify="center">
          <Loader size="sm" />
        </Group>
      </Card>
    );
  }

  const topScore = scoresQuery.data?.[0];
  const budget = budgetQuery.data;

  if (!topScore) {
    return (
      <Card withBorder padding="lg">
        <Text c="dimmed" ta="center">
          No trust score yet. Get endorsed by a trusted member to join the network.
        </Text>
      </Card>
    );
  }

  const tier = getTierInfo(topScore.distance, topScore.path_diversity);
  const budgetUsed = budget?.slots_used ?? 0;
  const budgetTotal = budget?.slots_total ?? 0;
  const budgetPercent = budgetTotal > 0 ? Math.round((budgetUsed / budgetTotal) * 100) : 0;

  return (
    <Card withBorder padding="lg">
      <Stack gap="md">
        <Group justify="space-between" align="flex-start">
          <Title order={4}>Trust Score</Title>
          {tier !== null && (
            <Badge color={tier.color} variant="light">
              {tier.label}
            </Badge>
          )}
        </Group>

        <Group gap="xl">
          <Stack gap={4} align="center">
            <Group gap={4} align="center">
              <IconShield size={16} />
              <Text size="xl" fw={700}>
                {topScore.distance.toFixed(1)}
              </Text>
            </Group>
            <Text size="xs" c="dimmed">
              Distance
            </Text>
          </Stack>

          <Stack gap={4} align="center">
            <Group gap={4} align="center">
              <IconUsers size={16} />
              <Text size="xl" fw={700}>
                {topScore.path_diversity}
              </Text>
            </Group>
            <Text size="xs" c="dimmed">
              Path Diversity
            </Text>
          </Stack>
        </Group>

        {budget !== undefined && (
          <Stack gap={4}>
            <Group justify="space-between">
              <Text size="sm">Endorsement Budget</Text>
              <Text size="sm" c="dimmed">
                {budgetUsed} / {budgetTotal}
              </Text>
            </Group>
            <Progress value={budgetPercent} size="sm" />
          </Stack>
        )}
      </Stack>
    </Card>
  );
}
