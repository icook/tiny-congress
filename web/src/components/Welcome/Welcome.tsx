import { IconChartBar, IconDoor, IconUserPlus } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import {
  Alert,
  Button,
  Container,
  Group,
  SimpleGrid,
  Stack,
  Text,
  ThemeIcon,
  Title,
} from '@mantine/core';
import { buildVerifierUrl, useVerificationStatus } from '../../features/verification';
import { useCrypto } from '../../providers/CryptoProvider';
import { useDevice } from '../../providers/DeviceProvider';
import classes from './Welcome.module.css';

// CSS module kept for responsive hero sizing that Mantine props alone cannot match; see ADR 0001.

const steps = [
  {
    icon: IconUserPlus,
    title: 'Sign up',
    description: 'Create a cryptographic identity in seconds — no email required.',
  },
  {
    icon: IconDoor,
    title: 'Find a poll',
    description: 'Browse topic rooms and find a question to weigh in on.',
  },
  {
    icon: IconChartBar,
    title: 'Vote on a spectrum',
    description: 'Move sliders to express nuanced positions — not just yes or no.',
  },
];

export function Welcome() {
  const { deviceKid, privateKey, username } = useDevice();
  const { crypto } = useCrypto();
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const isLoggedIn = deviceKid !== null;
  const isVerified = verificationQuery.data?.isVerified ?? false;

  return (
    <Container size="md">
      <Stack align="center" gap="xl" mt={100}>
        {isLoggedIn && username ? (
          <Text c="dimmed" size="lg">
            Welcome back, <strong>{username}</strong>.
          </Text>
        ) : null}

        <Title className={classes.title} ta="center">
          <Text
            inherit
            variant="gradient"
            component="span"
            gradient={{ from: 'pink', to: 'yellow' }}
          >
            TinyCongress
          </Text>
        </Title>

        <Text c="dimmed" ta="center" size="lg" maw={580}>
          Vote on a spectrum, not just yes or no. Verified people weigh in on issues that matter
          with nuance.
        </Text>

        <SimpleGrid cols={{ base: 1, sm: 3 }} spacing="xl" mt="xl">
          {steps.map((step) => (
            <Stack key={step.title} align="center" gap="xs">
              <ThemeIcon size={48} radius="md" variant="light">
                <step.icon size={28} />
              </ThemeIcon>
              <Text fw={600}>{step.title}</Text>
              <Text c="dimmed" size="sm" ta="center">
                {step.description}
              </Text>
            </Stack>
          ))}
        </SimpleGrid>

        {isLoggedIn && !isVerified && !verificationQuery.isLoading
          ? (() => {
              const url = buildVerifierUrl(username ?? '');
              return url ? (
                <Alert color="yellow" maw={480} w="100%">
                  Your identity isn&apos;t verified yet — you won&apos;t be able to vote until you
                  complete verification.{' '}
                  <a href={url} style={{ fontWeight: 600 }}>
                    Verify now →
                  </a>
                </Alert>
              ) : null;
            })()
          : null}

        <Group mt="xl">
          {isLoggedIn ? (
            <Button component={Link} to="/rooms" size="lg">
              Browse Rooms
            </Button>
          ) : (
            <>
              <Button component={Link} to="/signup" size="lg">
                Sign Up
              </Button>
              <Button component={Link} to="/rooms" size="lg" variant="outline">
                Browse Rooms
              </Button>
            </>
          )}
        </Group>
      </Stack>
    </Container>
  );
}
