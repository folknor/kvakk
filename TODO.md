# TODO

## Wayland/WebKitGTK Workarounds

**Location:** `app/main/src-tauri/src/main.rs`

Current workarounds:
```rust
#[cfg(target_os = "linux")]
std::env::set_var("GDK_BACKEND", "x11");
#[cfg(target_os = "linux")]
std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
```

### Recommendation

- [ ] **Remove `GDK_BACKEND=x11`** — Forces X11 for all users, breaks Wayland-only setups, increasingly problematic as distros drop X11
- [ ] **Keep `WEBKIT_DISABLE_DMABUF_RENDERER=1`** — Safer fallback that works on both X11 and Wayland

The DMA-BUF issues are primarily NVIDIA-specific. Users with AMD/Intel on Wayland typically work fine.

### Alternative

Remove both workarounds and document them for affected users to set externally (like Yaak, Atuin do).

### References

- https://github.com/tauri-apps/tauri/issues/9394
- https://github.com/tauri-apps/tauri/issues/12361
- https://yaak.app/docs/getting-started/troubleshooting

## Dependency Updates

**Location:** `app/main/package.json`

### Safe Updates (minor/patch)

- [ ] Tauri ecosystem (2.0.x → 2.9.x)
  - `@tauri-apps/api` 2.0.3 → 2.9.1
  - `@tauri-apps/cli` 2.0.4 → 2.9.6
  - `@tauri-apps/plugin-*` various → 2.x latest
- [ ] `vue` 3.5.12 → 3.5.27
- [ ] `typescript` 5.6.3 → 5.9.3
- [ ] `autoprefixer` 10.4.20 → 10.4.23
- [ ] `postcss` 8.4.47 → 8.5.6
- [ ] `semver` 7.6.3 → 7.7.3

### Major Updates (require migration)

- [ ] `tailwindcss` 3.4.14 → 4.1.18 — Major rewrite, new config format
- [ ] `vite` 5.4.10 → 7.3.1 — Two major versions behind
- [ ] `pinia` 2.2.6 → 3.0.4 — Major version
- [ ] `vitest` 2.1.4 → 4.0.18 — Two major versions
- [ ] `@vitejs/plugin-vue` 5.1.4 → 6.0.3 — Follows Vite
- [ ] `vue-tsc` 2.1.10 → 3.2.4 — Major version
- [ ] `unplugin-auto-import` 0.19.0 → 21.0.0 — Major jump
- [ ] `eslint-plugin-vue` 9.30.0 → 10.7.0 — Major version
- [ ] `@stylistic/eslint-plugin` 2.10.1 → 5.7.1 — Major version
- [ ] `eslint-config-prettier` 9.1.0 → 10.1.8 — Major version
- [ ] `globals` 15.14.0 → 17.2.0 — Major version
- [ ] `cross-env` 7.0.3 → 10.1.0 — Major version
- [ ] `postcss-nesting` 13.0.1 → 14.0.0 — Major version
- [ ] `@types/node` 20.16.5 → 25.1.0 — Major version

## Rust Dependency Updates

**Location:** `core_lib/Cargo.toml`

All updates are patch/minor — should be safe to upgrade.

- [ ] Minor updates
  - `prost` / `prost-build` 0.13.5 → 0.14.3
  - `tokio` 1.43.0 → 1.49.0
  - `uuid` 1.15.1 → 1.20.0
  - `bytes` 1.10.1 → 1.11.0
  - `once_cell` 1.20.3 → 1.21.3
- [ ] Patch updates
  - `anyhow` 1.0.97 → 1.0.100
  - `btleplug` 0.11.7 → 0.11.8
  - `log` 0.4.26 → 0.4.29
  - `rand` 0.9.0 → 0.9.2
  - `serde` 1.0.218 → 1.0.228
  - `sha2` 0.10.8 → 0.10.9
  - `tokio-util` 0.7.13 → 0.7.18
  - `tracing-subscriber` 0.3.19 → 0.3.22
  - `dbus` 0.9.7 → 0.9.10
  - `bluer` 0.17.3 → 0.17.4
