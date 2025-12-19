# Import Boundaries Design

**Ticket:** #219 - [UI] Enforce import boundaries (barrels + alias vs relative)
**Date:** 2025-12-18
**Status:** Approved

## Overview

Establish and enforce clear import policies for frontend code to prevent tangled dependencies and brittle relative imports across features.

## Decisions

| Topic | Decision |
|-------|----------|
| Enforcement level | Strict from day one (errors, not warnings) |
| Within-feature imports | Sub-barrel pattern (`../api`, not `../api/client`) |
| Cross-feature imports | Forbidden — lift shared code to shared layers |
| Shared layers | Existing: `components`, `api`, `providers`, `theme` |
| Pages | Import from features and shared; nothing imports pages |
| Tree-shaking tradeoff | Accept it — trust modern bundlers |
| Tooling | `eslint-plugin-boundaries` for declarative rules |

## Import Hierarchy

One-way dependency graph:

```
pages
  ↓
features
  ↓
shared layers (components, api, providers, theme)
```

### Allowed Imports by Location

| From | Can Import |
|------|------------|
| `@/pages/*` | `@/features/*`, `@/components/*`, `@/api/*`, `@/providers/*`, `@/theme/*` |
| `@/features/*` | `@/components/*`, `@/api/*`, `@/providers/*`, `@/theme/*` |
| `@/components/*` | `@/api/*`, `@/providers/*`, `@/theme/*` |
| `@/api/*` | `@/providers/*`, `@/theme/*` |
| `@/providers/*` | `@/theme/*` |

### Forbidden Patterns

- Features importing other features (`@/features/foo` → `@/features/bar`)
- Anything importing from pages (`@/pages/*`)
- Deep imports into feature internals (`@/features/identity/keys/crypto`)

### Within a Feature

- Relative imports allowed only within the same subdirectory
- Sibling subdirectories import through their barrel (`../api` not `../api/client`)

## ESLint Implementation

Using `eslint-plugin-boundaries` (~147KB, no native dependencies).

### Element Types

```javascript
{
  type: 'pages',      match: 'src/pages/*',
  type: 'features',   match: 'src/features/*',
  type: 'components', match: 'src/components/*',
  type: 'api',        match: 'src/api/*',
  type: 'providers',  match: 'src/providers/*',
  type: 'theme',      match: 'src/theme/*',
}
```

### Dependency Rules

```javascript
rules: [
  { from: 'pages',      allow: ['features', 'components', 'api', 'providers', 'theme'] },
  { from: 'features',   allow: ['components', 'api', 'providers', 'theme'] },
  { from: 'components', allow: ['api', 'providers', 'theme'] },
  { from: 'api',        allow: ['providers', 'theme'] },
  { from: 'providers',  allow: ['theme'] },
]
```

### Within-Feature Barrel Enforcement

Block deep relative imports into sibling directories:

```javascript
{
  patterns: [{
    group: ['../*/**'],
    message: 'Import from sibling barrel (../api) not internals (../api/client).'
  }]
}
```

## Existing Violations

One violation found:

| File | Current | Fix |
|------|---------|-----|
| `features/identity/screens/Signup.tsx` | `../api/queries` | `../api` |

## Implementation Tasks

1. Install `eslint-plugin-boundaries`
2. Update `web/eslint.config.js` with boundaries config
3. Fix existing violation in `Signup.tsx`
4. Add "Import Boundaries" section to `docs/interfaces/react-coding-standards.md`
5. Run `just lint-frontend` and `just test-frontend` to verify

## Files Changed

| File | Change |
|------|--------|
| `web/package.json` | Add `eslint-plugin-boundaries` |
| `web/eslint.config.js` | Add boundaries config |
| `web/src/features/identity/screens/Signup.tsx` | Fix import |
| `docs/interfaces/react-coding-standards.md` | Add section |

## Documentation

Add to `docs/interfaces/react-coding-standards.md`:

```markdown
## Import Boundaries

ESLint enforces a strict import hierarchy to maintain clean dependencies.

### Layer Hierarchy

pages → features → shared layers

| Layer | Can Import From |
|-------|-----------------|
| `@/pages/*` | features, components, api, providers, theme |
| `@/features/*` | components, api, providers, theme |
| `@/components/*` | api, providers, theme |
| `@/api/*` | providers, theme |
| `@/providers/*` | theme |

### Rules

1. **Use `@/` alias for cross-layer imports**
2. **Features cannot import other features** — lift shared code
3. **No deep imports** — use barrels (`@/features/identity`, not `@/features/identity/api/client`)
4. **Relative imports within features** — use sibling barrels (`../api`, not `../api/client`)

### Examples

// Good: Page imports feature
import { Signup } from '@/features/identity';

// Good: Feature imports shared
import { Button } from '@/components';

// Bad: Feature imports feature
import { something } from '@/features/other';

// Bad: Deep import
import { client } from '@/features/identity/api/client';
```
