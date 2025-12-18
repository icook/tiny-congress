# Test Utilities

Shared testing utilities for the TinyCongress frontend.

## Files

| File | Purpose |
|------|---------|
| `index.ts` | Re-exports all utilities |
| `render.tsx` | Custom render function with providers |

## Usage

Import the custom `render` function instead of `@testing-library/react`:

```tsx
import { render } from '@test-utils';
import { MyComponent } from './MyComponent';

test('renders correctly', () => {
  const { getByText } = render(<MyComponent />);
  expect(getByText('Hello')).toBeInTheDocument();
});
```

## What render() Provides

The custom render wraps components with:
- `QueryClientProvider` - TanStack Query with test-friendly defaults (no retries)
- `MantineProvider` - Theme context matching the app's Mantine theme

This ensures components render in the same context as the real app.

`render` also accepts standard Testing Library render options (including `wrapper`) when needed.

## Adding Utilities

When adding shared test helpers:
1. Create the utility in this directory
2. Export it from `index.ts`
3. Document usage in this README

## Related

- [frontend-test-patterns playbook](../../docs/playbooks/frontend-test-patterns.md)
- [react-coding-standards](../../docs/interfaces/react-coding-standards.md)
