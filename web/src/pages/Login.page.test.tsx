import { render, screen, userEvent } from '@test-utils';
import { beforeEach, describe, expect, test, vi } from 'vitest';
import { LoginPage } from './Login.page';

// Mock the crypto provider
const mockCrypto = {
  derive_kid: vi.fn(() => 'kid-root'),
  encode_base64url: vi.fn(() => 'mock-encoded'),
  decode_base64url: vi.fn(() => new Uint8Array(90)),
};

vi.mock('@/providers/CryptoProvider', () => ({
  useCryptoRequired: vi.fn(() => mockCrypto),
}));

// Mock the DeviceProvider
const mockSetDevice = vi.fn();
vi.mock('@/providers/DeviceProvider', () => ({
  useDevice: vi.fn(() => ({
    deviceKid: null,
    privateKey: null,
    isLoading: false,
    setDevice: mockSetDevice,
    clearDevice: vi.fn(),
  })),
}));

// Mock ed25519 for getPublicKey (backup integrity check)
vi.mock('@noble/curves/ed25519.js', () => ({
  ed25519: {
    getPublicKey: vi.fn(() => new Uint8Array(32)),
  },
}));

// Mock router navigation and Link to avoid needing RouterProvider in tests
const mockNavigate = vi.fn();
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...props }: { children: React.ReactNode; to: string }) => (
    <a href={to} {...props}>
      {children}
    </a>
  ),
  useNavigate: vi.fn(() => mockNavigate),
}));

// Mock the login mutation and crypto functions
const mockMutateAsync = vi.fn();
const mockFetchBackup = vi.fn();
const mockDecryptBackupEnvelope = vi.fn();

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
    fetchBackup: (...args: unknown[]) => mockFetchBackup(...args),
    decryptBackupEnvelope: (...args: unknown[]) => mockDecryptBackupEnvelope(...args),
  };
});

describe('LoginPage', () => {
  beforeEach(() => {
    mockMutateAsync.mockReset();
    mockFetchBackup.mockReset();
    mockDecryptBackupEnvelope.mockReset();
    mockSetDevice.mockReset();
    mockNavigate.mockReset();
    mockCrypto.encode_base64url.mockReset();
    mockCrypto.decode_base64url.mockReset();
    mockCrypto.derive_kid.mockReset();

    // Restore default implementations after reset
    mockCrypto.derive_kid.mockReturnValue('kid-root');
    mockCrypto.encode_base64url.mockReturnValue('mock-encoded');
    mockCrypto.decode_base64url.mockReturnValue(new Uint8Array(90));

    // Default happy path mocks
    mockFetchBackup.mockResolvedValue({
      encrypted_backup: 'mock-backup-blob',
      root_kid: 'kid-root',
    });
    mockDecryptBackupEnvelope.mockResolvedValue(new Uint8Array(32));
  });

  test('submits login with backup decryption and timestamp', async () => {
    mockMutateAsync.mockResolvedValue({
      account_id: 'abc',
      root_kid: 'kid-root',
      device_kid: 'kid-device',
    });
    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), ' alice ');
    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    // Should fetch backup for the trimmed username
    expect(mockFetchBackup).toHaveBeenCalledWith('alice');

    // Should decrypt the backup envelope with the password
    expect(mockDecryptBackupEnvelope).toHaveBeenCalledWith(
      expect.any(Uint8Array) as Uint8Array,
      'my-password'
    );

    // Should call the login API with timestamp
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

    // Should store device credentials
    expect(mockSetDevice).toHaveBeenCalledWith('kid-device', expect.any(Uint8Array) as Uint8Array);
  });

  test('shows error when backup decryption fails (wrong password)', async () => {
    mockDecryptBackupEnvelope.mockRejectedValue(new Error('Wrong password or corrupted backup'));

    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.type(screen.getByLabelText(/backup password/i), 'wrong-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(await screen.findByText(/Wrong password or corrupted backup/)).toBeInTheDocument();
    expect(mockMutateAsync).not.toHaveBeenCalled();
  });

  test('shows error when login API fails', async () => {
    mockMutateAsync.mockRejectedValue(new Error('Invalid credentials'));

    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(await screen.findByText(/Invalid credentials/)).toBeInTheDocument();
  });

  test('shows error when root_kid integrity check fails', async () => {
    // derive_kid returns a KID that does NOT match root_kid in the backup response
    mockCrypto.derive_kid.mockReturnValue('kid-WRONG');

    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(await screen.findByText(/Backup integrity check failed/)).toBeInTheDocument();
    // Login API should NOT have been called
    expect(mockMutateAsync).not.toHaveBeenCalled();
  });

  test('does not submit when username is blank', async () => {
    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(mockFetchBackup).not.toHaveBeenCalled();
    expect(mockMutateAsync).not.toHaveBeenCalled();
  });

  test('does not submit when password is blank', async () => {
    const user = userEvent.setup();

    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(mockFetchBackup).not.toHaveBeenCalled();
    expect(mockMutateAsync).not.toHaveBeenCalled();
  });
});
