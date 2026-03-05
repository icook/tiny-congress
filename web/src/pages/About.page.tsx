import { IconAlertTriangle, IconInfoCircle } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { Alert, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
import { buildInfoQuery } from '../api/queries';
import { TimestampText } from '../components/TimestampText';

export function AboutPage() {
  const { data, isPending, isError, error } = useQuery(buildInfoQuery);

  return (
    <Stack gap="md">
      <Group gap="xs">
        <IconInfoCircle size={20} />
        <Title order={2}>About TinyCongress</Title>
      </Group>

      <Text size="sm">
        TinyCongress is an experimental platform for structured group decision-making. Instead of
        simple yes/no polls, participants vote across multiple dimensions — capturing the nuance of
        how people actually think about complex issues.
      </Text>

      <Text size="sm">
        Every account is backed by cryptographic identity. Your keys are generated in your browser
        and never leave your device. The server is a witness, not a trusted authority.
      </Text>

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="xs">
          <Title order={5}>How it works</Title>
          <Text size="sm" c="dimmed">
            1. Create an account with a username and backup password
          </Text>
          <Text size="sm" c="dimmed">
            2. Verify your identity to prove you&apos;re a real person
          </Text>
          <Text size="sm" c="dimmed">
            3. Join a room and vote on multi-dimensional polls
          </Text>
          <Text size="sm" c="dimmed">
            4. See how the community thinks — not just averages, but the shape of opinion
          </Text>
        </Stack>
      </Card>

      <Title order={4} mt="md">
        Build Info
      </Title>
      <Text c="dimmed" size="xs">
        Revision metadata for the API and UI.
      </Text>

      {isPending ? (
        <Card shadow="sm" padding="lg" radius="md" withBorder data-testid="build-info-loading">
          <Group gap="sm">
            <Loader size="sm" />
            <Text>Loading build information…</Text>
          </Group>
        </Card>
      ) : null}

      {isError ? (
        <Alert
          icon={<IconAlertTriangle size={16} />}
          title="Unable to load build info"
          color="red"
          data-testid="build-info-error"
        >
          {error instanceof Error ? error.message : 'Unable to load build metadata'}
        </Alert>
      ) : null}

      {data ? (
        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="sm">
            <Metric label="API Version" value={data.version} testId="api-version" />
            <Metric label="Git SHA" value={data.gitSha} testId="api-git-sha" />
            <TimestampMetric label="Build time" value={data.buildTime} testId="api-build-time" />
            {data.message ? (
              <Metric label="Message" value={data.message} testId="api-build-message" />
            ) : null}
          </Stack>
        </Card>
      ) : null}

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="sm">
          <Metric label="UI Git SHA" value={__UI_GIT_SHA__} testId="ui-git-sha" />
          <TimestampMetric label="UI Build time" value={__UI_BUILD_TIME__} testId="ui-build-time" />
        </Stack>
      </Card>
    </Stack>
  );
}

function Metric({ label, value, testId }: { label: string; value: string; testId: string }) {
  return (
    <Group gap="xs" wrap="nowrap">
      <Text fw={500} w={120}>
        {label}
      </Text>
      <Text data-testid={testId}>{value}</Text>
    </Group>
  );
}

function TimestampMetric({
  label,
  value,
  testId,
}: {
  label: string;
  value: string;
  testId: string;
}) {
  return (
    <Group gap="xs" wrap="nowrap">
      <Text fw={500} w={120}>
        {label}
      </Text>
      <TimestampText value={value} data-testid={testId} />
    </Group>
  );
}
