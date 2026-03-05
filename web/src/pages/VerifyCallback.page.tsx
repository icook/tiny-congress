/**
 * Verification callback page
 * Handles redirects from the demo verifier with success/error status
 */

import { useEffect } from 'react';
import { IconAlertTriangle, IconCheck, IconInfoCircle } from '@tabler/icons-react';
import { useQueryClient } from '@tanstack/react-query';
import { Link, useSearch } from '@tanstack/react-router';
import { Alert, Button, Stack, Text, Title } from '@mantine/core';
import { buildVerifierUrl } from '@/features/verification';
import { useDevice } from '@/providers/DeviceProvider';

interface VerifyCallbackSearch {
  verification?: string;
  method?: string;
  message?: string;
}

const VERIFIER_ERROR_MESSAGES: Record<string, string> = {
  jwt_expired: 'Your verification session expired. Please try again.',
  signature_mismatch: 'The verification signature was invalid. Please try again.',
  user_not_found: 'Your account was not found during verification. Please try again.',
  already_verified: 'This account is already verified.',
};

function friendlyErrorMessage(raw: string | undefined): string {
  if (!raw) {
    return 'An unknown error occurred during verification.';
  }
  const decoded = decodeURIComponent(raw);
  return VERIFIER_ERROR_MESSAGES[decoded] ?? decoded;
}

export function VerifyCallbackPage() {
  const search: VerifyCallbackSearch = useSearch({ strict: false });
  const queryClient = useQueryClient();
  const { username } = useDevice();

  const isSuccess = search.verification === 'success';
  const isError = search.verification === 'error';
  const isUnknown = !isSuccess && !isError;

  useEffect(() => {
    if (isSuccess) {
      void queryClient.invalidateQueries({ queryKey: ['verification-status'] });
    }
  }, [isSuccess, queryClient]);

  const retryUrl = buildVerifierUrl(username ?? '');

  return (
    <Stack gap="md" maw={500} mx="auto" mt="xl">
      <Title order={2}>Verification</Title>

      {isSuccess ? (
        <>
          <Alert icon={<IconCheck size={16} />} title="Identity Verified" color="green">
            Your identity has been verified
            {search.method ? ` via ${search.method.replace(/_/g, ' ')}` : ''}.
          </Alert>
          <Button component={Link} to="/rooms">
            Go to Rooms Now
          </Button>
        </>
      ) : null}

      {isError ? (
        <>
          <Alert icon={<IconAlertTriangle size={16} />} title="Verification Failed" color="red">
            {friendlyErrorMessage(search.message)}
          </Alert>
          {retryUrl ? (
            <Button
              onClick={() => {
                window.location.href = retryUrl;
              }}
            >
              Try Again
            </Button>
          ) : (
            <Button component={Link} to="/rooms" variant="outline">
              Return to Rooms
            </Button>
          )}
        </>
      ) : null}

      {isUnknown ? (
        <Alert icon={<IconInfoCircle size={16} />} title="No verification result" color="gray">
          No verification result was found. If you were redirected here by mistake, you can go back
          to rooms.
        </Alert>
      ) : null}

      {isUnknown ? (
        <Button component={Link} to="/rooms" variant="outline">
          Browse Rooms
        </Button>
      ) : null}

      <Text size="xs" c="dimmed" ta="center">
        Having trouble? Make sure you&apos;re logged in to the same account you started verification
        with, then <Link to="/settings">try again from settings</Link>.
      </Text>
    </Stack>
  );
}
