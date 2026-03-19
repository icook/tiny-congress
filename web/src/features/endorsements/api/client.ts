/**
 * Endorsements API client — thin re-exports from shared @/api layer.
 * The canonical implementations live in @/api/endorsements and @/api/trust.
 */
export { getMyEndorsements } from '@/api/endorsements';
export {
  getMyBudget as getTrustBudget,
  listMyInvites as getMyInvites,
  createInvite,
  acceptInvite,
  revokeEndorsement,
} from '@/api/trust';
