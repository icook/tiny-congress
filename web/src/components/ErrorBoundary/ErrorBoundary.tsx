import { type ErrorInfo, type ReactNode } from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { ErrorFallback } from './ErrorFallback';

interface ErrorBoundaryProps {
  children: ReactNode;
  /**
   * Optional custom fallback component
   */
  fallback?: ReactNode;
  /**
   * Optional callback when error occurs
   */
  onError?: (error: Error, info: ErrorInfo) => void;
  /**
   * Context identifier for error logging (e.g., 'Router', 'QueryProvider', 'Dashboard')
   */
  context?: string;
}

/**
 * ErrorBoundary wrapper for the react-error-boundary library.
 *
 * Catches JavaScript errors anywhere in the child component tree,
 * logs those errors, and displays a fallback UI.
 *
 * @example
 * ```tsx
 * <ErrorBoundary context="Dashboard">
 *   <DashboardPage />
 * </ErrorBoundary>
 * ```
 *
 * @see docs/interfaces/react-coding-standards.md for standards and best practices
 */
export function ErrorBoundary({
  children,
  fallback,
  onError,
  context = 'Application',
}: ErrorBoundaryProps) {
  const handleError = (error: Error, info: ErrorInfo) => {
    // Log to console in development
    if (import.meta.env.DEV) {
      // eslint-disable-next-line no-console
      console.error(`[ErrorBoundary:${context}] Error caught:`, error);
      // eslint-disable-next-line no-console
      console.error('Component stack:', info.componentStack);
    }

    // Call custom error handler if provided
    onError?.(error, info);

    // TODO: Send to error tracking service (e.g., Sentry, LogRocket)
    // errorTrackingService.captureException(error, {
    //   context,
    //   componentStack: info.componentStack,
    // });
  };

  return (
    <ReactErrorBoundary
      fallback={fallback ?? <ErrorFallback context={context} />}
      onError={handleError}
    >
      {children}
    </ReactErrorBoundary>
  );
}
