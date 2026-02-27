/**
 * Signup page - Route-level container
 * Handles hooks, crypto, and API calls, delegates rendering to SignupForm
 */

import { useState } from 'react';
import { buildBackupEnvelope, generateKeyPair, signMessage, useSignup } from '@/features/identity';
import { SignupForm } from '@/features/identity/components';
import { useCryptoRequired } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

export function SignupPage() {
  const crypto = useCryptoRequired();
  const signup = useSignup();
  const { setDevice } = useDevice();

  const [username, setUsername] = useState('');
  const [isGeneratingKeys, setIsGeneratingKeys] = useState(false);
  const [createdAccount, setCreatedAccount] = useState<{
    account_id: string;
    root_kid: string;
    device_kid: string;
  } | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!username.trim()) {
      return;
    }

    setIsGeneratingKeys(true);

    try {
      // Generate root key pair
      const rootKeyPair = generateKeyPair(crypto);

      // Generate device key pair
      const deviceKeyPair = generateKeyPair(crypto);

      // Sign the device pubkey with the root key (certificate)
      const certificate = signMessage(deviceKeyPair.publicKey, rootKeyPair.privateKey);

      // Build backup envelope
      const envelope = buildBackupEnvelope();

      // Call signup API with full payload
      const response = await signup.mutateAsync({
        username: username.trim(),
        root_pubkey: crypto.encode_base64url(rootKeyPair.publicKey),
        backup: {
          encrypted_blob: crypto.encode_base64url(envelope),
        },
        device: {
          pubkey: crypto.encode_base64url(deviceKeyPair.publicKey),
          name: getDeviceName(),
          certificate: crypto.encode_base64url(certificate),
        },
      });

      // Store device credentials in session context
      setDevice(response.device_kid, deviceKeyPair.privateKey);

      setCreatedAccount(response);
    } catch {
      // Error is handled by TanStack Query mutation state
    } finally {
      setIsGeneratingKeys(false);
    }
  };

  return (
    <SignupForm
      username={username}
      onUsernameChange={setUsername}
      onSubmit={(e) => {
        void handleSubmit(e);
      }}
      isLoading={signup.isPending || isGeneratingKeys}
      loadingText={isGeneratingKeys ? 'Generating keys...' : undefined}
      error={signup.isError ? signup.error.message : null}
      successData={createdAccount}
    />
  );
}

/**
 * Attempt to derive a reasonable device name from the browser.
 */
function getDeviceName(): string {
  const ua = navigator.userAgent;
  if (ua.includes('iPhone') || ua.includes('iPad') || ua.includes('iPod')) {
    return 'iOS Device';
  }
  if (ua.includes('Android')) {
    return 'Android Device';
  }
  if (ua.includes('Mac')) {
    return 'Mac';
  }
  if (ua.includes('Windows')) {
    return 'Windows PC';
  }
  if (ua.includes('Linux')) {
    return 'Linux';
  }
  return 'Browser';
}
