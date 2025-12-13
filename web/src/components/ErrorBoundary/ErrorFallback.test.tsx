import { render, screen } from '@test-utils';
import { ErrorFallback } from './ErrorFallback';

describe('ErrorFallback', () => {
  it('renders error message with default context', () => {
    render(<ErrorFallback />);

    expect(screen.getByText(/something went wrong/i)).toBeInTheDocument();
    expect(screen.getByText(/application/i)).toBeInTheDocument();
  });

  it('renders error message with custom context', () => {
    render(<ErrorFallback context="Dashboard" />);

    expect(screen.getByText(/dashboard/i)).toBeInTheDocument();
  });

  it('renders reload button', () => {
    render(<ErrorFallback />);

    const reloadButton = screen.getByRole('button', { name: /reload page/i });
    expect(reloadButton).toBeInTheDocument();
  });

  it('reloads page when reload button is clicked', () => {
    const reloadSpy = vi.fn();
    Object.defineProperty(window, 'location', {
      value: { reload: reloadSpy },
      writable: true,
    });

    render(<ErrorFallback />);

    const reloadButton = screen.getByRole('button', { name: /reload page/i });
    reloadButton.click();

    expect(reloadSpy).toHaveBeenCalled();
  });

  it('does not display error details in production', () => {
    const originalEnv = import.meta.env.DEV;
    import.meta.env.DEV = false;

    const error = new Error('Test error message');

    render(<ErrorFallback error={error} />);

    expect(screen.queryByText(/error details/i)).not.toBeInTheDocument();

    import.meta.env.DEV = originalEnv;
  });

  it('displays error details in development mode', () => {
    const originalEnv = import.meta.env.DEV;
    import.meta.env.DEV = true;

    const error = new Error('Test error message');
    error.stack = 'Error stack trace';

    render(<ErrorFallback error={error} />);

    expect(screen.getByText(/error details/i)).toBeInTheDocument();
    expect(screen.getByText(/test error message/i)).toBeInTheDocument();

    import.meta.env.DEV = originalEnv;
  });
});
