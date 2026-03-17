# Sidebar User Menu Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move the logged-in user menu from the top-right header dropdown into a collapsible accordion at the bottom of the sidebar, folding in Trust and Endorse links.

**Architecture:** The existing `UserMenu` component in `Layout.tsx` is deleted. The `Navbar` component gains a new bottom section: when authenticated, it renders a `UserAccordion` component that shows username + trust-tier dot badge when collapsed, and Trust / Endorse / Settings / Logout items when expanded. Accordion state is local to `UserAccordion` (not reset on navigation).

**Tech Stack:** React, Mantine v7 (`Accordion`, `NavLink`, `Badge`, `Group`, `Text`, `UnstyledButton`), `@tanstack/react-router`, `@tabler/icons-react`

---

### Task 1: Remove `UserMenu` from the header

**Files:**
- Modify: `web/src/pages/Layout.tsx`

**Step 1: Remove the `UserMenu` function and its header usage**

Delete the entire `UserMenu` function (lines 42–81) and remove `<UserMenu />` from the header `Group` (line 126). Also remove unused imports: `IconLogout`, `IconSettings`, `IconUser`, `Menu`, `UnstyledButton` — but only if no longer used elsewhere in the file.

After edit, the header `Group` should look like:
```tsx
<Group gap="sm" ml="auto">
  <ActionIcon
    variant="subtle"
    onClick={toggleColorScheme}
    size="lg"
    aria-label="Toggle color scheme"
  >
    {colorScheme === 'dark' ? <IconSun size={20} /> : <IconMoon size={20} />}
  </ActionIcon>
</Group>
```

**Step 2: Verify it compiles**

```bash
cd web && yarn tsc --noEmit
```
Expected: no errors relating to Layout.tsx.

**Step 3: Commit**

```bash
git add web/src/pages/Layout.tsx
git commit -m "feat(nav): remove UserMenu from header"
```

---

### Task 2: Create `UserAccordion` component

**Files:**
- Create: `web/src/components/Navbar/UserAccordion.tsx`

**Step 1: Write the component**

```tsx
import {
  IconHeartHandshake,
  IconLogout,
  IconSettings,
  IconShieldCheck,
  IconShieldHalfFilled,
  IconUser,
} from '@tabler/icons-react';
import { Link, useNavigate } from '@tanstack/react-router';
import {
  Accordion,
  Badge,
  Group,
  NavLink,
  Stack,
  Text,
  ThemeIcon,
} from '@mantine/core';
import { useTrustScores } from '@/features/trust';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

function TrustDot({
  isVerified,
  trustScore,
  username,
}: {
  isVerified: boolean;
  trustScore: { distance: number; path_diversity: number } | null;
  username: string | null;
}) {
  if (isVerified && trustScore) {
    if (trustScore.distance <= 3.0 && trustScore.path_diversity >= 2) {
      return <Badge size="xs" color="violet" circle />;
    }
    if (trustScore.distance <= 6.0 && trustScore.path_diversity >= 1) {
      return <Badge size="xs" color="blue" circle />;
    }
    return <Badge size="xs" color="green" circle />;
  }
  if (isVerified) {
    return <Badge size="xs" color="green" circle />;
  }
  const url = buildVerifierUrl(username ?? '');
  if (url) {
    return <Badge size="xs" color="yellow" circle />;
  }
  return null;
}

interface UserAccordionProps {
  onNavigate?: () => void;
}

export function UserAccordion({ onNavigate }: UserAccordionProps) {
  const { deviceKid, privateKey, username, clearDevice } = useDevice();
  const navigate = useNavigate();
  const { crypto } = useCrypto();
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const trustScoresQuery = useTrustScores(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;
  const trustScore = trustScoresQuery.data?.[0] ?? null;

  const handleLogout = () => {
    clearDevice();
    void navigate({ to: '/' });
    onNavigate?.();
  };

  return (
    <Accordion variant="default" chevronPosition="right">
      <Accordion.Item value="user">
        <Accordion.Control>
          <Group gap="xs" wrap="nowrap">
            <ThemeIcon variant="subtle" size="sm">
              <IconUser size={16} />
            </ThemeIcon>
            <Text size="sm" fw={500} truncate>
              {username}
            </Text>
            <TrustDot
              isVerified={isVerified}
              trustScore={trustScore}
              username={username}
            />
          </Group>
        </Accordion.Control>
        <Accordion.Panel>
          <Stack gap={4}>
            <NavLink
              component={Link}
              to="/trust"
              label="Trust"
              leftSection={<IconShieldHalfFilled size={16} stroke={1.5} />}
              onClick={onNavigate}
            />
            <NavLink
              component={Link}
              to="/endorse"
              label="Endorse"
              leftSection={<IconHeartHandshake size={16} stroke={1.5} />}
              onClick={onNavigate}
            />
            <NavLink
              component={Link}
              to="/settings"
              label="Settings"
              leftSection={<IconSettings size={16} stroke={1.5} />}
              onClick={onNavigate}
            />
            <NavLink
              label="Logout"
              leftSection={<IconLogout size={16} stroke={1.5} />}
              color="red"
              onClick={handleLogout}
            />
          </Stack>
        </Accordion.Panel>
      </Accordion.Item>
    </Accordion>
  );
}
```

**Step 2: Verify it compiles**

```bash
cd web && yarn tsc --noEmit
```
Expected: no errors.

**Step 3: Commit**

```bash
git add web/src/components/Navbar/UserAccordion.tsx
git commit -m "feat(nav): add UserAccordion sidebar component"
```

---

### Task 3: Wire `UserAccordion` into the Navbar

**Files:**
- Modify: `web/src/components/Navbar/Navbar.tsx`

**Step 1: Update imports**

Add `UserAccordion` import at the top:
```tsx
import { UserAccordion } from './UserAccordion';
```

Remove from the `authNavLinks` array and its rendering block (lines 26–29 and 136–149):
```tsx
const authNavLinks = [
  { icon: IconShieldHalfFilled, label: 'Trust', path: '/trust' },
  { icon: IconHeartHandshake, label: 'Endorse', path: '/endorse' },
];
```
and
```tsx
{isAuthenticated
  ? authNavLinks.map((link) => (
      <NavLink ... />
    ))
  : null}
```

Remove now-unused icon imports: `IconShieldHalfFilled`, `IconHeartHandshake`.

**Step 2: Replace the authenticated bottom section**

Change the authenticated branch of the bottom section (lines 169–216) from the trust badge display to the `UserAccordion`:

```tsx
) : (
  <Box pt="sm" style={{ borderTop: `1px solid ${borderColor}` }}>
    <UserAccordion onNavigate={onNavigate} />
  </Box>
)}
```

**Step 3: Remove now-unused imports from Navbar.tsx**

Remove: `Badge`, `useTrustScores`, `buildVerifierUrl`, `useVerificationStatus`, `useCrypto`, `IconShieldCheck` (all consumed by `UserAccordion` now). Keep only what Navbar itself still uses.

**Step 4: Verify it compiles with no lint errors**

```bash
cd web && yarn tsc --noEmit && yarn eslint src/components/Navbar/Navbar.tsx src/components/Navbar/UserAccordion.tsx
```
Expected: clean.

**Step 5: Commit**

```bash
git add web/src/components/Navbar/Navbar.tsx
git commit -m "feat(nav): wire UserAccordion into sidebar bottom, remove auth links from main nav"
```

---

### Task 4: Run tests and fix any breakage

**Step 1: Run frontend tests**

```bash
cd web && yarn vitest run
```

Expected: all pass. If any test imports `UserMenu` from Layout or asserts on the header dropdown, update the test to reflect the new location.

**Step 2: Run lint**

```bash
just lint-frontend
```

Expected: clean.

**Step 3: Fix any failures, then commit if changes needed**

```bash
git add -p
git commit -m "fix(nav): update tests for sidebar user menu refactor"
```

---

### Task 5: Manual smoke check

**Step 1: Start the frontend dev server**

```bash
just dev-frontend
```

Open `http://localhost:5173` in a browser.

**Checklist (logged out):**
- [ ] Header shows only logo + env badge + dark/light toggle (no user menu)
- [ ] Sidebar bottom shows Login + Sign Up links

**Checklist (logged in):**
- [ ] Header shows only logo + env badge + dark/light toggle
- [ ] Sidebar bottom shows collapsed accordion: user icon + username + colored dot (or no dot if unverified with no verifier URL)
- [ ] Clicking accordion expands upward to show Trust, Endorse, Settings, Logout
- [ ] Navigating to Trust / Endorse / Settings keeps accordion open
- [ ] Logout clears session and redirects to `/`
- [ ] Trust and Endorse are NOT in the main sidebar nav section
- [ ] Mobile: sidebar closes after tapping a link; accordion state is preserved when reopening sidebar

**Step 2: Commit design doc cleanup**

After smoke check passes:

```bash
git add .plan/2026-03-17-sidebar-user-menu-design.md .plan/2026-03-17-sidebar-user-menu-plan.md
git commit -m "docs: add sidebar user menu design and plan"
```
