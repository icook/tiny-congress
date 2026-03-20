import type { Endorsement } from '@/api/endorsements';
import { EndorsementList } from './EndorsementList';

export default { title: 'Endorsements/EndorsementList' };

const noOp = () => {
  // Intentionally empty for storybook
};

const sampleEndorsements: Endorsement[] = [
  {
    id: '1',
    subject_id: 'user-alice',
    topic: 'trust',
    issuer_id: 'user-bob',
    created_at: '2026-03-01T12:00:00Z',
    revoked: false,
  },
  {
    id: '2',
    subject_id: 'user-charlie',
    topic: 'trust',
    issuer_id: 'user-bob',
    created_at: '2026-03-10T08:30:00Z',
    revoked: false,
  },
];

export const Empty = () => <EndorsementList endorsements={[]} onRevoke={noOp} isRevoking={false} />;

export const WithEndorsements = () => (
  <EndorsementList endorsements={sampleEndorsements} onRevoke={noOp} isRevoking={false} />
);

export const Revoking = () => (
  <EndorsementList endorsements={sampleEndorsements} onRevoke={noOp} isRevoking />
);
