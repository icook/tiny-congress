import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { ScoreSnapshot } from '@/features/trust';
import type { VerificationStatus } from '@/features/verification';
import { DeviceContext, type DeviceContextValue } from '@/providers/DeviceProvider';
import { UserAccordion } from './UserAccordion';

const noOp = () => {
  // Intentionally empty for storybook
};

const FAKE_KID = 'AAAAAAAAAAAAAAAAAAAAAA';

const authedDevice: DeviceContextValue = {
  deviceKid: FAKE_KID,
  privateKey: null,
  username: 'alice',
  isLoading: false,
  setDevice: noOp,
  clearDevice: noOp,
};

const unverifiedScore: ScoreSnapshot[] = [];
const verifiedScore: ScoreSnapshot[] = [
  {
    subject_id: FAKE_KID,
    distance: 2.5,
    path_diversity: 3,
    computed_at: '2026-03-01T00:00:00Z',
  },
];

const notVerified: VerificationStatus = { isVerified: false };
const isVerified: VerificationStatus = {
  isVerified: true,
  verifiedAt: '2026-02-15T12:00:00Z',
};

function SeedQueries({
  scores,
  verification,
  children,
}: {
  scores: ScoreSnapshot[];
  verification: VerificationStatus;
  children: React.ReactNode;
}) {
  const queryClient = useQueryClient();
  useEffect(() => {
    queryClient.setQueryData(['trust-scores', FAKE_KID], scores);
    queryClient.setQueryData(['verification-status', FAKE_KID], verification);
  }, [queryClient, scores, verification]);
  return <>{children}</>;
}

export default { title: 'Components/UserAccordion' };

export const Unverified = () => (
  <DeviceContext.Provider value={authedDevice}>
    <SeedQueries scores={unverifiedScore} verification={notVerified}>
      <div style={{ width: 260 }}>
        <UserAccordion onNavigate={noOp} />
      </div>
    </SeedQueries>
  </DeviceContext.Provider>
);

export const VerifiedWithTrustScore = () => (
  <DeviceContext.Provider value={authedDevice}>
    <SeedQueries scores={verifiedScore} verification={isVerified}>
      <div style={{ width: 260 }}>
        <UserAccordion onNavigate={noOp} />
      </div>
    </SeedQueries>
  </DeviceContext.Provider>
);

export const VerifiedNoScore = () => (
  <DeviceContext.Provider value={authedDevice}>
    <SeedQueries scores={unverifiedScore} verification={isVerified}>
      <div style={{ width: 260 }}>
        <UserAccordion onNavigate={noOp} />
      </div>
    </SeedQueries>
  </DeviceContext.Provider>
);
