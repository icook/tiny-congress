/**
 * Signup page - Route-level container
 * Handles hooks, crypto, and API calls, delegates rendering to SignupForm
 */

import { useState } from 'react';
import { generateKeyPair, useSignup } from '@/features/identity';
import { SignupForm } from '@/features/identity/components';
import { useCryptoRequired } from '@/providers/CryptoProvider';

export function SignupPage() {
  const crypto = useCryptoRequired();
  const signup = useSignup();

  const [username, setUsername] = useState('');
  const [isGeneratingKeys, setIsGeneratingKeys] = useState(false);
  const [createdAccount, setCreatedAccount] = useState<{
    account_id: string;
    root_kid: string;
  } | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!username.trim()) {
      return;
    }

    setIsGeneratingKeys(true);

    try {
      // Generate key pair (uses WASM for KID derivation)
      const keyPair = generateKeyPair(crypto);

      // Call signup API
      const response = await signup.mutateAsync({
        username: username.trim(),
        root_pubkey: crypto.encode_base64url(keyPair.publicKey),
      });

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
      onSubmit={handleSubmit}
      isLoading={signup.isPending || isGeneratingKeys}
      loadingText={isGeneratingKeys ? 'Generating keys...' : undefined}
      error={signup.isError ? signup.error?.message || 'An error occurred' : null}
      successData={createdAccount}
    />
  );
}
