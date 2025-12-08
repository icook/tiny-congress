import { ReactNode, useEffect } from 'react';
import { Center, Loader, Stack, Text } from '@mantine/core';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { useAuth } from './AuthProvider';

export function AuthGate({ children }: { children: ReactNode }) {
  const { status } = useAuth();
  const navigate = useNavigate();
  const location = useRouterState({ select: (state) => state.location });

  useEffect(() => {
    if (status !== 'unauthenticated' && status !== 'error') {
      return;
    }

    const searchString = stringifySearch(location.search);
    const next = searchString ? `${location.pathname}?${searchString}` : location.pathname;

    navigate({
      to: '/login',
      search: { next },
      replace: true,
    });
  }, [status, location.pathname, location.search, navigate]);

  if (status === 'authenticated') {
    return <>{children}</>;
  }

  if (status === 'error') {
    return (
      <Center h="60vh">
        <Stack gap="xs" align="center">
          <Text fw={600}>Sign-in required</Text>
          <Text c="dimmed" size="sm">
            Redirecting you to the login screen.
          </Text>
        </Stack>
      </Center>
    );
  }

  return (
    <Center h="60vh">
      <Loader size="md" />
    </Center>
  );
}

function stringifySearch(search: Record<string, unknown> | undefined) {
  const params = new URLSearchParams();

  Object.entries(search ?? {}).forEach(([key, value]) => {
    if (value === undefined || value === null) {
      return;
    }
    params.set(key, String(value));
  });

  return params.toString();
}
