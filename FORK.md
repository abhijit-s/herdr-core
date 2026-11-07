# Fork changes

This fork of [herdrdev/herdr-core](https://github.com/herdrdev/herdr-core) is the hc build used day to day with the herdr-app fork. It tracks upstream by merging `upstream/master` into `master`. This file is the inventory of what diverges from upstream.

## Changes

### Build identifier

`feat/build-tag` (fork only): a fork build says which commit it is, beside an unchanged version. `crates/herdr-core/build.rs` runs git (`crates/herdr-core/fork_build.rs`) and sets `HERDR_FORK_BUILD` to `fork.<short sha>`, with `.dirty` when tracked files differ from HEAD; without git (a tarball) it is unset. No `Cargo.toml` edit, so upstream syncs stay clean.

- Shown: `hc --version` prints `hc 0.1.18 (fork 8cc5ec1)`; `hc server status` (`server.status`) carries `"build": "fork.8cc5ec1"` next to `"version": "0.1.18"`, or `"build": null` without git. `schema/hc.openrpc.json` gains the field; `schema/released.openrpc.json` is upstream's and stays as it is.
- The version stays upstream's everywhere it is compared or sent: `hello` (apps' compatibility check, remote machines and mirrors), `server.status`'s `version`, the OpenRPC document, `hc update` (`older`, release manifests and downloads), machine installs and `distribution/macos-Info.plist`. Readers of a `--version` line cut a fork build's ` (fork …)`: `cli/update.rs`'s `version_of` (`hc update --from`) and `scripts/mac-bundle` (the bundle's `Info.plist`). `distribution/install.sh` compares only the numbers, and `install.ps1` only prints the line. `hc machine add` and `hc machine update` print the remote hc's whole `--version` line, fork part included: display only, nothing compares it.
- Rebuilds: build.rs reruns when HEAD, its reflog, its branch or `packed-refs` change (paths from `git rev-parse`, so worktrees work), and on edits under `crates/herdr-core/src`. Not on index changes: editors and `git status` rewrite the index, and each rerun recompiles the crate. So an edit elsewhere in the workspace, or a staged-only change, does not mark the build `.dirty` until something else reruns it. Worktrees need their own target dir (the default): Cargo hashes a package by its path within the workspace, so a shared `CARGO_TARGET_DIR` could keep another worktree's build.
- Tests: `fork_build::tests` (the `--version` line, a well-formed build, git in a temporary repository: no git gives none, a commit gives its sha, an edit adds `.dirty`, untracked files do not), `tests/m0.rs` (`server status` keeps the version and has a `build`).
- Files: `crates/herdr-core/{build.rs, fork_build.rs, src/fork_build.rs, src/lib.rs, src/api.rs, src/api/types.rs, src/cli.rs, src/cli/update.rs, tests/m0.rs}`, `schema/hc.openrpc.json`, `scripts/mac-bundle`

### A piped observer ends at end of input

`fix/observe-eof` (fork fix, upstream candidate): `hc attach --observe` with a stdin that is not a terminal and reaches end of input (`< /dev/null`, a closed pipe) spun at about 100% CPU. At EOF stdin stays readable, so `read_stdin` returned None at once on every turn and the attach loop's `None => {}` waited on it again. The loop now ends on None whether or not stdin is a terminal, as the `detached = !tty` comment intended, and prints `[detached from <address>]` like a closed window. A piped attach without `--observe` spun the same way and now ends the same way. One loop serves Unix and Windows, whose `read_stdin` also returns None at end of input, so both are fixed.

- Tests: `tests/m2.rs` `a_piped_observer_ends_when_its_input_does` (exits within 5 s, success, the terminal stays live).
- Files: `crates/herdr-core/src/attach.rs`, `crates/herdr-core/tests/m2.rs`
