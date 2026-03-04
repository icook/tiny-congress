/**
 * Builds the URL to redirect users to the demo verifier
 */

import { getDemoVerifierUrl } from '@/config';

export function buildVerifierUrl(username: string): string | null {
  const verifierBase = getDemoVerifierUrl();
  if (!verifierBase) {
    return null;
  }

  const callback = encodeURIComponent(window.location.origin);
  return `${verifierBase}/?callback=${callback}&username=${encodeURIComponent(username)}`;
}
