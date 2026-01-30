# TODO

## Wayland/WebKitGTK Workarounds

**Status:** No workarounds currently in code. If issues arise, consider:
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` — Safer fallback for NVIDIA users

### References

- https://github.com/tauri-apps/tauri/issues/9394
- https://github.com/tauri-apps/tauri/issues/12361
- https://yaak.app/docs/getting-started/troubleshooting

## Dependency Updates

**Status:** All dependencies up to date as of this commit.

### Completed

- [x] Tauri ecosystem 2.9.x
- [x] Rust dependencies (all patch + minor)
- [x] Vue 3.5.27
- [x] Vite 7.3.1
- [x] Vitest 4.0.18
- [x] Pinia 3.0.4
- [x] Vue-tsc 3.2.4
- [x] TypeScript 5.9.3
- [x] ESLint 9.39.2 + related plugins
- [x] Unplugin-auto-import 21.0.0
- [x] Cross-env 10.1.0
- [x] PostCSS 8.5.6 + plugins
- [x] pnpm 10.28.2
- [x] **Tailwind CSS 4.1.18** — Migrated to CSS-first config with `@tailwindcss/vite` plugin
