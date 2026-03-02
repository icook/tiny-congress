import { render, screen, userEvent } from '@test-utils';
import { beforeEach, describe, expect, test, vi } from 'vitest';
import { DecryptionError } from '@/features/identity';
import { LoginPage } from './Login.page';

// Mock router
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...props }: { children: React.ReactNode; to: string }) => (
    <a href={to} {...props}>
      {children}
    </a>
  ),
  useNavigate: vi.fn(() => vi.fn()),
}));

// Mock the crypto provider
const mockCrypto = {
  derive_kid: vi.fn(() => 'root-kid-123'),
  encode_base64url: vi.fn(() => 'mock-encoded'),
  decode_base64url: vi.fn(() => new Uint8Array(90)),
};

vi.mock('@/providers/CryptoProvider', () => ({
  useCryptoRequired: vi.fn(() => mockCrypto),
}));

// Mock the device provider
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

// Mock identity feature exports
const mockMutateAsync = vi.fn();
const mockFetchBackup = vi.fn();
const mockDecryptBackupInWorker = vi.fn();

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
    fetchBackup: (...args: unknown[]) => mockFetchBackup(...args),
    decryptBackupInWorker: (...args: unknown[]) => mockDecryptBackupInWorker(...args),
    generateKeyPair: vi.fn(() => ({
      publicKey: new Uint8Array(32),
      privateKey: new Uint8Array(32),
      kid: 'device-kid-456',
    })),
    signMessage: vi.fn(() => new Uint8Array(64)),
    getDeviceName: vi.fn(() => 'Mac'),
  };
});

// Mock ed25519 getPublicKey
vi.mock('@noble/curves/ed25519.js', () => ({
  ed25519: {
    getPublicKey: vi.fn(() => new Uint8Array(32)),
  },
}));

describe('LoginPage', () => {
  beforeEach(() => {
    mockMutateAsync.mockReset();
    mockSetDevice.mockReset();
    mockFetchBackup.mockReset();
    mockDecryptBackupInWorker.mockReset();
    mockCrypto.derive_kid.mockReturnValue('root-kid-123');
    mockCrypto.encode_base64url.mockClear();
    mockCrypto.decode_base64url.mockReturnValue(new Uint8Array(90));
  });

  test('submits login and stores device credentials', async () => {
    mockFetchBackup.mockResolvedValue({
      encrypted_backup: 'encoded-backup',
      root_kid: 'root-kid-123',
    });
    mockDecryptBackupInWorker.mockResolvedValue(new Uint8Array(32));
    mockMutateAsync.mockResolvedValue({
      account_id: 'acc-1',
      root_kid: 'root-kid-123',
      device_kid: 'dev-789',
    });

    const user = userEvent.setup();
    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    // Should fetch backup for the trimmed username
    expect(mockFetchBackup).toHaveBeenCalledWith('alice');

    // Should decrypt the envelope
    expect(mockDecryptBackupInWorker).toHaveBeenCalledWith(expect.any(Uint8Array), 'my-password');

    // Should call login with device info
    expect(mockMutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        username: 'alice',
        device: expect.objectContaining({
          pubkey: 'mock-encoded',
          name: 'Mac',
          certificate: 'mock-encoded',
        }),
      })
    );

    // Should store device credentials
    expect(mockSetDevice).toHaveBeenCalledWith('dev-789', expect.any(Uint8Array));
  });

  test('does not submit when username is blank', async () => {
    const user = userEvent.setup();
    render(<LoginPage />);

    // Only fill password, leave username empty
    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(mockFetchBackup).not.toHaveBeenCalled();
  });

  test('does not submit when password is blank', async () => {
    const user = userEvent.setup();
    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(mockFetchBackup).not.toHaveBeenCalled();
  });

  test('shows "Wrong password" error on decryption failure', async () => {
    mockFetchBackup.mockResolvedValue({
      encrypted_backup: 'encoded-backup',
      root_kid: 'root-kid-123',
    });
    mockDecryptBackupInWorker.mockRejectedValue(
      new DecryptionError('Wrong password or corrupted backup')
    );

    const user = userEvent.setup();
    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.type(screen.getByLabelText(/backup password/i), 'wrong-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(await screen.findByText(/Wrong password or corrupted backup/)).toBeInTheDocument();
  });

  test('shows API error message on login mutation failure', async () => {
    mockFetchBackup.mockResolvedValue({
      encrypted_backup: 'encoded-backup',
      root_kid: 'root-kid-123',
    });
    mockDecryptBackupInWorker.mockResolvedValue(new Uint8Array(32));
    mockMutateAsync.mockRejectedValue(new Error('Account not found'));

    const user = userEvent.setup();
    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(await screen.findByText(/Account not found/)).toBeInTheDocument();
  });

  test('shows error when root_kid integrity check fails', async () => {
    mockFetchBackup.mockResolvedValue({
      encrypted_backup: 'encoded-backup',
      root_kid: 'server-root-kid',
    });
    mockDecryptBackupInWorker.mockResolvedValue(new Uint8Array(32));
    // derive_kid returns a DIFFERENT kid than the server's root_kid
    mockCrypto.derive_kid.mockReturnValue('different-derived-kid');

    const user = userEvent.setup();
    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), 'alice');
    await user.type(screen.getByLabelText(/backup password/i), 'my-password');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(await screen.findByText(/Backup integrity check failed/)).toBeInTheDocument();

    // Should NOT call the login API
    expect(mockMutateAsync).not.toHaveBeenCalled();
  });

  test('trims username before sending', async () => {
    mockFetchBackup.mockResolvedValue({
      encrypted_backup: 'encoded-backup',
      root_kid: 'root-kid-123',
    });
    mockDecryptBackupInWorker.mockResolvedValue(new Uint8Array(32));
    mockMutateAsync.mockResolvedValue({
      account_id: 'acc-1',
      root_kid: 'root-kid-123',
      device_kid: 'dev-789',
    });

    const user = userEvent.setup();
    render(<LoginPage />);

    await user.type(screen.getByLabelText(/username/i), '  alice  ');
    await user.type(screen.getByLabelText(/backup password/i), 'pw');
    await user.click(screen.getByRole('button', { name: /log in/i }));

    expect(mockFetchBackup).toHaveBeenCalledWith('alice');
    expect(mockMutateAsync).toHaveBeenCalledWith(expect.objectContaining({ username: 'alice' }));
  });
});
