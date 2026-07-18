# `t-0` (npm)

Thin bootstrap for [T-0](https://t-0.dev) on macOS.

```bash
npx t-0
```

- If `~/.t-0/bin/t0` exists and the local stack responds → opens the product URL.
- Otherwise downloads `https://t-0.dev/install` (GitHub fallback) and runs it with bash.

This package does **not** ship the PTY server or launcher sources. Those stay in the [t-0 repo](https://github.com/aatosolavi/t-0).

## Publish (maintainers)

Publishing is tagged-release only (see `.github/workflows/npm-publish.yml`).

1. Create an npm access token with publish rights for the `t-0` package.
2. Add repo secret `NPM_TOKEN`.
3. Tag a release matching `npm/t-0/package.json` version, e.g. `git tag v0.3.0 && git push origin v0.3.0`.
