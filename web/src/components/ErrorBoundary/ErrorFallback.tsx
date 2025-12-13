import { Alert, Button, Container, Stack, Text, Title } from '@mantine/core';
import { IconAlertCircle, IconRefresh } from '@tabler/icons-react';

interface ErrorFallbackProps {
  /**
   * Context identifier for the error (e.g., 'Router', 'QueryProvider')
   */
  context?: string;
  /**
   * Optional error object to display details
   */
  error?: Error;
}

/**
 * Default fallback UI component displayed when an error boundary catches an error.
 *
 * Provides:
 * - User-friendly error message
 * - Reload button to recover
 * - Error details in development mode
 *
 * @see docs/interfaces/react-coding-standards.md
 */
export function ErrorFallback({ context = 'Application', error }: ErrorFallbackProps) {
  const handleReload = () => {
    window.location.reload();
  };

  return (
    <Container size="sm" py="xl">
      <Stack gap="lg">
        <Alert
          icon={<IconAlertCircle size={24} />}
          title="Something went wrong"
          color="red"
          variant="light"
        >
          <Text size="sm">
            An unexpected error occurred in the {context}. Please try reloading the page.
          </Text>
        </Alert>

        <Button
          leftSection={<IconRefresh size={18} />}
          onClick={handleReload}
          variant="light"
          color="blue"
        >
          Reload Page
        </Button>

        {import.meta.env.DEV && error && (
          <Stack gap="xs">
            <Title order={4}>Error Details (Development Only)</Title>
            <Alert color="gray" variant="outline">
              <Text size="xs" ff="monospace">
                <strong>Message:</strong> {error.message}
              </Text>
              {error.stack && (
                <Text size="xs" ff="monospace" mt="xs" style={{ whiteSpace: 'pre-wrap' }}>
                  <strong>Stack:</strong>
                  {'\n'}
                  {error.stack}
                </Text>
              )}
            </Alert>
          </Stack>
        )}
      </Stack>
    </Container>
  );
}
