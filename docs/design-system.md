# Bella Design System

Bella uses a Bernese mountain dog-inspired palette: graphite, off white, orange,
warm gray, charcoal, and a small set of operational status colors. The product is a
dark-first operational tool, so most screens should feel quiet, dense, and easy
to scan. Orange is a focused action color, not a general decoration color.

## Source Of Truth

The implementation source of truth is `apps/web/src/app/globals.css`.

The web app uses Tailwind v4, shadcn CSS variables, and `next-themes`.
Components should consume semantic tokens through Tailwind classes, not direct
color values.

Use:

```tsx
<Button>Investigate</Button>
<section className="bg-background text-foreground" />
<Card className="bg-card text-card-foreground" />
<p className="text-muted-foreground">Synced 4 minutes ago</p>
```

Avoid:

```tsx
<div className="bg-black text-white" />
<span className="text-orange-500" />
<div style={{ backgroundColor: "#121317" }} />
```

## Brand Primitives

| Token | Value | Role |
| --- | --- | --- |
| `--bella-graphite` | `#121317` | Sidebar and elevated dark neutral surface |
| `--bella-off-white` | `#F7F7F5` | Primary text on dark surfaces |
| `--bella-orange` | `#E68A2E` | Primary actions, active states, focus, key highlights |
| `--bella-charcoal` | `#2A2826` | Subdued neutral depth on light surfaces |
| `--bella-warm-gray` | `#D7D7D2` | Borders, dividers, muted fills |
| `--bella-success` | `#4CAF70` | Successful syncs, healthy states |
| `--bella-error` | `#E05A5A` | Errors and destructive states |
| `--bella-info` | `#4A8DFF` | Informational system states |

## Semantic Mapping

Components should use semantic tokens:

| Need | Use |
| --- | --- |
| Page surface | `bg-background text-foreground` |
| Panels and repeated content | `bg-card text-card-foreground` |
| Menus, dialogs, popovers | `bg-popover text-popover-foreground` |
| Primary actions | `bg-primary text-primary-foreground` |
| Secondary fills | `bg-secondary text-secondary-foreground` |
| Subtle areas | `bg-muted text-muted-foreground` |
| Hover and selected rows | `bg-accent text-accent-foreground` |
| Borders and dividers | `border-border` |
| Inputs | `border-input` |
| Focus rings | `ring-ring` or component defaults |
| Errors | `text-destructive`, `bg-destructive/10`, `border-destructive` |
| Warnings | `text-warning`, `bg-warning/15` |
| Success | `text-success`, `bg-success/15` |
| Info | `text-info`, `bg-info/15` |

## Dark Mode Rules

Dark mode is the product default. Light mode is available through the user menu
theme toggle, but new UI should be reviewed in dark mode first. In dark mode:

- True black is the main application surface. Graphite is used for sidebar and elevated dark neutral areas.
- Off White is primary text and high-emphasis icon color.
- Orange is for primary buttons, active navigation, focus rings, selected states,
  and the most important chart series.
- Warm gray and softened foreground colors create hierarchy through muted text, dividers, and subdued
  controls.

Do not create isolated light cards inside dark workflows unless the content is a
document, preview, uploaded asset, or another object whose native appearance is
light.

## Component Rules

- Prefer existing shadcn components from `apps/web/src/components/ui`.
- Use component variants before adding custom styling.
- Use `className` for layout and spacing. Do not override component colors or
  typography unless the component exposes a variant for it.
- Use `gap-*` instead of `space-x-*` or `space-y-*`.
- Use `Badge` for statuses and small labels.
- Use lucide icons in buttons when an icon exists.
- Add new color semantics to `globals.css` before using them in components.

## Illustration And Iconography

The Bernese mountain dog is the brand mascot. Use illustration sparingly in
empty states, onboarding, and investigation progress states. Product workflows
should remain compact and utility-first.

Iconography should be simple line icons. Use orange only when the icon represents
an active state, current selection, primary action, or important signal.

## Agent Checklist

Before finishing UI work:

1. Search changed files for raw colors: `bg-black`, `bg-white`, `text-white`,
   `text-black`, `bg-*-*`, `text-*-*`, and hex values.
2. Replace raw colors with semantic tokens unless the raw color is part of an
   external embedded asset or third-party chart selector.
3. Check dark mode first.
4. Make sure orange has not become a general decoration color.
5. Run the app's lint/typecheck commands when practical.
