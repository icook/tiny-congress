# Styling Guide (Mantine-first)

This guide operationalizes [ADR 0001: Mantine-first styling](../adr/adr-0001-mantine-first-styling.md). Use it when adding or revising UI.

- Use Mantine components and style props first for layout, spacing, color, and typography.
- Keep the canonical tokens in `web/src/theme/mantineTheme.ts`; adjust visual changes there instead of scattering overrides.
- Reach for CSS Modules only when Mantine props cannot express the need (complex grid/layout orchestration, keyframe animations, niche responsive tweaks). When you keep CSS, add a short comment in the component explaining why it stays.
- Do not introduce new styling libraries or global CSS unless a ticket requires it.

## Quick examples

- **Bad:** create a `.card` class for padding/radius.

  ```css
  .card {
    padding: 16px;
    border-radius: 8px;
    box-shadow: var(--mantine-shadow-sm);
  }
  ```

- **Good:** use Mantine props.

  ```tsx
  <Paper p="md" radius="md" shadow="sm">
    ...
  </Paper>
  ```

### Before/after reference

- **Before:** Navbar styling spread across a CSS Module for background, padding, hover, and active states.

  ```tsx
  <UnstyledButton className={classes.navLink} data-active={...}>
    <Group gap="sm">
      <IconGauge size={20} />
      <Text>Dashboard</Text>
    </Group>
  </UnstyledButton>
  ```

- **After:** `web/src/components/Navbar/Navbar.tsx` uses Mantine `NavLink`, `Stack`, and theme tokens for spacing, borders, and active states without custom CSS.

  ```tsx
  <NavLink
    component={Link}
    to="/dashboard"
    label="Dashboard"
    leftSection={<IconGauge size={18} stroke={1.5} />}
    active={isActive('/dashboard')}
    radius="sm"
    fw={500}
  />
  ```

Use the refactored Navbar as the “good” example for future navigation or sidebar work.

## Naming & structure

- Keep CSS Modules co-located with the component/page they style.
- Prefer simple class names tied to structure (`.container`, `.hero`) rather than visual traits (`.blueText`).
- Delete unused CSS rules once Mantine props cover the behavior.

## Adding a new page/component

1. Start with Mantine layout primitives (`Stack`, `Group`, `Flex`, `SimpleGrid`, `AppShell`).
2. Apply spacing, radius, shadows, and colors via props (`p`, `m`, `gap`, `radius`, `shadow`, `bg`, `c`).
3. If you need bespoke behavior (sticky headers, animations), isolate it in a `.module.css` next to the component and explain why in a comment.
4. Update `web/src/theme/mantineTheme.ts` if you need new tokens rather than hard-coding values.

## CSS Module audit (current)

- `web/src/components/Navbar/Navbar.tsx`: refactored to Mantine props; no module remains.
- `web/src/pages/Layout.tsx`: logo sizing moved to Mantine `Image`; module removed.
- `web/src/components/Welcome/Welcome.module.css`: kept for hero/typography sizing across breakpoints; comment in component explains the exception.

## Guardrails

- ESLint warns on importing disallowed styling libraries (`tailwindcss`, `styled-components`, `@emotion/*`, `@mui/*`). If you must violate this, document the reason in the PR and the file.
- Prefer adding or adjusting theme tokens over introducing new global CSS or utility layers.
