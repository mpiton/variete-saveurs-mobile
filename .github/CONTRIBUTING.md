# Contributing to variete-saveurs-mobile

First off, thanks for considering contributing! Every contribution matters, whether it's a bug report, a feature request, or a pull request.

## How to Contribute

### Reporting Bugs

1. Check if the bug has already been reported in [Issues](https://github.com/mpiton/variete-saveurs-mobile/issues)
2. If not, create a new issue using the **Bug Report** template
3. Include steps to reproduce, expected behavior, and actual behavior

### Suggesting Features

1. Check existing [Feature Requests](https://github.com/mpiton/variete-saveurs-mobile/issues?q=label%3Aenhancement)
2. Open a new issue using the **Feature Request** template
3. Describe the problem and your proposed solution

### Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/your-feature`)
3. Make your changes following the project's coding standards
4. Write or update tests as needed
5. Commit using [Conventional Commits](https://www.conventionalcommits.org/) format
6. Push to your fork and open a Pull Request

### Commit Message Format

```
<type>(<scope>): <description>

[optional body]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`, `ci`

### Development Setup

```bash
# Clone the repo
git clone https://github.com/mpiton/variete-saveurs-mobile.git
cd variete-saveurs-mobile

# Toolchain — dx is pinned to the dioxus crate version, it generates the
# Gradle project and assembles the APK
rustup target add aarch64-linux-android
cargo install dioxus-cli@0.7.9 --locked

# Run on a device or emulator
dx serve --platform android
```

### Quality Gates

The same five gates run in CI and must be green before a PR merges. No Android
build in CI — the APK is built locally, where the signing keystore lives.

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo llvm-cov --locked --fail-under-lines 85 \
  --ignore-filename-regex 'src/(ui|platform)/|src/main\.rs|tests/'
cargo audit        # RustSec advisories, any vulnerability fails the build
cargo deny check   # licenses, advisories, bans, sources
```

Coverage is scoped to `src/domain/`. `src/ui/` (RSX) and `src/platform/` (JNI
bridges) are excluded and covered by a manual pass on the phone before release.

A new dependency needs a justification in the PR — what it does, and which std
or already-present crate was ruled out. Prefer `default-features = false`.
`CHANGELOG.md` is updated in the same PR, under `[Unreleased]`.

## Code of Conduct

Be decent to each other. Harassment, personal attacks and bad faith get you
removed from the discussion, no policy document needed.

## Questions?

Open a [Discussion](https://github.com/mpiton/variete-saveurs-mobile/discussions) or file an issue using the **Question** template.
