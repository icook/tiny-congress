import { render, screen } from '@test-utils';
import { ErrorBoundary } from './ErrorBoundary';

// Component that throws an error
function ThrowError({ message = 'Test error' }: { message?: string }) {
  throw new Error(message);
}

// Suppress console.error in tests to avoid cluttering test output
const consoleError = console.error;
beforeAll(() => {
  console.error = vi.fn();
});
afterAll(() => {
  console.error = consoleError;
});

describe('ErrorBoundary', () => {
  it('renders children when no error occurs', () => {
    render(
      <ErrorBoundary context="Test">
        <div>Child content</div>
      </ErrorBoundary>
    );

    expect(screen.getByText('Child content')).toBeInTheDocument();
  });

  it('displays error fallback when child component throws', () => {
    render(
      <ErrorBoundary context="Test">
        <ThrowError />
      </ErrorBoundary>
    );

    expect(screen.getByText(/something went wrong/i)).toBeInTheDocument();
    expect(screen.getByText(/test/i)).toBeInTheDocument();
  });

  it('includes context in error message', () => {
    render(
      <ErrorBoundary context="Dashboard">
        <ThrowError />
      </ErrorBoundary>
    );

    expect(screen.getByText(/dashboard/i)).toBeInTheDocument();
  });

  it('displays reload button', () => {
    render(
      <ErrorBoundary context="Test">
        <ThrowError />
      </ErrorBoundary>
    );

    expect(screen.getByRole('button', { name: /reload page/i })).toBeInTheDocument();
  });

  it('calls onError callback when error occurs', () => {
    const onError = vi.fn();

    render(
      <ErrorBoundary context="Test" onError={onError}>
        <ThrowError message="Custom error" />
      </ErrorBoundary>
    );

    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'Custom error' }),
      expect.objectContaining({ componentStack: expect.any(String) })
    );
  });

  it('renders custom fallback when provided', () => {
    const customFallback = <div>Custom error UI</div>;

    render(
      <ErrorBoundary context="Test" fallback={customFallback}>
        <ThrowError />
      </ErrorBoundary>
    );

    expect(screen.getByText('Custom error UI')).toBeInTheDocument();
    expect(screen.queryByText(/something went wrong/i)).not.toBeInTheDocument();
  });
});
