import { IconAlertTriangle, IconInfoCircle } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { Alert, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
import { buildInfoQuery } from '../api/queries';

export function AboutPage() {
  const { data, isPending, isError, error } = useQuery(buildInfoQuery);

  return (
    <Stack gap="md">
      <Group gap="xs">
        <IconInfoCircle size={20} />
        <Title order={2}>About TinyCongress</Title>
      </Group>

      <Text c="dimmed" size="sm">
        Build metadata for both the API (fetched via GraphQL) and the UI (baked in at build time),
        so operators can verify exactly which revisions are running.
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
            <Metric label="Build time" value={data.buildTime} testId="api-build-time" />
            {data.message ? (
              <Metric label="Message" value={data.message} testId="api-build-message" />
            ) : null}
          </Stack>
        </Card>
      ) : null}

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="sm">
          <Metric label="UI Git SHA" value={__UI_GIT_SHA__} testId="ui-git-sha" />
          <Metric label="UI Build time" value={__UI_BUILD_TIME__} testId="ui-build-time" />
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
