import { useEffect, useState } from 'react';
import { IconAlertTriangle, IconInfoCircle } from '@tabler/icons-react';
import { Alert, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
import { BuildInfo, fetchBuildInfo } from '../api/buildInfo';

type LoadState =
  | { status: 'loading' }
  | { status: 'loaded'; data: BuildInfo }
  | { status: 'error'; message: string };

export function AboutPage() {
  const [state, setState] = useState<LoadState>({ status: 'loading' });

  useEffect(() => {
    let active = true;

    fetchBuildInfo()
      .then((data) => {
        if (active) {
          setState({ status: 'loaded', data });
        }
      })
      .catch((error: Error) => {
        if (active) {
          setState({
            status: 'error',
            message: error.message || 'Unable to load build metadata',
          });
        }
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <Stack gap="md">
      <Group gap="xs">
        <IconInfoCircle size={20} />
        <Title order={2}>About TinyCongress</Title>
      </Group>

      <Text c="dimmed" size="sm">
        Build metadata comes directly from the API via GraphQL, ensuring the UI reflects the running
        backend revision.
      </Text>

      {state.status === 'loading' && (
        <Card shadow="sm" padding="lg" radius="md" withBorder data-testid="build-info-loading">
          <Group gap="sm">
            <Loader size="sm" />
            <Text>Loading build information…</Text>
          </Group>
        </Card>
      )}

      {state.status === 'error' && (
        <Alert
          icon={<IconAlertTriangle size={16} />}
          title="Unable to load build info"
          color="red"
          data-testid="build-info-error"
        >
          {state.message}
        </Alert>
      )}

      {state.status === 'loaded' && (
        <Card shadow="sm" padding="lg" radius="md" withBorder>
          <Stack gap="sm">
            <Metric label="API Version" value={state.data.version} testId="api-version" />
            <Metric label="Git SHA" value={state.data.gitSha} testId="api-git-sha" />
            <Metric label="Build time" value={state.data.buildTime} testId="api-build-time" />
            {state.data.message && (
              <Metric label="Message" value={state.data.message} testId="api-build-message" />
            )}
          </Stack>
        </Card>
      )}
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
