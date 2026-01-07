# GitHub Actions Workflows

This directory contains automated workflows for the Craterboy emulator project.

## Workflows

### 1. CI (`ci.yml`)
**Trigger:** Automatically on pull requests to `main`

**Purpose:** Continuous integration checks for pull requests

**Jobs:**
- Install system dependencies (ALSA, X11/Wayland, udev)
- Check code formatting with `cargo fmt`
- Run linter with `cargo clippy`
- Build the project
- Run all tests

**Caching:** Enabled for cargo registry, index, and build artifacts to speed up runs

---

### 2. Release (`release.yml`)
**Trigger:** 
- Manual workflow dispatch (Actions tab → Release Build → Run workflow)
- Automatically on version tags matching `v*.*.*` (e.g., `v0.1.0`)

**Purpose:** Build release binaries for distribution

**Jobs:**
- `build-linux`: Build on Ubuntu for Linux x86_64
- `build-macos`: Build on macOS for macOS x86_64
- `build-windows`: Build on Windows for Windows x86_64

**Artifacts:** Each job uploads a platform-specific binary

**System Dependencies (Linux only):**
- `libasound2-dev` - Audio support (rodio)
- `libudev-dev` - Gamepad support (gilrs)
- `libxcb-*-dev` - X11 support (winit/wgpu)
- `libxkbcommon-dev` - Keyboard support
- `libssl-dev` - TLS support

---

## Why Automatic Builds are Disabled on Push to Main

Automatic builds on every push to `main` have been **intentionally disabled** because:

1. **System dependencies required**: The project uses GUI (wgpu/winit), audio (rodio), and gamepad (gilrs) crates that require system libraries not present in standard GitHub Actions images
2. **Build time**: Full builds with all dependencies take significant time
3. **Resource efficiency**: Manual releases are more appropriate for this stage of development

---

## How to Trigger a Release Build

### Option 1: Manual Workflow Dispatch
1. Go to the [Actions tab](../../actions)
2. Select "Release Build" from the workflow list
3. Click "Run workflow"
4. Enter a version string (e.g., `v0.1.0`)
5. Click "Run workflow"

### Option 2: Git Tag
```bash
git tag v0.1.0
git push origin v0.1.0
```

This will automatically trigger the release workflow.

---

## Re-enabling Automatic Builds on Push to Main

If you want to re-enable automatic builds on every commit to `main`, uncomment these lines in `ci.yml`:

```yaml
on:
  push:
    branches: [ "main" ]
  pull_request:
    branches: [ "main" ]
```

**Note:** Ensure your GitHub Actions runner has sufficient time/resources for builds.

---

## Local Development

These workflows replicate checks you should run locally before pushing:

```bash
# Format code
cargo fmt --all

# Run linter
cargo clippy --all-targets --all-features

# Build
cargo build

# Run tests
cargo test
```

The project uses **lefthook** for git hooks that automatically run `cargo fmt` and `cargo clippy` on commit, and `cargo test` on push.
