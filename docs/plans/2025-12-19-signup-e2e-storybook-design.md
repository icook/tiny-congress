# Signup E2E Tests & Storybook Design

## Overview

Add end-to-end tests for the signup flow with screenshots in the Playwright HTML report, plus comprehensive Storybook stories for the Signup component. Establishes presentational/container pattern for scalable component architecture.

## Component Architecture

### File Structure

```
web/src/pages/
└── Signup.page.tsx              # Route-level container

web/src/features/identity/components/
├── SignupForm.tsx               # Presentational component
└── SignupForm.story.tsx         # Storybook stories

web/tests/e2e/
└── signup.spec.ts               # E2E test with screenshots
```

### Pattern: Pages as Thin Wrappers

- `pages/*.page.tsx` - Route shells (handle params, compose features)
- `features/*/components/` - Reusable UI pieces
- `features/*/api/` - Data layer

This pattern provides:
- Clear route discovery - all routes in `pages/`
- Feature co-location - related code stays together
- Consistent naming - `.page.tsx` suffix for all routes

### SignupForm Props Interface

```typescript
type SignupFormProps = {
  // Form state
  username: string;
  onUsernameChange: (value: string) => void;
  onSubmit: (e: React.FormEvent) => void;

  // Loading states
  isLoading: boolean;
  loadingText?: string;  // "Generating keys..." vs default spinner

  // Error state
  error?: string | null;

  // Success state (when set, shows success view instead of form)
  successData?: {
    account_id: string;
    root_kid: string;
  } | null;
};
```

## Storybook Stories

**File:** `web/src/features/identity/components/SignupForm.story.tsx`

Six stories covering all states:
1. **Default** - Empty form ready for input
2. **Filled** - Form with username entered
3. **GeneratingKeys** - Loading with "Generating keys..." text
4. **Submitting** - Loading with default spinner
5. **Error** - Form with error alert
6. **Success** - Success view with account details

Each story is pure props - no mocking, no providers required.

## E2E Test

**File:** `web/tests/e2e/signup.spec.ts`

### Test: signup flow creates account

1. Navigate to `/signup`
2. Capture screenshot: "signup-form"
3. Fill username with unique value (`test-user-${Date.now()}`)
4. Click "Sign Up"
5. Wait for success (Account Created visible)
6. Capture screenshot: "signup-success"

Screenshots attached via `test.info().attach()` for HTML report visibility.

## Implementation Changes

### New Files

| File | Purpose |
|------|---------|
| `web/src/pages/Signup.page.tsx` | Route-level container |
| `web/src/pages/Signup.page.test.tsx` | Container tests |
| `web/src/features/identity/components/SignupForm.tsx` | Presentational component |
| `web/src/features/identity/components/SignupForm.story.tsx` | Storybook stories |
| `web/src/features/identity/components/index.ts` | Component exports |
| `web/tests/e2e/signup.spec.ts` | E2E test with screenshots |

### Modified Files

| File | Change |
|------|--------|
| `web/src/Router.tsx` | Import SignupPage from pages/ |
| `web/src/features/identity/index.ts` | Export from components/ instead of screens/ |

### Deleted Files

| File | Reason |
|------|--------|
| `web/src/features/identity/screens/` | Replaced by pages/ + components/ pattern |

## Pattern Established

This implementation establishes:

1. **Pages as thin wrappers** - Route components in `pages/` compose feature components
2. **Presentational/Container split** - Complex form components separate "what it looks like" from "what it does"
3. **Storybook as prop examples** - Stories are declarative prop variations, not mock configurations
4. **E2E screenshots in HTML report** - Use `test.info().attach()` for visibility

## Constraints

- No new dependencies
- No API changes
- No database changes
- Existing tests continue to pass
