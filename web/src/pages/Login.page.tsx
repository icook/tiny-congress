import {
  IconArrowRight,
  IconBrandGithub,
  IconBrandGoogle,
  IconKey,
  IconLock,
  IconUsers,
} from '@tabler/icons-react';
import { useRouterState } from '@tanstack/react-router';
import {
  Badge,
  Button,
  Card,
  Container,
  Flex,
  Group,
  List,
  Loader,
  Paper,
  Stack,
  Text,
  Title,
} from '@mantine/core';
import { useAuth } from '../auth/AuthProvider';
import type { OAuthProvider } from '../auth/types';

const providers: { id: OAuthProvider; label: string; description: string }[] = [
  {
    id: 'github',
    label: 'Continue with GitHub',
    description: 'Use your GitHub identity to enter the workspace.',
  },
  {
    id: 'google',
    label: 'Continue with Google',
    description: 'Use your Google account to join TinyCongress.',
  },
];

export function LoginPage() {
  const { loginWithProvider, status, error } = useAuth();
  const { search } = useRouterState({ select: (state) => state.location });
  const nextPath = typeof search.next === 'string' && search.next ? search.next : '/dashboard';

  const authenticating = status === 'authenticating';

  const handleLogin = (provider: OAuthProvider) => {
    loginWithProvider(provider, nextPath);
  };

  return (
    <Container size="xl" py="xl">
      <Paper
        radius="lg"
        shadow="md"
        p={{ base: 'lg', md: 'xl' }}
        withBorder
        style={{
          background:
            'linear-gradient(145deg, rgba(50, 115, 220, 0.08) 0%, rgba(247, 148, 29, 0.08) 100%)',
        }}
      >
        <Stack gap="xl">
          <Group justify="space-between" align="flex-start">
            <Stack gap={6}>
              <Badge variant="light" color="indigo" leftSection={<IconLock size={14} />}>
                Secure OAuth
              </Badge>
              <Title order={1}>Sign in with a trusted provider</Title>
              <Text size="lg" c="dimmed" maw={640}>
                Pick the identity provider your team already uses. We will redirect you to complete
                authentication and bring you straight back to TinyCongress.
              </Text>
            </Stack>
            {authenticating && (
              <Group gap={6} c="dimmed">
                <Loader size="sm" />
                <Text size="sm">Redirecting to your provider...</Text>
              </Group>
            )}
          </Group>

          <Flex direction={{ base: 'column', md: 'row' }} gap="lg" align="stretch">
            <Card withBorder radius="md" shadow="sm" p="lg" miw={320} style={{ flex: 1 }}>
              <Stack gap="md">
                {providers.map((provider) => (
                  <Button
                    key={provider.id}
                    leftSection={
                      provider.id === 'github' ? (
                        <IconBrandGithub size={18} />
                      ) : (
                        <IconBrandGoogle size={18} />
                      )
                    }
                    variant={provider.id === 'github' ? 'filled' : 'outline'}
                    color={provider.id === 'github' ? 'dark' : 'indigo'}
                    radius="md"
                    size="md"
                    onClick={() => handleLogin(provider.id)}
                    disabled={authenticating}
                  >
                    {provider.label}
                  </Button>
                ))}

                {error && (
                  <Text size="sm" c="red" mt={-6}>
                    {error}
                  </Text>
                )}

                <Stack gap={4}>
                  <Group gap={6}>
                    <IconArrowRight size={16} color="var(--mantine-color-dimmed)" />
                    <Text size="sm" fw={600}>
                      What happens next
                    </Text>
                  </Group>
                  <Text size="sm" c="dimmed">
                    You will be redirected to your provider to approve access, then returned here
                    with a secure session.
                  </Text>
                </Stack>
              </Stack>
            </Card>

            <Card radius="md" shadow="sm" p="lg" withBorder style={{ flex: 1 }}>
              <Stack gap="md">
                <Group gap={8}>
                  <IconUsers size={20} />
                  <Text fw={600}>Why OAuth?</Text>
                </Group>

                <List spacing="sm" icon={<IconArrowRight size={16} stroke={1.5} />}>
                  <List.Item icon={<IconLock size={16} stroke={1.5} />}>
                    No passwords stored here—authentication stays with your provider.
                  </List.Item>
                  <List.Item icon={<IconKey size={16} stroke={1.5} />}>
                    Fine-grained consent so we only receive basic profile details.
                  </List.Item>
                  <List.Item icon={<IconArrowRight size={16} stroke={1.5} />}>
                    Automatic sign-out on token revocation keeps shared devices secure.
                  </List.Item>
                </List>

                <Text size="sm" c="dimmed">
                  Need a different provider? Reach out to your admin to enable additional OAuth
                  issuers.
                </Text>
              </Stack>
            </Card>
          </Flex>
        </Stack>
      </Paper>
    </Container>
  );
}
