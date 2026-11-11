# Fork changes

This fork of [herdrdev/herdr-core](https://github.com/herdrdev/herdr-core) is the hc build used day to day with the herdr-app fork. It tracks upstream by merging upstream releases or `upstream/master` into `master`. This file is the inventory of what diverges from upstream.

## Changes

### Build identifier

`feat/build-tag` (fork only): a fork build says which commit it is, beside an unchanged version. `crates/herdr-core/build.rs` runs git (`crates/herdr-core/fork_build.rs`) and sets `HERDR_FORK_BUILD` to `fork.<short sha>`, with `.dirty` when tracked files differ from HEAD; without git (a tarball) it is unset. No `Cargo.toml` edit, so upstream syncs stay clean.

- Shown: `hc --version` prints `hc 0.1.20 (fork 8cc5ec1)`; `hc server status` (`server.status`) carries `"build": "fork.8cc5ec1"` next to `"version": "0.1.20"`, or `"build": null` without git. `schema/hc.openrpc.json` gains the field; `schema/released.openrpc.json` is upstream's and stays as it is.
- The version stays upstream's everywhere it is compared or sent: `hello` (apps' compatibility check, remote machines and mirrors), `server.status`'s `version`, the OpenRPC document, `hc update` (`older`, release manifests and downloads), machine installs and `distribution/macos-Info.plist`. Readers of a `--version` line cut a fork build's ` (fork …)`: `cli/update.rs`'s `version_of` (`hc update --from`) and `scripts/mac-bundle` (the bundle's `Info.plist`). `distribution/install.sh` compares only the numbers, and `install.ps1` only prints the line. `hc machine add` and `hc machine update` print the remote hc's whole `--version` line, fork part included: display only, nothing compares it.
- Rebuilds: build.rs reruns when HEAD, its reflog, its branch or `packed-refs` change (paths from `git rev-parse`, so worktrees work), and on edits under `crates/herdr-core/src`. Not on index changes: editors and `git status` rewrite the index, and each rerun recompiles the crate. So an edit elsewhere in the workspace, or a staged-only change, does not mark the build `.dirty` until something else reruns it. Worktrees need their own target dir (the default): Cargo hashes a package by its path within the workspace, so a shared `CARGO_TARGET_DIR` could keep another worktree's build.
- Tests: `fork_build::tests` (the `--version` line, a well-formed build, git in a temporary repository: no git gives none, a commit gives its sha, an edit adds `.dirty`, untracked files do not), `tests/m0.rs` (`server status` keeps the version and has a `build`).
- Files: `crates/herdr-core/{build.rs, fork_build.rs, src/fork_build.rs, src/lib.rs, src/api.rs, src/api/types.rs, src/cli.rs, src/cli/update.rs, tests/m0.rs}`, `schema/hc.openrpc.json`, `scripts/mac-bundle`

### A piped observer ends at end of input

`fix/observe-eof` (fork fix, upstream PR #184): `hc attach --observe` with a stdin that is not a terminal and reaches end of input (`< /dev/null`, a closed pipe) spun at about 100% CPU. At EOF stdin stays readable, so `read_stdin` returned None at once on every turn and the attach loop's `None => {}` waited on it again. The loop now ends on None whether or not stdin is a terminal, as the `detached = !tty` comment intended, and prints `[detached from <address>]` like a closed window. A piped attach without `--observe` spun the same way and now ends the same way. One loop serves Unix and Windows, whose `read_stdin` also returns None at end of input, so both are fixed.

- Tests: `tests/m2.rs` `a_piped_observer_ends_when_its_input_does` (exits within 5 s, success, the terminal stays live).
- Files: `crates/herdr-core/src/attach.rs`, `crates/herdr-core/tests/m2.rs`

### A launch task ends when its agent exits to the shell

Upstream (`72ab4ae`, #166, in 0.1.20) resets a terminal's status when its root interactive shell is back in front (`watch_foreground` in `crates/herdr-core/src/foreground.rs`): an agent typed at a prompt, or started by Herdr's `"$@"; exec "$0" -li` wrapper, no longer leaves its last word (often `blocked`) on the terminal after a crash or Ctrl-C. That reset also drops a pending launch task (`first_task_turn`) but tells nobody, so a parent's `hc wait --children --any` kept waiting when the agent given that task exited without ending a turn.

`c01e095` (fork fix, upstream candidate), on upstream's reset: as when the program exits, a launch task let go without an ended turn is news, so the parent's wait returns.

- Tests: `tests/m7.rs` `a_launch_task_ends_when_the_agent_given_it_exits_to_its_shell` (Herdr's wrapper shape: the agent, then `exec sh -i`); `tests/m5.rs` `a_job_that_exits_to_its_shell_takes_its_status_with_it` (the `status` and `turn` events of the reset), `a_jobs_status_survives_a_core_restart_or_crash_and_still_ends_with_it` (a restart or crash keeps a running job's status, and a crash replay of its old reports does not bring back a reset one).
- Files: `crates/herdr-core/src/foreground.rs`, `crates/herdr-core/tests/{m5.rs, m7.rs}`

### An adoption announces the status it changed

`15f9f14` (fork fix, upstream candidate): adopting a holder without a handover (a core start after a crash, or the relink after reading a holder fails) replayed the tail's OSC 7501 reports into the record but never told subscribers, so `hc get` said one word while event streams kept the old one until they resubscribed. The relink case happens inside a live core with subscribers attached. The core compares the status facts before and after the replay and emits one `status` event with the final facts when they differ, none when the tail only repeats what subscribers already heard. Turn counting and `status_ms` are unchanged.

- Tests: `runtime::tests::adopting_a_holder_announces_a_status_its_tail_changed`, `adopting_a_holder_whose_tail_repeats_the_status_announces_nothing`.
- Files: `crates/herdr-core/src/runtime.rs`, `crates/herdr-core/src/runtime/tests.rs`

## Upstream syncs

### 0.1.20: `3965a46..de97ea5` (`v0.1.20`)

Merged the `v0.1.20` tag (41 commits, releases 0.1.19 and 0.1.20) into the fork's `647bae2`. It includes the range of the unmerged `sync/upstream-2026-11-08` branch (`3965a46..cd12ddd`), whose notes on that range are carried here.

Upstream changes that matter to the fork:

- `72ab4ae` (#166) status resets when the root interactive shell is back in front (see above). Unix only; Windows and Unix PowerShell keep the old rule.
- `287d6ab` (#182) is the fork's `1af54fa` (Ghostty's xcframework skipped on Apple targets, `crates/ghostty-vt/build.rs`), upstreamed: the merged file is upstream's, identical in effect.
- `de6cc39` ending a terminal hangs up its program and foreground job instead of hunting down detached ones, so `nohup` and `setsid` programs keep running. It adds `PtyIo` (`platform/unix.rs`): the holder's master is non-blocking and each read and write polls a stop pipe first.
- `3ba03e6`, `c72b82d`, `27fad02` agent messages (`send`, notes, a launch task) wait while a person has an unfinished draft at the prompt, up to `draft_wait`; a cancelled send reports so. Every viewer key goes through the draft tracker in `crates/herdr-core/src/input.rs`.
- `f4ca1c0` (#144) `hc attach` keeps a multi-byte character the first paint cut in half. Beside the fork's piped-observer end in `attach.rs`; both stay.
- `c6b316f`, `cd12ddd`, `ef9599a` a planned restart hands the old core's environment to the new one (best effort, single use, three minutes); `hc server restart --fresh` takes the caller's instead.
- `ee9a0f0`, `513948a`, `ce81486`, `10f012e`, `ce93650` macOS: holders register as background apps, the launchd core starts on demand, a login refreshes the spawning context and a stale login environment; autostart waits for the old core's lock.
- Also: systemd 239 (#135), Windows path and `via` fixes, `wait --output` across chunks (#129), `--off` reports (#125), notes to a lost parent (#124), untagged remote sends (#119), machine-name and ssh-target fixes, the core setting every terminal's `TERM` (`cd88285`).

Choices:

- Conflicts: `crates/herdr-core/src/foreground.rs` and `crates/herdr-core/src/SKILL.md`, both from upstream's #166 meeting the fork's `fix/status-on-exit`.
- Status on exit converges on upstream. The fork's rule (`2cbdda3`, `64d73e2`, `c01e095`, `45dd13d`) tracked the reporting process group (`reporter`, `status::foreground_seen`, `status::lapsed`, `platform::group_gone`) and reset when that group had ended. Upstream's rule needs no saved state and covers the cases that matter more day to day: a job that reports and exits between two polls (the fork took it for the program and kept its status), and Herdr's `-lic` wrapper once it has become the interactive shell. The fork's rule caught some cases upstream's misses, a job ending back at a nested shell or under a `sh -c` root, and kept a stopped (Ctrl-Z) job's or the shell's own status where upstream clears them; those are not worth a second rule beside upstream's. Removed: the `reporter` field (an old saved record's `reporter` key is ignored on load), `foreground_seen`, `lapsed`, `group_gone` and the crash-replay reporter handling in `runtime.rs`, with `status::tests::a_job_that_reported_takes_its_facts_with_it` and the m5 tests that pinned the fork-only behaviour (`a_crash_does_not_blame_a_new_job_for_an_old_ones_exit`, `a_job_keeps_its_status_while_it_runs_its_own_tools_or_waits`, `the_programs_own_status_outlasts_its_jobs_and_ends_with_it`). Kept: the launch-task news (`c01e095`) on upstream's reset, and the m5 and m7 tests rewritten for an interactive root shell. `SKILL.md`, `README.md` and `schema/README.md` are upstream's.
- `runtime.rs`: the fork's adoption announce (`15f9f14`) stays, without the reporter lines. Its test builds a `holder_proto::Welcome`, which gains upstream's `login_session` and `draining` fields.
- Tests (macOS, CI's shape; Linux in `rust:1.95-trixie` as a non-root user): fmt, clippy `-D warnings` (native and Windows with zig stubbed), and the suite pass, except upstream's own. On macOS, `tests/foreground.rs` `a_shell_that_execs_an_agent_keeps_its_status` and `a_shell_script_root_keeps_its_agents_status` fail on plain `v0.1.20` too: they read the status as soon as the agent is in front, before its report arrives (with a 1 s wait it passes). That is a race in the tests, and an upstream candidate. Linux passes both. `platform::environment_tests::a_restart_refreshes_only_a_dead_saved_ssh_agent_socket` and `tests/keep.rs` `late_initial_spawn_failure_is_handled_after_timeout_even_after_keep_off` failed once under load and pass alone. On Linux, `tests/macos_distribution.rs`'s two notarization tests need `jq`, which the container lacks.
- Known cost, kept: `PtyIo`'s poll before every read adds one `poll` per read in the holder. Measured on the 2026-11-08 sync against the pre-sync build, over 8 alternating isolated passes each: holder syscalls rise 15–20% (+49% for a no-viewer 15 MB `cat`) and holder CPU rises 6–8% under sustained output (median, 7 of 8 pairs). Core CPU falls 2–10%, so total CPU rises 0–5%. Trying the read first and polling only on `WouldBlock` is worse (+47–76% syscalls), because the holder usually outpaces the program. A wake-based alternative is upstream PR #202 (`up/ptyio-blocking-wake`), not part of this merge.
