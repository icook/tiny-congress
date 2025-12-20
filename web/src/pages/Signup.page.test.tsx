import { render, screen, userEvent } from '@test-utils';
import { beforeEach, describe, expect, test, vi } from 'vitest';
import { SignupPage } from './Signup.page';

// Mock the crypto provider
const mockCrypto = {
  derive_kid: vi.fn(() => 'kid-123'),
  encode_base64url: vi.fn(() => 'mock-pubkey'),
  decode_base64url: vi.fn(() => new Uint8Array(32)),
};

vi.mock('@/providers/CryptoProvider', () => ({
  useCryptoRequired: vi.fn(() => mockCrypto),
}));

// Mock the signup mutation
const mockMutateAsync = vi.fn();
vi.mock('@/features/identity', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/features/identity')>();
  return {
    ...original,
    useSignup: vi.fn(() => ({
      mutateAsync: mockMutateAsync,
      isPending: false,
      isError: false,
      error: null,
    })),
    generateKeyPair: vi.fn(() => ({
      publicKey: new Uint8Array([
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        26, 27, 28, 29, 30, 31, 32,
      ]),
      privateKey: new Uint8Array(32),
      kid: 'kid-123',
    })),
  };
});

describe('SignupPage', () => {
  beforeEach(() => {
    mockMutateAsync.mockReset();
    mockCrypto.encode_base64url.mockClear();
    mockCrypto.derive_kid.mockClear();
  });

  test('submits signup and shows account details', async () => {
    mockMutateAsync.mockResolvedValue({ account_id: 'abc', root_kid: 'kid-123' });
    const user = userEvent.setup();

    render(<SignupPage />);

    await user.type(screen.getByLabelText(/username/i), ' alice ');
    await user.click(screen.getByRole('button', { name: /sign up/i }));

    expect(mockMutateAsync).toHaveBeenCalledWith({
      username: 'alice',
      root_pubkey: 'mock-pubkey',
    });

    expect(await screen.findByText(/Account ID:/i)).toBeInTheDocument();
    expect(screen.getByText(/abc/)).toBeInTheDocument();
    expect(screen.getByText(/kid-123/)).toBeInTheDocument();
  });

  test('shows an error message when signup fails', async () => {
    // Return a rejected mutation with error state
    mockMutateAsync.mockRejectedValue(new Error('boom'));

    // Re-import to get fresh mock with error state
    const { useSignup } = await import('@/features/identity');
    vi.mocked(useSignup).mockReturnValue({
      mutateAsync: mockMutateAsync,
      isPending: false,
      isError: true,
      error: new Error('boom'),
    } as unknown as ReturnType<typeof useSignup>);

    const user = userEvent.setup();

    render(<SignupPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.click(screen.getByRole('button', { name: /sign up/i }));

    expect(await screen.findByText(/boom/)).toBeInTheDocument();
  });

  test('does not submit when username is blank', async () => {
    mockMutateAsync.mockResolvedValue({ account_id: 'abc', root_kid: 'kid-123' });
    const user = userEvent.setup();

    render(<SignupPage />);

    await user.click(screen.getByRole('button', { name: /sign up/i }));

    expect(mockMutateAsync).not.toHaveBeenCalled();
  });
});
