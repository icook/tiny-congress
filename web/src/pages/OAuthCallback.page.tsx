import { IconAlertTriangle, IconCircleCheck, IconLock } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { Button, Center, Container, Loader, Paper, Stack, Text, Title } from '@mantine/core';
import { useAuth } from '../auth/AuthProvider';
import type { OAuthProvider } from '../auth/types';

export function OAuthCallbackPage() {
  const { completeOAuth, status, error } = useAuth();
  const navigate = useNavigate();
  const search = useRouterState({ select: (state) => state.location.search });
  const typedSearch = search as Record<string, unknown>;

  const code = typeof typedSearch.code === 'string' ? typedSearch.code : null;
  const stateParam = typeof typedSearch.state === 'string' ? typedSearch.state : undefined;
  const providerParam =
    typeof typedSearch.provider === 'string' && isKnownProvider(typedSearch.provider)
      ? typedSearch.provider
      : undefined;
  const nextFromQuery = typeof typedSearch.next === 'string' ? typedSearch.next : undefined;

  const [localError, setLocalError] = useState<string | null>(null);
  const [completed, setCompleted] = useState(false);

  useEffect(() => {
    if (!code && status === 'authenticated') {
      navigate({ to: nextFromQuery ?? '/dashboard', replace: true });
    }
  }, [code, status, nextFromQuery, navigate]);

  useEffect(() => {
    if (!code) {
      setLocalError('Missing OAuth code in the callback URL. Please restart sign-in.');
      return;
    }

    completeOAuth({ code, state: stateParam, provider: providerParam })
      .then(({ nextPath }) => {
        setCompleted(true);
        const target = nextFromQuery ?? nextPath ?? '/dashboard';
        navigate({ to: target, replace: true });
      })
      .catch((err) => {
        const message = err instanceof Error ? err.message : 'Unable to finish sign-in.';
        setLocalError(message);
      });
  }, [code, stateParam, providerParam, nextFromQuery, completeOAuth, navigate]);

  const showError = localError ?? error;
  const isLoading = !showError && !completed;

  return (
    <Container size="sm" py="xl">
      <Paper radius="lg" withBorder shadow="md" p="xl">
        <Stack gap="md" align="center">
          <Center
            w={70}
            h={70}
            bg="var(--mantine-color-body)"
            style={{ borderRadius: '50%' }}
          >
            {showError ? (
              <IconAlertTriangle size={32} color="var(--mantine-color-red-6)" />
            ) : isLoading ? (
              <Loader size="sm" />
            ) : (
              <IconCircleCheck size={32} color="var(--mantine-color-green-6)" />
            )}
          </Center>

          <Title order={2}>
            {showError ? 'Sign-in failed' : completed ? 'Signed in' : 'Finishing sign-in'}
          </Title>
          <Text c="dimmed" ta="center">
            {showError
              ? showError
              : 'Exchanging your OAuth code for a secure TinyCongress session.'}
          </Text>

          <Stack gap="xs" align="center">
            <Text size="sm" c="dimmed">
              <IconLock size={14} style={{ marginRight: 4 }} />
              We never see your password; the provider returns a token we use for this session.
            </Text>
            {showError ? (
              <Button variant="light" onClick={() => navigate({ to: '/login', replace: true })}>
                Back to login
              </Button>
            ) : (
              <Text size="sm" c="dimmed">
                You will be redirected automatically once the session is ready.
              </Text>
            )}
          </Stack>
        </Stack>
      </Paper>
    </Container>
  );
}

function isKnownProvider(provider: string): provider is OAuthProvider {
  return provider === 'github' || provider === 'google';
}
