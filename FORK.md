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

### A job's status ends with the job

`fix/status-on-exit` (fork fix, upstream candidate): an agent started from a terminal's shell (`claude`, `codex` typed at the prompt) reports through OSC 7501, but only the terminal's own program ending reset the status. An agent that exited back to its shell without a last report (a crash, Ctrl-C, `/exit` before `idle`) left its final word, often `blocked`, on the terminal for good: Herdr kept showing "Needs you", counting it in the Dock badge and holding its notification.

- Rule: a status lasts as long as what reported it. The foreground process group first seen after a report (`foreground_seen` in `crates/herdr-core/src/status.rs`) made it: the terminal's program, or a job its shell runs. A job's status resets once every process in that job's group has ended and something else is in front, with the same `status` and `turn` events and news as a program exit, and a pending launch task is let go as on exit, with news when no turn ended, so a `wait --children --any` returns. The program's own status still waits for its exit. A job that lives keeps its status: running its own tools (in its group or in front as their own job), stopped with Ctrl-Z, or idle.
- Records: child records (`id`) go with the root one, as they do on exit; the reporter is whoever made the latest report of any record. A `state=clear` report forgets it.
- Detection: the existing foreground poll (`watch_foreground`, every `HC_FOREGROUND_MS`, 500 ms by default) names the reporter and checks it; no new timer. `platform::group_gone` is `kill(-pgid, 0)` failing with `ESRCH` on Unix: the whole group, since a pipeline's first command (its leader) can end long before the agent in it, and a process hc may not signal counts as running. On Windows a pseudo console has no foreground groups, so the foreground is always the program and the rule never fires; `group_gone` there is `false`, keeping the old behaviour.
- Restarts: the reporter is a private saved field (`reporter`, kept out of the API). A core restart's handover re-reads fresh reports and names the reporter again; a crash replay that repeats the same word keeps it only while its group still runs, since another job may have said that word after it ended (a missed reset is better than a wrongful one; the next poll names whoever is in front). Pids survive both, so adopted terminals keep their status until the job really ends. After a lapse the reporter stays set with no facts (`status::lapsed`): the holder's tail still holds the ended job's reports, and a crash replay must not bring them back. Reports made while the core was down after that are lost, as a crash replay already accepts.
- Limits: the reporter is whatever is in front at the first poll after the report. A job that reports and exits before then is taken for the program, which keeps the old behaviour; a program that puts another job in front right after reporting has that job taken for the reporter. Likewise, a job that reports from the background (`agent &`) while another job is in front gets that job recorded, and its status may reset when that job exits. A reused pid keeps the status until it ends too. A job sharing the program's process group (no job control) is the program's.
- Tests: `status::tests::a_job_that_reported_takes_its_facts_with_it`; `tests/m7.rs` `a_launch_task_ends_when_the_job_given_it_exits_to_its_shell`; `tests/m5.rs` `a_crash_does_not_blame_a_new_job_for_an_old_ones_exit`, `a_job_that_exits_to_its_shell_takes_its_status_with_it`, `a_job_keeps_its_status_while_it_runs_its_own_tools_or_waits`, `the_programs_own_status_outlasts_its_jobs_and_ends_with_it`, `a_jobs_status_survives_a_core_restart_or_crash_and_still_ends_with_it`.
- Files: `crates/herdr-core/src/{status.rs, foreground.rs, terminals.rs, runtime.rs, platform/unix.rs, platform/windows.rs, SKILL.md}`, `crates/herdr-core/tests/{m5.rs, m7.rs}`, `README.md`, `schema/README.md`

### Ghostty's xcframework is skipped

`1af54fa` (fork fix, upstream candidate): with only the Command Line Tools installed, `/usr/bin/xcodebuild` exists but fails, and Ghostty builds its xcframework whenever it finds xcodebuild, so the libghostty-vt build broke. hc links only the static library, so `crates/ghostty-vt/build.rs` passes `-Demit-xcframework=false` on Apple targets.

- Files: `crates/ghostty-vt/build.rs`

### An adoption announces the status it changed

`15f9f14` (fork fix, upstream candidate): adopting a holder without a handover (a core start after a crash, or the relink after reading a holder fails) replayed the tail's OSC 7501 reports into the record but never told subscribers, so `hc get` said one word while event streams kept the old one until they resubscribed. The relink case happens inside a live core with subscribers attached. The core now compares the status facts before and after the replay and emits one `status` event with the final facts when they differ, none when the tail only repeats what subscribers already heard. Turn counting and `status_ms` are unchanged.

- Tests: `runtime::tests::adopting_a_holder_announces_a_status_its_tail_changed`, `adopting_a_holder_whose_tail_repeats_the_status_announces_nothing`.
- Files: `crates/herdr-core/src/runtime.rs`, `crates/herdr-core/src/runtime/tests.rs`

## Upstream syncs

### 2026-11-08: `3965a46..cd12ddd`

Merged `upstream/master` (10 commits) into the fork's `647bae2`. Every fork change above is kept.

- `c6b316f` a planned restart (`hc update`, `server restart`) hands the old core's environment to the new one (#21).
- `de6cc39` ending a terminal hangs up its program and foreground job instead of hunting down detached ones, so `nohup` and `setsid` programs keep running, background servers among them (their ports stay busy); an unexpected holder loss still ends them. A core start no longer finishes kept records already waiting for a shell as `stopped_by_restart` (`attach_all`), and terminals kept through an update keep their old holder's behaviour until recreated.
- `aef2dac` the stitch test waits for the final wheel repaint.
- `ad23942` a starting core or holder gets 15 s before the client reports it failed.
- `f4ca1c0` `hc attach` keeps a multi-byte character the first paint cut in half (#144).
- `c0a205e` Windows expands references such as `%ProgramFiles%` left in the system path before spawning terminals.
- `72f77c4` a terminal command named `hc` or `herdr-core` falls back to the serving core when absent from the terminal's path.
- `9ce385d` reverts `72f77c4`.
- `1fe7bc6` Linux service startup and planned restarts work on systemd 239 (#135).
- `cd12ddd` the saved restart environment is best effort (a failed save still restarts), expires after three minutes and is used once.

Choices:

- No textual conflicts. Eleven files changed on both sides (`attach.rs`, `platform/unix.rs`, `platform/windows.rs`, `runtime.rs`, `terminals.rs`, `api.rs`, `cli.rs`, `lib.rs`, `runtime/tests.rs`, `SKILL.md`, `README.md`), in separate hunks; git's merge is kept as is.
- `attach.rs`: upstream's `shown()` fix (#144) and the fork's piped-observer end (`detached = !tty`) touch different functions; both stay.
- `platform/unix.rs`: upstream's `PtyIo`, which makes the master non-blocking and polls a stop pipe before each read and write, sits beside the fork's `group_gone`. Status-on-exit does read the PTY: the foreground poll asks the holder, which calls `tcgetpgrp` on the master (`holder.rs` `FOREGROUND` → `Pty::foreground` in `platform/unix.rs`). That call goes to the master fd directly, not through `PtyIo`, and `O_NONBLOCK` does not change `tcgetpgrp`, so the rule is unaffected.
- No duplicates to drop: no upstream commit in this range fixes the fork's xcframework build, observe EOF, crash-adoption status announce or status-on-exit, so all four fork fixes stay.
