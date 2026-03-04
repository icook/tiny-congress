import { IconChartBar, IconDoor, IconUserPlus } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { Button, Container, Group, SimpleGrid, Stack, Text, ThemeIcon, Title } from '@mantine/core';
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
    title: 'Enter a room',
    description: 'Join a topic room where decisions are being made.',
  },
  {
    icon: IconChartBar,
    title: 'Vote & see results',
    description: 'Cast multi-dimensional votes and see how the group thinks.',
  },
];

export function Welcome() {
  const { deviceKid } = useDevice();
  const isLoggedIn = deviceKid !== null;

  return (
    <Container size="md">
      <Stack align="center" gap="xl" mt={100}>
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
          Verified people vote on issues that matter, with more nuance than yes/no.
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

        <Group mt="xl">
          {isLoggedIn ? (
            <>
              <Button component={Link} to="/rooms" size="lg">
                Browse Rooms
              </Button>
              <Button component={Link} to="/settings" size="lg" variant="outline">
                Go to Settings
              </Button>
            </>
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
