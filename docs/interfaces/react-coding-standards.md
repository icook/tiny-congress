# React Coding Standards

Guidelines for consistent, maintainable React code in the web crate.

## Error Handling

### Use Error Boundaries for Component Failures

Wrap strategic parts of the component tree with error boundaries to prevent crashes from propagating:

```tsx
// Good: Strategic error boundary placement
import { ErrorBoundary } from '@/components/ErrorBoundary';

function App() {
  return (
    <ErrorBoundary context="Application">
      <QueryProvider>
        <MantineProvider>
          <Router />
        </MantineProvider>
      </QueryProvider>
    </ErrorBoundary>
  );
}
```

### Error Boundary Placement

Place error boundaries at these locations:

| Location | Purpose |
|----------|---------|
| App root | Last line of defense |
| Router | Prevents navigation failures from crashing |
| Query provider | Isolates data fetching errors |
| Complex features | Charts, forms, third-party integrations |

```tsx
// Good: Nested boundaries for isolation
<ErrorBoundary context="Application">
  <Header />
  <Sidebar />
  <ErrorBoundary context="MainContent">
    <MainContent />
  </ErrorBoundary>
  <Footer />
</ErrorBoundary>

// Bad: Too coarse - one error breaks everything
<ErrorBoundary>
  <Header />
  <Sidebar />
  <MainContent />
  <Footer />
</ErrorBoundary>

// Bad: Too fine - over-engineering
<ErrorBoundary>
  <Button />
</ErrorBoundary>
```

### ErrorBoundary Component API

```tsx
interface ErrorBoundaryProps {
  children: ReactNode;
  context?: string;      // Identifies where error occurred
  fallback?: ReactNode;  // Custom fallback UI
  onError?: (error: Error, info: { componentStack: string }) => void;
}

// Usage with all props
<ErrorBoundary
  context="Dashboard"
  fallback={<CustomErrorUI />}
  onError={(error) => trackError(error)}
>
  <DashboardContent />
</ErrorBoundary>
```

### Query Error Handling

Use TanStack Query's built-in error handling for data fetching:

```tsx
// Good: Handle query errors in component
function UserList() {
  const { data, error, isError } = useQuery({
    queryKey: ['users'],
    queryFn: fetchUsers,
  });

  if (isError) {
    return <Alert color="red">Failed to load users: {error.message}</Alert>;
  }

  return <div>{/* render data */}</div>;
}

// Bad: Let query errors bubble uncaught
function UserList() {
  const { data } = useQuery({
    queryKey: ['users'],
    queryFn: fetchUsers,
  });

  return <div>{data.map(...)}</div>; // Crashes if data is undefined
}
```

### Error Logging

Development errors are logged to console with context. For production, integrate error tracking:

```tsx
// Future: Add observability in ErrorBoundary.tsx
if (import.meta.env.PROD) {
  Sentry.captureException(error, {
    tags: { context },
    extra: { componentStack: info.componentStack },
  });
}
```

## Component Structure

### Prefer Function Components

```tsx
// Good: Function component with hooks
function UserProfile({ userId }: UserProfileProps) {
  const { data } = useQuery({ queryKey: ['user', userId], queryFn: () => fetchUser(userId) });
  return <div>{data?.name}</div>;
}

// Avoid: Class components
class UserProfile extends Component { ... }
```

### Props Interface Naming

```tsx
// Good: ComponentNameProps
interface UserProfileProps {
  userId: string;
  onUpdate?: () => void;
}

// Bad: Inconsistent naming
interface IUserProfile { ... }
interface UserProfilePropsType { ... }
```

### Destructure Props

```tsx
// Good: Destructure in signature
function UserCard({ name, email, avatar }: UserCardProps) {
  return <div>{name}</div>;
}

// Avoid: Props object access
function UserCard(props: UserCardProps) {
  return <div>{props.name}</div>;
}
```

## Hooks

### Custom Hook Naming

Prefix custom hooks with `use`:

```tsx
// Good
function useUserData(userId: string) { ... }
function useDebounce<T>(value: T, delay: number) { ... }

// Bad
function getUserData(userId: string) { ... }  // Looks like regular function
```

### Exhaustive Dependencies

Always include all dependencies in useEffect/useCallback/useMemo:

```tsx
// Good: All dependencies listed
useEffect(() => {
  fetchData(userId);
}, [userId, fetchData]);

// Bad: Missing dependency
useEffect(() => {
  fetchData(userId);
}, []); // userId should be in deps
```

ESLint enforces this with `react-hooks/exhaustive-deps: error`.

## State Management

### Prefer TanStack Query for Server State

```tsx
// Good: Server state in TanStack Query
function UserList() {
  const { data } = useQuery({ queryKey: ['users'], queryFn: fetchUsers });
  return <div>{data?.map(...)}</div>;
}

// Bad: Server state in useState
function UserList() {
  const [users, setUsers] = useState([]);
  useEffect(() => {
    fetchUsers().then(setUsers);
  }, []);
  return <div>{users.map(...)}</div>;
}
```

### Use Local State for UI State

```tsx
// Good: UI state in useState
function Modal() {
  const [isOpen, setIsOpen] = useState(false);
  return <Dialog opened={isOpen} onClose={() => setIsOpen(false)} />;
}
```

## Styling

### Mantine-First Approach

Use Mantine components and props instead of custom CSS or other styling systems. See `docs/style/STYLE_GUIDE.md`.

```tsx
// Good: Mantine props
<Stack gap="md">
  <Text size="lg" fw={500}>Title</Text>
  <Button color="blue" variant="filled">Submit</Button>
</Stack>

// Bad: Custom CSS classes
<div className="stack">
  <span className="title">Title</span>
  <button className="btn-primary">Submit</button>
</div>
```

ESLint warns against importing Tailwind, styled-components, Emotion, or MUI.

## File Organization

### One Component Per File

```
src/components/
├── UserCard/
│   ├── UserCard.tsx        # Component
│   ├── UserCard.test.tsx   # Tests
│   ├── UserCard.story.tsx  # Storybook story
│   └── index.ts            # Re-export
└── ErrorBoundary/
    ├── ErrorBoundary.tsx
    ├── ErrorFallback.tsx
    ├── ErrorBoundary.test.tsx
    ├── ErrorFallback.test.tsx
    └── index.ts
```

### Barrel Exports

Use index.ts for clean imports:

```tsx
// src/components/ErrorBoundary/index.ts
export { ErrorBoundary } from './ErrorBoundary';
export { ErrorFallback } from './ErrorFallback';

// Usage
import { ErrorBoundary } from '@/components/ErrorBoundary';
```

## Testing

### Use Testing Library

```tsx
import { render, screen } from '@test-utils';
import { UserCard } from './UserCard';

test('displays user name', () => {
  render(<UserCard name="Alice" email="alice@example.com" />);
  expect(screen.getByText('Alice')).toBeInTheDocument();
});
```

### Test Error Boundaries

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
```

### Query Key Testing

Mock or use testing utilities for TanStack Query:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function wrapper({ children }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

test('fetches user data', async () => {
  render(<UserProfile userId="123" />, { wrapper });
  await screen.findByText('Alice');
});
```

## Anti-patterns

| Don't | Do Instead |
|-------|------------|
| `any` type | Define proper interfaces |
| `// @ts-ignore` | Fix the type error |
| `useEffect` for derived state | `useMemo` or compute inline |
| State for server data | TanStack Query |
| Inline styles | Mantine props |
| `console.log` in commits | Remove or use conditional logging |
| Empty dependency arrays for effects that use props | Include all dependencies |
| Catch and ignore errors | Handle or propagate with context |
