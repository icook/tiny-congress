import { render, screen, userEvent } from '@test-utils';
import { beforeEach, describe, expect, test, vi } from 'vitest';
import { LoginPage } from './Login.page';

// Mock the crypto provider
const mockCrypto = {
  derive_kid: vi.fn(() => 'kid-123'),
  encode_base64url: vi.fn(() => 'mock-encoded'),
  decode_base64url: vi.fn(() => new Uint8Array(32)),
};

vi.mock('@/providers/CryptoProvider', () => ({
  useCryptoRequired: vi.fn(() => mockCrypto),
}));

// Mock the login mutation and crypto functions
const mockMutateAsync = vi.fn();
vi.mock('@/features/identity', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/features/identity')>();
  return {
    ...original,
    useLogin: vi.fn(() => ({
      mutateAsync: mockMutateAsync,
      isPending: false,
      isError: false,
      error: null,
    })),
    generateKeyPair: vi.fn(() => ({
      publicKey: new Uint8Array(32),
      privateKey: new Uint8Array(32),
      kid: 'kid-123',
    })),
    signMessage: vi.fn(() => new Uint8Array(64)),
  };
});

describe('LoginPage', () => {
  beforeEach(() => {
    mockMutateAsync.mockReset();
    mockCrypto.encode_base64url.mockClear();
    mockCrypto.derive_kid.mockClear();
  });

  test('submits login with timestamp and shows session details', async () => {
    mockMutateAsync.mockResolvedValue({
      account_id: 'abc',
      root_kid: 'kid-root',
      device_kid: 'kid-device',
    });
    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), ' alice ');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(mockMutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        username: 'alice',
        timestamp: expect.any(Number) as number,
        device: expect.objectContaining({
          pubkey: 'mock-encoded',
          name: expect.any(String) as string,
          certificate: 'mock-encoded',
        }) as Record<string, unknown>,
      })
    );

    expect(await screen.findByText(/Account ID:/i)).toBeInTheDocument();
    expect(screen.getByText(/abc/)).toBeInTheDocument();
    expect(screen.getByText(/kid-root/)).toBeInTheDocument();
    expect(screen.getByText(/kid-device/)).toBeInTheDocument();
  });

  test('shows error when login fails', async () => {
    mockMutateAsync.mockRejectedValue(new Error('Invalid credentials'));

    const { useLogin } = await import('@/features/identity');
    vi.mocked(useLogin).mockReturnValue({
      mutateAsync: mockMutateAsync,
      isPending: false,
      isError: true,
      error: new Error('Invalid credentials'),
    } as unknown as ReturnType<typeof useLogin>);

    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(await screen.findByText(/Invalid credentials/)).toBeInTheDocument();
  });

  test('does not submit when username is blank', async () => {
    mockMutateAsync.mockResolvedValue({
      account_id: 'abc',
      root_kid: 'kid-root',
      device_kid: 'kid-device',
    });
    const user = userEvent.setup();

    render(<LoginPage />);

    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(mockMutateAsync).not.toHaveBeenCalled();
  });
});
