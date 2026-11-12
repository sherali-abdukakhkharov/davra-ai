# Davra web

The public site and sign-in screens for Davra: landing page (`/`), `/login`, `/register` and a
minimal signed-in stub (`/app`). React 19, TypeScript (strict), Vite, Tailwind CSS v4, shadcn/ui
components (copied into `src/components/ui`), React Router.

No runtime network calls besides the hub's own `/api`: fonts are self-hosted from `@fontsource`,
icons come from `lucide-react`, there is no analytics.

## Commands

```sh
npm ci            # install exact pinned versions
npm run dev       # http://localhost:5173, proxies /api and /.well-known to http://127.0.0.1:8787
npm run typecheck # tsc --noEmit
npm run build     # typecheck + production build into dist/ (index.html at the root)
npm run preview   # serve dist/ on http://localhost:4321 with the same proxy
```

The hub serves `dist/` with an SPA fallback: any unknown path should return `dist/index.html`.

## Layout

- `src/i18n/dict.ts`: every user-facing string, English and Uzbek (Latin).
- `src/lib/api.ts`: client for the hub's auth API (`/api/v1/auth/*`, `X-Davra: 1` on every POST).
- `src/lib/prefs.tsx`: language and theme (follows `prefers-color-scheme` until toggled).
- `src/components/landing/`: the sample room in the hero (synthetic data).
- `src/pages/`: route components; `/login`, `/register`, `/app` are lazy chunks.
- `public/llms.txt`: plain-text summary for AI agents.
- `public/theme-boot.js`: sets theme and language before first paint (kept external so a strict
  `script-src 'self'` CSP works).
