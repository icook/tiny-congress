# ADR-005: Mantine-first styling

## Status

Accepted

## Context

- The web client already uses Mantine as its component library, with light use of CSS Modules for bespoke tweaks.
- Styling build tooling is minimal: `postcss-preset-mantine` for Mantine helpers and `postcss-simple-vars` for Mantine breakpoint tokens.
- Theme configuration lived in small ad-hoc objects and individual components sometimes relied on custom CSS for spacing and layout.

## Decision

- Use Mantine components and style props as the primary way to express layout, spacing, color, and typography.
- Keep a single canonical theme object in `web/src/theme/mantineTheme.ts`; `MantineProvider` and tests must import from there.
- Allow CSS Modules only when Mantine props cannot reasonably express the behavior (complex layouts, keyframe animations, niche responsive rules). When CSS is kept, leave a short code comment explaining why.
- Do not introduce additional styling libraries (Tailwind, styled-components, emotion, new global CSS) unless a future ticket explicitly requests it.
- Retain the current PostCSS plugins because existing CSS uses Mantine helpers (`rem()`, `light-dark()`) and breakpoint variables; revisit if CSS usage shrinks further.

## Alternatives

- **Tailwind**: Would introduce a second styling DSL, duplicate Mantine capabilities, and require re-training contributors and agents.
- **MUI**: Another full component system with its own styling model; switching would slow iteration and conflict with Mantine components already in place.
- **Bootstrap/SCSS**: Global utility layer would fight Mantine’s scoped styling and complicate tree-shaking.
- **CSS-in-JS (styled-components/emotion)**: Adds runtime styling overhead and fragment the styling story; Mantine props already cover the needed surface.

## Consequences

- Faster UI iteration by relying on off-the-shelf Mantine primitives and tokens.
- Boring, utilitarian visuals that are easy for humans and LLMs to extend without inventing new patterns.
- Some tighter coupling to Mantine APIs; future visual shifts require updating the shared theme object.
- A small amount of CSS remains for edge cases (e.g., bespoke hero sizing) with explicit justification.
- ESLint guardrails warn on bringing in disallowed styling libraries to keep the Mantine-first approach intact.

## References

- Style policy: `../style/STYLE_GUIDE.md`
- LLM extension guide: `../style/LLM_UI_GUIDE.md`
- Canonical theme: `web/src/theme/mantineTheme.ts`
- Reference implementation: `web/src/components/Navbar/Navbar.tsx`
