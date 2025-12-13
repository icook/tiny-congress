# LLM UI Extension Guide

You are extending the TinyCongress UI. Follow these instructions literally.

- Start with Mantine components and props. Avoid writing CSS unless Mantine props cannot express the requirement.
- Use the canonical theme in `web/src/theme/mantineTheme.ts` for colors, radius, spacing, and typography.
- Consult [ADR-005](../decisions/005-mantine-first-styling.md) and `STYLE_GUIDE.md` before choosing a pattern.
- If you keep or add CSS, keep it in a `.module.css` next to the component and add a short comment in the component explaining why CSS is required.
- Reference `web/src/components/Navbar/Navbar.tsx` as the preferred Mantine-first example for navigation and layout.

## Do not

- Do not add Tailwind, styled-components, emotion, MUI, or other styling libraries.
- Do not create new global CSS files; use component-level modules only when necessary.
- Do not hard-code colors/radius/spacing; pull from Mantine props or the shared theme.

## Worked patterns

- **Add a new form section**
  - Use `Stack` to space fields (`gap="md"`), `Group` for inline controls.
  - Prefer `TextInput`, `PasswordInput`, `Checkbox`, `Select`, `Textarea`.
  - Set labels/description via component props; avoid custom CSS for spacing.
  - Example: ` <Stack gap="sm"><TextInput label="Display name" required /><Checkbox label="Share profile" /></Stack> `

- **Add a filter bar**
  - Use `Group` or `Flex` with `wrap="wrap"` for responsive alignment.
  - Combine `Select`, `SegmentedControl`, `Checkbox`, and `Button` components.
  - Apply `px`, `py`, `bg`, `radius`, `shadow="xs"` on a `Paper` instead of custom classes.

## Data Validation

Always validate external data (API responses) with Zod:

- Define schemas in `web/src/api/schemas.ts`
- Use `z.infer<typeof schema>` for TypeScript types
- Call `.parse()` on API responses to validate at runtime
- Example reference: `web/src/api/buildInfo.ts` and `web/src/api/schemas.ts`

```typescript
// In schemas.ts
export const userSchema = z.object({
  id: z.string(),
  name: z.string(),
  email: z.string().email(),
});
export type User = z.infer<typeof userSchema>;

// In API function
const data = await graphqlRequest<{ user: User }>(query);
const result = userSchema.parse(data.user); // Throws if invalid
return result;
```

## Where to look

- Tokens/theme defaults: `web/src/theme/mantineTheme.ts`
- Policy and examples: `STYLE_GUIDE.md`
- Navigation reference: `web/src/components/Navbar/Navbar.tsx`
- Data validation: `web/src/api/schemas.ts`, `web/src/api/buildInfo.ts`

## If you think you need CSS

1. Check if `Stack`, `Group`, `Flex`, `Grid`, or `SimpleGrid` with props solve it.
2. If not, create a `.module.css` file next to the component.
3. Add a short comment in the component (e.g., `// CSS kept for sticky scroll behavior; see ADR-005`) so humans know why.
