import { Text } from '@mantine/core';
import { ErrorBoundary } from './ErrorBoundary';

export default { title: 'Components/ErrorBoundary' };

function ThrowingChild(): React.ReactNode {
  throw new Error('Story: intentional throw to demo ErrorBoundary');
}

export const Idle = () => (
  <ErrorBoundary>
    <Text>This content renders normally inside the boundary.</Text>
  </ErrorBoundary>
);

export const Triggered = () => (
  <ErrorBoundary context="Story">
    <ThrowingChild />
  </ErrorBoundary>
);
