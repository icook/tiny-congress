/**
 * Verification callback page
 * Handles redirects from the demo verifier with success/error status
 */

import { useEffect } from 'react';
import { IconAlertTriangle, IconCheck } from '@tabler/icons-react';
import { useQueryClient } from '@tanstack/react-query';
import { useNavigate, useSearch } from '@tanstack/react-router';
import { Alert, Button, Stack, Text, Title } from '@mantine/core';
import { buildVerifierUrl } from '@/features/verification';

interface VerifyCallbackSearch {
  verification?: string;
  method?: string;
  message?: string;
}

export function VerifyCallbackPage() {
  const search: VerifyCallbackSearch = useSearch({ strict: false });
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const isSuccess = search.verification === 'success';
  const isError = search.verification === 'error';

  useEffect(() => {
    if (isSuccess) {
      void queryClient.invalidateQueries({ queryKey: ['verification-status'] });

      const timer = setTimeout(() => {
        void navigate({ to: '/rooms' });
      }, 2000);

      return () => {
        clearTimeout(timer);
      };
    }
  }, [isSuccess, navigate, queryClient]);

  return (
    <Stack gap="md" maw={500} mx="auto" mt="xl">
      <Title order={2}>Verification</Title>

      {isSuccess ? (
        <>
          <Alert icon={<IconCheck size={16} />} title="Identity Verified" color="green">
            Your identity has been verified
            {search.method ? ` via ${search.method.replace(/_/g, ' ')}` : ''}. Redirecting to
            rooms...
          </Alert>
          <Button component="a" href="/rooms" variant="outline">
            Go to Rooms Now
          </Button>
        </>
      ) : null}

      {isError ? (
        <>
          <Alert icon={<IconAlertTriangle size={16} />} title="Verification Failed" color="red">
            {search.message
              ? decodeURIComponent(search.message)
              : 'An unknown error occurred during verification.'}
          </Alert>
          <Button
            onClick={() => {
              const url = buildVerifierUrl('');
              if (url) {
                window.location.href = url;
              }
            }}
            variant="outline"
          >
            Try Again
          </Button>
        </>
      ) : null}

      <Text size="xs" c="dimmed" ta="center">
        Having trouble? Try signing out and signing back in.
      </Text>
    </Stack>
  );
}
