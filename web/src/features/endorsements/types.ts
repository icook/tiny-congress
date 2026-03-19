// InvitesListResponse: endorsements had { invites: InviteResponse[] }.
// No direct equivalent in @/api/trust (listMyInvites returns Invite[]).
import type { Invite } from '@/api/trust';

/**
 * Re-exports from shared @/api layer — single source of truth for endorsement types.
 */
export type { Endorsement, EndorsementsListResponse } from '@/api/endorsements';
export type { TrustBudget as BudgetResponse } from '@/api/trust';
export type { Invite as InviteResponse } from '@/api/trust';
export type { CreateInvitePayload } from '@/api/trust';
export type { AcceptInviteResult as AcceptInviteResponse } from '@/api/trust';

// createInvite returns Invite (same shape as InviteResponse); alias for backward compat.
export type { Invite as CreateInviteResponse } from '@/api/trust';

export interface InvitesListResponse {
  invites: Invite[];
}
