import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { TrustBudget } from '@/api/trust';
import type { ScoreSnapshot } from '../api';
import { TrustScoreCard } from './TrustScoreCard';

const FAKE_KID = 'AAAAAAAAAAAAAAAAAAAAAA';

const congressScore: ScoreSnapshot[] = [
  {
    subject_id: FAKE_KID,
    distance: 2.0,
    path_diversity: 3,
    computed_at: '2026-03-01T00:00:00Z',
  },
];

const communityScore: ScoreSnapshot[] = [
  {
    subject_id: FAKE_KID,
    distance: 5.0,
    path_diversity: 1,
    computed_at: '2026-03-01T00:00:00Z',
  },
];

const noTierScore: ScoreSnapshot[] = [
  {
    subject_id: FAKE_KID,
    distance: 8.0,
    path_diversity: 0,
    computed_at: '2026-03-01T00:00:00Z',
  },
];

const budget: TrustBudget = {
  slots_total: 10,
  slots_used: 4,
  slots_available: 6,
  out_of_slot_count: 0,
  denouncements_total: 5,
  denouncements_used: 1,
  denouncements_available: 4,
};

function SeedQueries({
  scores,
  trustBudget,
  children,
}: {
  scores: ScoreSnapshot[] | undefined;
  trustBudget: TrustBudget | undefined;
  children: React.ReactNode;
}) {
  const queryClient = useQueryClient();
  useEffect(() => {
    queryClient.setQueryData(['trust-scores', FAKE_KID], scores);
    queryClient.setQueryData(['trust-budget', FAKE_KID], trustBudget);
  }, [queryClient, scores, trustBudget]);
  return <>{children}</>;
}

export default { title: 'Trust/TrustScoreCard' };

export const NoAuth = () => <TrustScoreCard deviceKid={null} privateKey={null} wasmCrypto={null} />;

export const NoScore = () => (
  <SeedQueries scores={[]} trustBudget={undefined}>
    <TrustScoreCard deviceKid={FAKE_KID} privateKey={null} wasmCrypto={null} />
  </SeedQueries>
);

export const CongressTier = () => (
  <SeedQueries scores={congressScore} trustBudget={budget}>
    <TrustScoreCard deviceKid={FAKE_KID} privateKey={null} wasmCrypto={null} />
  </SeedQueries>
);

export const CommunityTier = () => (
  <SeedQueries scores={communityScore} trustBudget={budget}>
    <TrustScoreCard deviceKid={FAKE_KID} privateKey={null} wasmCrypto={null} />
  </SeedQueries>
);

export const NoTier = () => (
  <SeedQueries scores={noTierScore} trustBudget={budget}>
    <TrustScoreCard deviceKid={FAKE_KID} privateKey={null} wasmCrypto={null} />
  </SeedQueries>
);

export const BudgetFull = () => (
  <SeedQueries
    scores={congressScore}
    trustBudget={{ ...budget, slots_used: 10, slots_available: 0 }}
  >
    <TrustScoreCard deviceKid={FAKE_KID} privateKey={null} wasmCrypto={null} />
  </SeedQueries>
);
