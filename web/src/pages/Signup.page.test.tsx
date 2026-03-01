import { render, screen, userEvent } from '@test-utils';
import { beforeEach, describe, expect, test, vi } from 'vitest';
import { SignupPage } from './Signup.page';

// Mock router Link to avoid needing RouterProvider in tests
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...props }: { children: React.ReactNode; to: string }) => (
    <a href={to} {...props}>
      {children}
    </a>
  ),
  useNavigate: vi.fn(() => vi.fn()),
}));

// Hoist shared mocks so vi.mock factories (which are also hoisted) can reference them
const { mockCrypto, mockSetDevice, mockMutateAsync, mockCryptoKey } = vi.hoisted(() => ({
  mockCrypto: {
    derive_kid: vi.fn(() => 'kid-123'),
    encode_base64url: vi.fn(() => 'mock-encoded'),
    decode_base64url: vi.fn(() => new Uint8Array(32)),
  },
  mockSetDevice: vi.fn(),
  mockMutateAsync: vi.fn(),
  // Mock CryptoKey (non-extractable device key)
  mockCryptoKey: { type: 'private', algorithm: { name: 'Ed25519' } } as CryptoKey,
}));

vi.mock('@/providers/CryptoProvider', () => ({
  useCryptoRequired: vi.fn(() => mockCrypto),
}));

vi.mock('@/providers/DeviceProvider', () => ({
  useDevice: vi.fn(() => ({
    deviceKid: null,
    privateKey: null,
    isLoading: false,
    setDevice: mockSetDevice,
    clearDevice: vi.fn(),
  })),
}));

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
      publicKey: new Uint8Array(32),
      privateKey: new Uint8Array(32),
      kid: 'kid-123',
    })),
    generateDeviceKeyPair: vi.fn().mockResolvedValue({
      publicKey: new Uint8Array(32),
      privateKey: mockCryptoKey,
    }),
    signMessage: vi.fn(() => new Uint8Array(64)),
    buildBackupEnvelope: vi.fn().mockResolvedValue(new Uint8Array(90)),
  };
});

describe('SignupPage', () => {
  beforeEach(() => {
    mockMutateAsync.mockReset();
    mockSetDevice.mockReset();
    mockCrypto.encode_base64url.mockClear();
    mockCrypto.derive_kid.mockClear();
  });

  test('submits signup with full payload and stores device', async () => {
    mockMutateAsync.mockResolvedValue({
      account_id: 'abc',
      root_kid: 'kid-123',
      device_kid: 'dev-456',
    });
    const user = userEvent.setup();

    render(<SignupPage />);

    await user.type(screen.getByLabelText(/username/i), ' alice ');
    await user.type(screen.getByLabelText(/backup password/i), 'test-password');
    await user.click(screen.getByRole('button', { name: /sign up/i }));

    expect(mockMutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        username: 'alice',
        root_pubkey: 'mock-encoded',
        backup: expect.objectContaining({
          encrypted_blob: 'mock-encoded',
        }),
        device: expect.objectContaining({
          pubkey: 'mock-encoded',
          certificate: 'mock-encoded',
        }),
      })
    );

    // Should store device credentials (non-extractable CryptoKey)
    expect(mockSetDevice).toHaveBeenCalledWith('dev-456', mockCryptoKey);

    expect(await screen.findByText(/Account ID:/i)).toBeInTheDocument();
    expect(screen.getByText(/abc/)).toBeInTheDocument();
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
    await user.type(screen.getByLabelText(/backup password/i), 'test-password');
    await user.click(screen.getByRole('button', { name: /sign up/i }));

    expect(await screen.findByText(/boom/)).toBeInTheDocument();
  });

  test('does not submit when username is blank', async () => {
    mockMutateAsync.mockResolvedValue({
      account_id: 'abc',
      root_kid: 'kid-123',
      device_kid: 'dev-456',
    });
    const user = userEvent.setup();

    render(<SignupPage />);

    await user.click(screen.getByRole('button', { name: /sign up/i }));

    expect(mockMutateAsync).not.toHaveBeenCalled();
  });
});
