# Frontend (React) Error Handling

React-specific error handling patterns for the web client. For general error codes and concepts, see [Error Handling Patterns](./error-handling.md).

## Error Boundary Strategy

Place error boundaries strategically to isolate failures:

```tsx
// App.tsx - Layered error boundaries
<ErrorBoundary context="Application">
  <QueryProvider>
    <MantineProvider>
      <ErrorBoundary context="Router">
        <Router />
      </ErrorBoundary>
    </MantineProvider>
  </QueryProvider>
</ErrorBoundary>
```

### Placement Guidelines

| Location | Purpose | Granularity |
|----------|---------|-------------|
| App root | Last line of defense | Coarse |
| Router | Isolate route failures | Medium |
| Feature sections | Isolate complex features | Medium |
| Third-party widgets | Isolate untrusted code | Fine |

```tsx
// Good: Strategic boundaries
<ErrorBoundary context="Dashboard">
  <DashboardHeader />
  <ErrorBoundary context="Charts">
    <ChartsSection />  {/* Third-party charting library */}
  </ErrorBoundary>
  <DashboardFooter />
</ErrorBoundary>

// Bad: Too coarse
<ErrorBoundary>
  <EntireApp />  {/* One error breaks everything */}
</ErrorBoundary>

// Bad: Too fine
<ErrorBoundary>
  <Button />  {/* Unnecessary overhead */}
</ErrorBoundary>
```

## ErrorBoundary Component

```tsx
// components/ErrorBoundary/ErrorBoundary.tsx
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { ErrorFallback } from './ErrorFallback';

interface ErrorBoundaryProps {
  children: ReactNode;
  context?: string;       // Identifies error location
  fallback?: ReactNode;   // Custom fallback UI
  onError?: (error: Error, info: ErrorInfo) => void;
}

export function ErrorBoundary({
  children,
  context = 'Application',
  fallback,
  onError,
}: ErrorBoundaryProps) {
  const handleError = (error: Error, info: ErrorInfo) => {
    if (import.meta.env.DEV) {
      console.error(`[ErrorBoundary:${context}]`, error);
      console.error('Component stack:', info.componentStack);
    }

    // Production: Send to error tracking
    // Sentry.captureException(error, { extra: { context, componentStack } });

    onError?.(error, info);
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
```

## ErrorFallback Component

```tsx
// components/ErrorBoundary/ErrorFallback.tsx
export function ErrorFallback({ context = 'Application', error }: ErrorFallbackProps) {
  return (
    <Container size="sm" py="xl">
      <Stack gap="lg">
        <Alert icon={<IconAlertCircle />} title="Something went wrong" color="red">
          An unexpected error occurred in the {context}. Please try reloading.
        </Alert>

        <Button leftSection={<IconRefresh />} onClick={() => window.location.reload()}>
          Reload Page
        </Button>

        {import.meta.env.DEV && error && (
          <Alert color="gray" variant="outline">
            <Text size="xs" ff="monospace">{error.message}</Text>
          </Alert>
        )}
      </Stack>
    </Container>
  );
}
```

## Network Error Handling

### TanStack Query Errors

```tsx
function UserList() {
  const { data, error, isError, isLoading, refetch } = useQuery({
    queryKey: ['users'],
    queryFn: fetchUsers,
    retry: 3,  // Automatic retry
    retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 30000),
  });

  if (isLoading) return <Skeleton />;

  if (isError) {
    return (
      <Alert color="red" title="Failed to load users">
        {error.message}
        <Button onClick={() => refetch()} mt="sm">Retry</Button>
      </Alert>
    );
  }

  return <div>{data.map(user => <UserCard key={user.id} {...user} />)}</div>;
}
```

### Mutation Errors

```tsx
function CreateUserForm() {
  const mutation = useMutation({
    mutationFn: createUser,
    onError: (error) => {
      // Show toast notification
      notifications.show({
        title: 'Failed to create user',
        message: error.message,
        color: 'red',
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      notifications.show({
        title: 'User created',
        message: 'The new user has been added.',
        color: 'green',
      });
    },
  });

  return (
    <form onSubmit={(e) => { e.preventDefault(); mutation.mutate(formData); }}>
      {mutation.isError && (
        <Alert color="red" mb="md">{mutation.error.message}</Alert>
      )}
      {/* form fields */}
      <Button type="submit" loading={mutation.isPending}>Create</Button>
    </form>
  );
}
```

## Form Validation Errors

Use Mantine's form validation with clear error messages:

```tsx
import { useForm } from '@mantine/form';

function SignupForm() {
  const form = useForm({
    initialValues: { username: '', email: '' },
    validate: {
      username: (value) => {
        if (!value) return 'Username is required';
        if (value.length < 3) return 'Username must be at least 3 characters';
        if (value.length > 64) return 'Username must be at most 64 characters';
        return null;
      },
      email: (value) => /^\S+@\S+$/.test(value) ? null : 'Invalid email',
    },
  });

  return (
    <form onSubmit={form.onSubmit(handleSubmit)}>
      <TextInput
        label="Username"
        {...form.getInputProps('username')}
        error={form.errors.username}  // Shows validation error
      />
      <TextInput
        label="Email"
        {...form.getInputProps('email')}
        error={form.errors.email}
      />
    </form>
  );
}
```

## Toast/Notification Patterns

Use Mantine's notification system for transient feedback:

```tsx
import { notifications } from '@mantine/notifications';

// Success notification
notifications.show({
  title: 'Success',
  message: 'Your changes have been saved',
  color: 'green',
  icon: <IconCheck />,
});

// Error notification
notifications.show({
  title: 'Error',
  message: 'Failed to save changes. Please try again.',
  color: 'red',
  icon: <IconX />,
  autoClose: false,  // Keep visible for errors
});

// With action
notifications.show({
  title: 'Connection lost',
  message: 'Attempting to reconnect...',
  color: 'yellow',
  loading: true,
});
```

## Error Recovery Options

Provide users with clear recovery paths:

| Error Type | Recovery Option |
|------------|-----------------|
| Network error | Retry button, offline indicator |
| Authentication | Redirect to login |
| Permission denied | Request access link |
| Not found | Back button, search |
| Validation | Inline field errors |
| Server error | Reload page, contact support |

## Testing Error Handling

```tsx
// Suppress console.error in error boundary tests
const consoleError = console.error;
beforeAll(() => { console.error = vi.fn(); });
afterAll(() => { console.error = consoleError; });

function ThrowError() {
  throw new Error('Test error');
}

test('displays fallback on error', () => {
  render(
    <ErrorBoundary context="Test">
      <ThrowError />
    </ErrorBoundary>
  );
  expect(screen.getByText(/something went wrong/i)).toBeInTheDocument();
});

test('shows retry button on query error', async () => {
  server.use(
    rest.get('/api/users', (req, res, ctx) => res(ctx.status(500)))
  );

  render(<UserList />);
  await screen.findByText(/failed to load/i);
  expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
});
```

---

## See Also

- [Error Handling Patterns](./error-handling.md) - Overview and standard error codes
- [Backend Error Handling](./error-handling-backend.md) - Rust error types and HTTP responses
- [React Coding Standards](./react-coding-standards.md) - General React conventions
