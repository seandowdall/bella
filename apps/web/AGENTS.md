<!-- BEGIN:nextjs-agent-rules -->
# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->

## Bella Design System

Dark mode is the default, but light mode is supported through `next-themes`.
Build UI with shadcn semantic color tokens from
`src/app/globals.css`: `bg-background`, `bg-card`, `text-foreground`,
`text-muted-foreground`, `bg-primary`, `border-border`, and `ring-ring`.

Do not use raw Tailwind palette colors such as `bg-orange-*`, `text-blue-*`,
`bg-black`, `bg-white`, or hex values in components. Add or update semantic
tokens in `src/app/globals.css` instead.

Bella Orange is reserved for primary actions, active navigation, focus states,
and important highlights. Off White is primarily text on dark surfaces, not a
general dark-mode fill. True black is the main application surface; Graphite is
used for the sidebar and elevated dark neutral areas.

See `../../docs/design-system.md` before introducing new visual variants,
status colors, or branded illustration treatments.
