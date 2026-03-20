import { render, screen } from '@test-utils';
import { describe, expect, it, vi } from 'vitest';
import { SuggestionFeed } from './SuggestionFeed';

// Stub out the WASM module — it doesn't exist in the test environment.
vi.mock('@/wasm/tc-crypto/tc_crypto.js', () => ({
  default: vi.fn(() => Promise.resolve()),
  derive_kid: vi.fn(() => 'kid-test'),
  encode_base64url: vi.fn(() => 'encoded'),
  decode_base64url: vi.fn(() => new Uint8Array(0)),
}));

// Mock the provider hooks so we can control auth state
const mockUseDevice = vi.fn();
vi.mock('@/providers/DeviceProvider', () => ({
  useDevice: () => mockUseDevice(),
}));

const mockUseCrypto = vi.fn();
vi.mock('@/providers/CryptoProvider', () => ({
  useCrypto: () => mockUseCrypto(),
}));

// Mock the API hooks
const mockUseSuggestions = vi.fn();
const mockUseCreateSuggestion = vi.fn();
vi.mock('../api', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api')>();
  return {
    ...original,
    useSuggestions: (...args: unknown[]) => mockUseSuggestions(...args),
    useCreateSuggestion: (...args: unknown[]) => mockUseCreateSuggestion(...args),
  };
});

function unauthenticatedDevice() {
  mockUseDevice.mockReturnValue({ deviceKid: null, privateKey: null, username: null });
  mockUseCrypto.mockReturnValue({ crypto: null, isLoading: false, error: null });
}

function authenticatedDevice() {
  mockUseDevice.mockReturnValue({
    deviceKid: 'kid-abc123',
    privateKey: {} as CryptoKey,
    username: 'testuser',
  });
  mockUseCrypto.mockReturnValue({
    crypto: { derive_kid: vi.fn(), encode_base64url: vi.fn(), decode_base64url: vi.fn() },
    isLoading: false,
    error: null,
  });
}

function mockCreateMutation() {
  mockUseCreateSuggestion.mockReturnValue({ mutate: vi.fn(), isPending: false, error: null });
}

describe('SuggestionFeed', () => {
  it('renders empty state message when suggestions list is empty', () => {
    unauthenticatedDevice();
    mockUseSuggestions.mockReturnValue({ data: [], isLoading: false });
    mockCreateMutation();

    render(<SuggestionFeed roomId="room-1" pollId="poll-1" />);

    expect(screen.getByText(/No suggestions yet/)).toBeInTheDocument();
  });

  it('renders suggestion items with correct text and status badges', () => {
    unauthenticatedDevice();
    mockUseSuggestions.mockReturnValue({
      data: [
        {
          id: 's-1',
          room_id: 'room-1',
          poll_id: 'poll-1',
          account_id: 'acc-1',
          suggestion_text: 'Investigate labor conditions',
          status: 'queued',
          filter_reason: null,
          evidence_ids: [],
          created_at: '2026-01-01T00:00:00Z',
          processed_at: null,
        },
        {
          id: 's-2',
          room_id: 'room-1',
          poll_id: 'poll-1',
          account_id: 'acc-2',
          suggestion_text: 'Review wage statistics',
          status: 'completed',
          filter_reason: null,
          evidence_ids: [],
          created_at: '2026-01-01T01:00:00Z',
          processed_at: '2026-01-01T02:00:00Z',
        },
        {
          id: 's-3',
          room_id: 'room-1',
          poll_id: 'poll-1',
          account_id: 'acc-3',
          suggestion_text: 'Off-topic request',
          status: 'rejected',
          filter_reason: 'Does not relate to the poll topic.',
          evidence_ids: [],
          created_at: '2026-01-01T03:00:00Z',
          processed_at: '2026-01-01T04:00:00Z',
        },
      ],
      isLoading: false,
    });
    mockCreateMutation();

    render(<SuggestionFeed roomId="room-1" pollId="poll-1" />);

    expect(screen.getByText('Investigate labor conditions')).toBeInTheDocument();
    expect(screen.getByText('Review wage statistics')).toBeInTheDocument();
    expect(screen.getByText('Off-topic request')).toBeInTheDocument();

    // Status badges
    expect(screen.getByText('queued')).toBeInTheDocument();
    expect(screen.getByText('completed')).toBeInTheDocument();
    expect(screen.getByText('rejected')).toBeInTheDocument();

    // Rejected item shows filter reason
    expect(screen.getByText('Does not relate to the poll topic.')).toBeInTheDocument();
  });

  it('hides input when not authenticated', () => {
    unauthenticatedDevice();
    mockUseSuggestions.mockReturnValue({ data: [], isLoading: false });
    mockCreateMutation();

    render(<SuggestionFeed roomId="room-1" pollId="poll-1" />);

    expect(screen.queryByPlaceholderText(/Suggest something/)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Suggest/i })).not.toBeInTheDocument();
  });

  it('shows input and submit button when authenticated', () => {
    authenticatedDevice();
    mockUseSuggestions.mockReturnValue({ data: [], isLoading: false });
    mockCreateMutation();

    render(<SuggestionFeed roomId="room-1" pollId="poll-1" />);

    expect(screen.getByPlaceholderText(/Suggest something/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Suggest/i })).toBeInTheDocument();
  });
});
