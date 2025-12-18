# Error Handling Patterns

Comprehensive error handling guidelines for backend (Rust) and frontend (React) code.

## Overview

Error handling should be:
- **Typed**: Use structured error types, not strings
- **Informative**: Provide context for debugging without leaking internals
- **Recoverable**: Allow users to understand and recover from errors
- **Logged**: Capture details for debugging while showing user-friendly messages

## Standard Error Codes

Use consistent error codes across REST and GraphQL APIs:

| Code | HTTP Status | Description | When to Use |
|------|-------------|-------------|-------------|
| `INTERNAL_ERROR` | 500 | Unexpected server error | Database failures, panics, unhandled exceptions |
| `VALIDATION_ERROR` | 400 | Invalid input data | Missing fields, format errors, constraint violations |
| `NOT_FOUND` | 404 | Resource doesn't exist | Entity lookup failures |
| `CONFLICT` | 409 | Resource conflict | Duplicate username, concurrent modification |
| `UNAUTHORIZED` | 401 | Authentication required | Missing or invalid credentials |
| `FORBIDDEN` | 403 | Permission denied | Insufficient privileges |
| `RATE_LIMITED` | 429 | Too many requests | Rate limit exceeded |

### Error Code Format

Use SCREAMING_SNAKE_CASE for error codes:

```
DUPLICATE_USERNAME
INVALID_SIGNATURE
ACCOUNT_NOT_FOUND
```

---

## Backend (Rust)

### Error Type Hierarchy

Organize errors into domain and infrastructure categories:

```
Domain Errors (business logic)
├── AccountError
│   ├── NotFound
│   ├── DuplicateUsername
│   └── InvalidSignature
├── EndorsementError
│   ├── InvalidSignature
│   └── ExpiredTimestamp
└── AuthError
    ├── InvalidToken
    └── Expired

Infrastructure Errors (technical)
├── DatabaseError
├── NetworkError
└── ConfigError
```

### Using `thiserror` for Domain Errors

Define typed errors for each module:

```rust
// service/src/identity/repo/accounts.rs
#[derive(Debug, thiserror::Error)]
pub enum AccountRepoError {
    #[error("username already taken")]
    DuplicateUsername,

    #[error("public key already registered")]
    DuplicateKey,

    #[error("account not found: {0}")]
    NotFound(Uuid),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

**Key principles:**
- Each variant represents a specific failure mode
- Use `#[from]` for automatic conversion from underlying errors
- Include relevant context (IDs, field names) in variants

### HTTP Error Responses

#### REST API: RFC 7807 Problem Details

Use the `ProblemDetails` struct for REST endpoints:

```rust
// service/src/rest.rs
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,    // URI identifying the error type
    pub title: String,           // Short summary
    pub status: u16,             // HTTP status code
    pub detail: String,          // Human-readable explanation
    pub instance: Option<String>, // URI for this occurrence
    pub extensions: Option<ProblemExtensions>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemExtensions {
    pub code: String,            // Standard error code
    pub field: Option<String>,   // Field that caused error (validation)
}
```

Example response:

```json
{
  "type": "https://tinycongress.com/errors/validation",
  "title": "Validation Error",
  "status": 400,
  "detail": "Username must be between 3 and 64 characters",
  "extensions": {
    "code": "VALIDATION_ERROR",
    "field": "username"
  }
}
```

#### Handler Error Mapping

Map domain errors to HTTP responses using `IntoResponse`:

```rust
// service/src/identity/http/mod.rs
impl IntoResponse for AccountRepoError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::DuplicateUsername => (
                StatusCode::CONFLICT,
                Json(ErrorResponse { error: "Username already taken".to_string() }),
            ).into_response(),

            Self::DuplicateKey => (
                StatusCode::CONFLICT,
                Json(ErrorResponse { error: "Public key already registered".to_string() }),
            ).into_response(),

            Self::NotFound(id) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { error: format!("Account not found: {id}") }),
            ).into_response(),

            Self::Database(e) => {
                tracing::error!("Database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: "Internal server error".to_string() }),
                ).into_response()
            }
        }
    }
}
```

**Important:** Never expose internal error details (SQL errors, stack traces) in responses.

### GraphQL Error Responses

For GraphQL, use `async_graphql::Error` with extensions:

```rust
use async_graphql::{Error, ErrorExtensions};

async fn get_account(&self, ctx: &Context<'_>, id: Uuid) -> Result<Account> {
    let account = repo.find(id).await.map_err(|e| match e {
        AccountRepoError::NotFound(_) => Error::new("Account not found")
            .extend_with(|_, e| {
                e.set("code", "NOT_FOUND");
                e.set("id", id.to_string());
            }),
        _ => Error::new("Internal error")
            .extend_with(|_, e| e.set("code", "INTERNAL_ERROR")),
    })?;
    Ok(account)
}
```

Response format:

```json
{
  "data": null,
  "errors": [{
    "message": "Account not found",
    "locations": [{"line": 1, "column": 1}],
    "path": ["getAccount"],
    "extensions": {
      "code": "NOT_FOUND",
      "id": "550e8400-e29b-41d4-a716-446655440000"
    }
  }]
}
```

### Error Propagation

Use the `?` operator with `#[from]` conversions:

```rust
// Good: Clean propagation
async fn create_account(pool: &PgPool, req: CreateRequest) -> Result<Account, AccountError> {
    let validated = validate_request(&req)?;  // ValidationError -> AccountError
    let account = repo.create(&validated).await?;  // sqlx::Error -> AccountError
    Ok(account)
}

// Bad: Manual mapping everywhere
async fn create_account(pool: &PgPool, req: CreateRequest) -> Result<Account, String> {
    let validated = validate_request(&req)
        .map_err(|e| format!("validation failed: {e}"))?;
    // ...
}
```

### Logging vs Returning Errors

| Scenario | Log Level | Return to Client |
|----------|-----------|------------------|
| Validation failure | `debug!` | Full error message |
| Business rule violation | `info!` | User-friendly message |
| Database error | `error!` | Generic "Internal error" |
| External service failure | `warn!` | Retry message or generic error |

```rust
match repo.create(account).await {
    Ok(account) => Ok(Json(account)),
    Err(AccountRepoError::DuplicateUsername) => {
        tracing::debug!("Duplicate username attempt: {}", account.username);
        Err((StatusCode::CONFLICT, "Username already taken"))
    }
    Err(AccountRepoError::Database(e)) => {
        tracing::error!("Database error creating account: {e}");
        Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
    }
}
```

### Structured Error Inspection

Never match errors by parsing string output:

```rust
// Bad: String matching is fragile
if e.to_string().contains("unique constraint") {
    return Err(ApiError::DuplicateUsername);
}

// Good: Use structured error accessors
if let sqlx::Error::Database(db_err) = &e {
    if let Some(constraint) = db_err.constraint() {
        match constraint {
            "accounts_username_key" => return Err(ApiError::DuplicateUsername),
            "accounts_root_kid_key" => return Err(ApiError::DuplicateKey),
            _ => {}
        }
    }
}
```

---

## Frontend (React)

### Error Boundary Strategy

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

#### Placement Guidelines

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

### ErrorBoundary Component

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

### ErrorFallback Component

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

### Network Error Handling

#### TanStack Query Errors

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

#### Mutation Errors

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

### Form Validation Errors

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

### Toast/Notification Patterns

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

### Error Recovery Options

Provide users with clear recovery paths:

| Error Type | Recovery Option |
|------------|-----------------|
| Network error | Retry button, offline indicator |
| Authentication | Redirect to login |
| Permission denied | Request access link |
| Not found | Back button, search |
| Validation | Inline field errors |
| Server error | Reload page, contact support |

---

## Localization Considerations

### Backend

Return error codes, not messages, for client-side translation:

```json
{
  "error": {
    "code": "DUPLICATE_USERNAME",
    "field": "username"
  }
}
```

### Frontend

Map error codes to localized messages:

```tsx
const errorMessages: Record<string, string> = {
  DUPLICATE_USERNAME: t('errors.duplicateUsername'),
  VALIDATION_ERROR: t('errors.validation'),
  INTERNAL_ERROR: t('errors.internal'),
};

function getErrorMessage(code: string, fallback: string): string {
  return errorMessages[code] ?? fallback;
}
```

---

## Testing Error Handling

### Backend

```rust
#[sqlx::test]
async fn test_duplicate_username_returns_conflict(pool: PgPool) {
    // Create first account
    create_account(&pool, request_with_username("alice")).await.unwrap();

    // Attempt duplicate
    let result = create_account(&pool, request_with_username("alice")).await;
    assert!(matches!(result, Err(AccountError::DuplicateUsername)));
}

#[tokio::test]
async fn test_error_response_format() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app.oneshot(/* invalid request */).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: ErrorResponse = parse_body(response).await;
    assert!(!body.error.is_empty());
}
```

### Frontend

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

- [Rust Coding Standards](./rust-coding-standards.md) - Error handling section
- [React Coding Standards](./react-coding-standards.md) - Error boundary section
- [API Contracts](./api-contracts.md) - Error response formats
- `service/src/rest.rs` - ProblemDetails implementation
- `web/src/components/ErrorBoundary/` - React error boundary components
