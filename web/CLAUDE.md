<!--VITE PLUS START-->

# Using Vite+, the Unified Toolchain for the Web

This project is using Vite+, a unified toolchain built on top of Vite, Rolldown, Vitest, tsdown, Oxlint, Oxfmt, and Vite Task. Vite+ wraps runtime management, package management, and frontend tooling in a single global CLI called `vp`. Vite+ is distinct from Vite, and it invokes Vite through `vp dev` and `vp build`. Run `vp help` to print a list of commands and `vp <command> --help` for information about a specific command.

Docs are local at `node_modules/vite-plus/docs` or online at https://viteplus.dev/guide/.

## Built-in Commands vs Scripts

`vp <name>` runs a built-in command. `vp run <name>` runs a `package.json` script or a `vite.config.ts` task. Scripts cannot overwrite built-ins, so `vp dev` and `vp run dev` may do different things. Check `package.json` and `vite.config.ts` first, and run `vp run <name>` when the project defines a script or task with that name.

## Tool Versions

Run `vp toolchain` to show versions and relationships in the active Vite+
release. Add a tool name to select part of the graph. For example, run
`vp toolchain vite`. Use `--global` to ignore the local `vite-plus` package. Use
`vp why <package>` to show the package-manager dependency graph.

## Review Checklist

- [ ] Run `vp install` after pulling remote changes and before getting started.
- [ ] Run `vp check` and `vp test` to format, lint, type check and test changes.
- [ ] Check if there are `vite.config.ts` tasks or `package.json` scripts necessary for validation, run via `vp run <script>`.
- [ ] If setup, runtime, or package-manager behavior looks wrong, run `vp env doctor` and include its output when asking for help.

<!--VITE PLUS END-->

## Seed Seeker web app

This directory is the Vite+ project root. Run every toolchain command from `web/`, and use `vp`
rather than `pnpm`, `npm`, `vite`, or `vitest` directly. The package manager is pnpm, pinned through
`devEngines.packageManager`; `vp` downloads the pinned version itself, so nothing has to be
installed globally beyond `vp`. There is no `package-lock.json` — `pnpm-lock.yaml` is the lockfile.

| Task                     | Command                                    |
| ------------------------ | ------------------------------------------ |
| Install dependencies     | `vp install`                               |
| Dev server               | `vp dev`                                   |
| Format, lint, type-check | `vp check` (`vp check --fix` to autofix)   |
| Tests                    | `vp test` (`vp test watch` for watch mode) |
| Production build         | `vp build`                                 |
| Preview the built app    | `vp preview`                               |

`package.json` deliberately declares no `scripts`: every command above is a `vp` built-in, so there
is nothing for `vp run` to run here.

Before `vp check`, `vp test`, or a build on a fresh clone, generate the browser engine and its
assets from the repository root:

```sh
./scripts/build-web-wasm.sh
```

That script writes `src/lib/wasm/pkg/`, `src/generated/`, and `public/third_party/`, all of which
are gitignored and required to type-check and build. Re-run it whenever the Rust engine changes.

### Conventions

- Tests import Vitest APIs from `vite-plus/test`, not `vitest`. The
  `vite-plus/prefer-vite-plus-imports` lint rule enforces this.
- Dependency versions are pinned through the pnpm catalog in `pnpm-workspace.yaml`, which also
  carries the `vite` -> `@voidzero-dev/vite-plus-core` override. Use `vp add` / `vp remove` rather
  than editing `package.json` by hand.
- All tool configuration lives in `vite.config.ts` (`fmt`, `lint`, `test` blocks). Do not add
  `.oxfmtrc.json`, `.oxlintrc.json`, or `vitest.config.ts`.
- Type checking runs through tsgolint inside `vp check`; there is no separate `tsc` step.
- Formatting is Oxfmt with its defaults; `vp check` rewrites files, so run it before committing.
