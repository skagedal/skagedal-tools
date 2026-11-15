# React apps

How a React app is written: the page of a tool that serves one, like
`calendar-view` or `kontoutdrag`, or an app of its own. The setup that
is specific to `skagedal-tools` (where the app lives, how it is built
into a binary) is in its `AGENTS.md`.

## Effects live in hooks, not components

A component does not call `useEffect`. An effect in a component mixes
what the page shows with when and how it talks to the outside world, and
the two are easier to read, and to get right, apart. Reach for these
instead, in this order:

- **The server: [react-query](https://tanstack.com/query).** Every fetch
  is a `useQuery`, every request that changes something a `useMutation`.
  Loading, errors, caching, refetching and polling (`refetchInterval`)
  come with it, rather than being written again around a `fetch` in an
  effect. When a query should run again because something changed, put
  that something in its `queryKey`. Prefetch with `useQueries` or
  `prefetchQuery`, not with a fetch whose result is thrown away.
- **The browser: an established hook library.** Keyboard shortcuts with
  [react-hotkeys-hook](https://react-hotkeys-hook.vercel.app/);
  `localStorage`, intervals, media queries, window size and the rest with
  [usehooks-ts](https://usehooks-ts.com/).
- **An outside value React should follow** (the URL hash, say):
  `useSyncExternalStore`.
- **Something to do to an element once it exists** (scroll it, focus
  it): a ref callback.
- **Anything else: a named hook of your own**, in `hooks.ts`, that says
  what it is for: `useHashPlace`, not an effect inline in `App`.

The page's I/O then reads as a list of hooks in one file, and components
are left with state and rendering. ESLint holds the line:
`no-restricted-imports` refuses `useEffect` and `useLayoutEffect` from
`react` everywhere but `hooks.ts`.

## Dates and times are Temporal

Date and time arithmetic uses
[Temporal](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Temporal),
not `Date`. A day is a `Temporal.PlainDate` and a moment shown to the
user a `Temporal.ZonedDateTime` in the browser's zone
(`Temporal.Now.timeZoneId()`). `Date` cannot say "a day" without a time
and a zone, which is where most calendar bugs come from: an off-by-one at
midnight, a day assumed to be 24 hours across a clock change.

- Import `temporal-polyfill/global` first thing in `main.tsx`. It uses
  the browser's own Temporal where there is one and installs
  [temporal-polyfill](https://github.com/fullcalendar/temporal-polyfill)
  where there is not, Safari among them.
- Take the types from TypeScript: add `"ESNext.Temporal"`, `"ESNext.Intl"`
  and `"ESNext.Date"` to `lib` in `tsconfig.json` (TypeScript 6 or later).
- Format with `toLocaleString` on the Temporal value, not by hand.

ESLint's `no-restricted-globals` refuses `Date`.

## Tooling

- Vite, TypeScript with `strict`, and pnpm. Dependencies are written as
  `"latest"`; see [dependency-versions.md](dependency-versions.md).
- ESLint 9 with a flat config: `@eslint/js`, `typescript-eslint`,
  `eslint-plugin-react` and `eslint-plugin-react-hooks`, plus the two
  rules above. `log-viewer/browser/eslint.config.js` and
  `calendar-view/browser/eslint.config.js` are references.
- `typescript` is pinned to `^6` where ESLint runs: typescript-eslint
  does not support TypeScript 7 yet.
- The `check` script runs `tsc --noEmit` and `eslint .`, and the tests
  when there are any.

## Look

Colours are CSS custom properties on `:root`, set again under
`@media (prefers-color-scheme: dark)`, so the page follows the system's
light or dark appearance. Times are shown in 24-hour form.

## Older apps

`kontoutdrag` predates this guide, and `log-viewer` predates the parts
about effects and Temporal. Bring them in line when you are working in
them anyway.
