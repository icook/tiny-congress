export interface Endorsement {
  id: string;
  subject_id: string;
  topic: string;
  issuer_id: string | null;
  created_at: string;
  revoked: boolean;
}

export interface EndorsementsListResponse {
  endorsements: Endorsement[];
}

export interface BudgetResponse {
  slots_total: number;
  slots_used: number;
  slots_available: number;
  denouncements_total: number;
  denouncements_used: number;
  denouncements_available: number;
}

export interface CreateInviteResponse {
  id: string;
  expires_at: string;
}

export interface InviteResponse {
  id: string;
  delivery_method: string;
  accepted_by: string | null;
  expires_at: string;
  accepted_at: string | null;
}

export interface InvitesListResponse {
  invites: InviteResponse[];
}

export interface AcceptInviteResponse {
  endorser_id: string;
  accepted_at: string;
}

export interface CreateInvitePayload {
  envelope: string;
  delivery_method: string;
  relationship_depth?: string;
  attestation: Record<string, unknown>;
}
