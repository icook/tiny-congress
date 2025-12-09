import { IconBrandGithub, IconBrandGoogle, IconLock } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Badge, Button, Group, Stack, Text, Title } from '@mantine/core';
import { useAuth } from '../../auth/AuthProvider';
import classes from './Welcome.module.css';

export function Welcome() {
  const { status, user } = useAuth();
  const isAuthenticated = status === 'authenticated';

  return (
    <Stack gap="md" align="center" mt={80}>
      <Title className={classes.title} ta="center">
        Welcome to{' '}
        <Text
          inherit
          variant="gradient"
          component="span"
          gradient={{ from: 'indigo', to: 'violet' }}
        >
          TinyCongress
        </Text>
      </Title>
      <Text c="dimmed" ta="center" size="lg" maw={640}>
        Join the workspace with OAuth—no new passwords to remember, just a quick hand-off to your
        trusted provider and you are ready to collaborate.
      </Text>

      <Group justify="center" gap="md" mt="md">
        <Button
          size="md"
          radius="md"
          leftSection={<IconBrandGithub size={16} />}
          component={Link}
          to="/login"
        >
          Sign in with OAuth
        </Button>
        <Button
          size="md"
          variant="outline"
          radius="md"
          leftSection={<IconBrandGoogle size={16} />}
          component={Link}
          to="/dashboard"
          disabled={!isAuthenticated}
        >
          Go to dashboard
        </Button>
      </Group>

      <Group gap="xs" justify="center">
        <Badge variant="light" color="green" leftSection={<IconLock size={12} />}>
          SSO protected
        </Badge>
        {isAuthenticated && user ? (
          <Badge variant="light" color="indigo">
            Signed in as {user.name}
          </Badge>
        ) : (
          <Text size="sm" c="dimmed">
            Prefer another provider? Start with GitHub or Google and switch later.
          </Text>
        )}
      </Group>
    </Stack>
  );
}
