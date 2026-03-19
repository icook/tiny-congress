import { render, screen, userEvent, waitFor } from '@test-utils';
import { describe, expect, it, vi } from 'vitest';
import { useDenounce, useLookupAccount, useMyDenouncements, type TrustBudget } from '../api';
import { DenouncementSection } from './DenouncementSection';

// Mock the API hooks so tests don't need real auth/network.
// vi.mock is hoisted by vitest, so the import above sees the mocked versions.
vi.mock('../api', () => ({
  useMyDenouncements: vi.fn(),
  useDenounce: vi.fn(),
  useLookupAccount: vi.fn(),
}));

const mockUseMyDenouncements = vi.mocked(useMyDenouncements);
const mockUseDenounce = vi.mocked(useDenounce);
const mockUseLookupAccount = vi.mocked(useLookupAccount);

function defaultMocks() {
  mockUseMyDenouncements.mockReturnValue({
    data: [],
    isLoading: false,
    isError: false,
    error: null,
  } as unknown as ReturnType<typeof useMyDenouncements>);

  mockUseDenounce.mockReturnValue({
    mutateAsync: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
  } as unknown as ReturnType<typeof useDenounce>);

  mockUseLookupAccount.mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: false,
    error: null,
  } as unknown as ReturnType<typeof useLookupAccount>);
}

function makeBudget(overrides?: Partial<TrustBudget>): TrustBudget {
  return {
    slots_total: 3,
    slots_used: 0,
    slots_available: 3,
    out_of_slot_count: 0,
    denouncements_total: 2,
    denouncements_used: 0,
    denouncements_available: 2,
    ...overrides,
  };
}

const defaultProps = {
  deviceKid: 'test-kid',
  privateKey: null,
  wasmCrypto: null,
  budget: makeBudget(),
};

describe('DenouncementSection', () => {
  it('shows budget badge with usage', () => {
    defaultMocks();
    render(
      <DenouncementSection
        {...defaultProps}
        budget={makeBudget({ denouncements_used: 1, denouncements_available: 1 })}
      />
    );

    expect(screen.getByText('1/2 used')).toBeInTheDocument();
  });

  it('shows 0/2 badge when budget is unused', () => {
    defaultMocks();
    render(<DenouncementSection {...defaultProps} />);

    expect(screen.getByText('0/2 used')).toBeInTheDocument();
  });

  it('hides input fields when budget is exhausted', () => {
    defaultMocks();
    render(
      <DenouncementSection
        {...defaultProps}
        budget={makeBudget({ denouncements_used: 2, denouncements_available: 0 })}
      />
    );

    expect(screen.queryByLabelText('Username to denounce')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Reason')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /file denouncement/i })).not.toBeInTheDocument();
  });

  it('shows exhausted budget alert when slots are used up', () => {
    defaultMocks();
    render(
      <DenouncementSection
        {...defaultProps}
        budget={makeBudget({ denouncements_used: 2, denouncements_available: 0 })}
      />
    );

    expect(screen.getByText(/you have used all 2 denouncement slots/i)).toBeInTheDocument();
  });

  it('disables the File Denouncement button when fields are empty', () => {
    defaultMocks();
    render(<DenouncementSection {...defaultProps} />);

    const button = screen.getByRole('button', { name: /file denouncement/i });
    expect(button).toBeDisabled();
  });

  it('enables the File Denouncement button when both fields are filled', async () => {
    defaultMocks();
    const user = userEvent.setup();
    render(<DenouncementSection {...defaultProps} />);

    await user.type(screen.getByLabelText('Username to denounce'), 'alice');
    await user.type(screen.getByLabelText('Reason'), 'Violated community rules');

    expect(screen.getByRole('button', { name: /file denouncement/i })).toBeEnabled();
  });

  it('shows confirmation modal when File Denouncement is clicked', async () => {
    defaultMocks();
    const user = userEvent.setup();
    render(<DenouncementSection {...defaultProps} />);

    await user.type(screen.getByLabelText('Username to denounce'), 'alice');
    await user.type(screen.getByLabelText('Reason'), 'Violated community rules');
    await user.click(screen.getByRole('button', { name: /file denouncement/i }));

    await waitFor(() => {
      expect(screen.getByText('Confirm Denouncement')).toBeInTheDocument();
    });
    expect(screen.getByText(/this action is irreversible/i)).toBeInTheDocument();
  });

  it('shows existing denouncements in list', () => {
    mockUseMyDenouncements.mockReturnValue({
      data: [
        {
          id: 'uuid-1',
          target_id: 'target-uuid',
          target_username: 'badactor',
          reason: 'Spamming',
          created_at: '2026-03-01T00:00:00Z',
        },
      ],
      isLoading: false,
      isError: false,
      error: null,
    } as unknown as ReturnType<typeof useMyDenouncements>);
    mockUseDenounce.mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
      isError: false,
      error: null,
    } as unknown as ReturnType<typeof useDenounce>);
    mockUseLookupAccount.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: false,
      error: null,
    } as unknown as ReturnType<typeof useLookupAccount>);

    render(
      <DenouncementSection
        {...defaultProps}
        budget={makeBudget({ denouncements_used: 1, denouncements_available: 1 })}
      />
    );

    expect(screen.getByText('badactor')).toBeInTheDocument();
  });

  it('shows "No active denouncements" when list is empty', () => {
    defaultMocks();
    render(<DenouncementSection {...defaultProps} />);

    expect(screen.getByText('No active denouncements.')).toBeInTheDocument();
  });
});
