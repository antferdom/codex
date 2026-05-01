# `/loop` vs `/goal`

This note summarizes the `/loop` feature and how it differs from the newer `/goal` system.

## Executive Summary

`/loop` is still relevant, but mainly as an experimental TUI feature and as a compact example of adding a custom Codex command. It solves a different problem from `/goal`: it repeats one concrete prompt on a timer until Codex creates an internal completion marker.

`/goal` is the more durable long-running-work primitive. It is persisted, protocol-visible, state-db backed, and integrated into core runtime accounting and continuation behavior.

The practical conclusion is:

- Use `/goal` for product-grade long-running objectives.
- Use `/loop` for timer-based retry experiments, polling-style tasks, and learning how to wire a custom TUI command end to end.
- Do not treat `/loop` as a replacement for `/goal`; if it graduates, it should likely become a scheduling/retry layer around goals or structured tasks.

## What `/loop` Adds

`/loop [<interval>] [--timeout <timeout>] <task>` arms a local retry loop in the TUI.

Example:

```text
/loop 5s --timeout 45s If READY exists, create RESULT.txt and verify it. If READY does not exist, report that you are waiting.
```

Behavior:

- Parses optional interval and timeout arguments.
- Defaults to `10m` retry interval and `1h` timeout.
- Sends the user task to the model with hidden loop instructions appended.
- Creates a private completion marker path under the system temp directory.
- Tells the model to touch that marker only when the task is truly complete.
- Resubmits the same user task after each interval while the marker is missing.
- Stops when the marker exists, the timeout expires, the user submits a non-loop task, or the active turn is interrupted.

The feature is intentionally simple: the TUI owns the retry state and uses the filesystem marker as the completion signal.

## Testing Notes

Run the dev build directly from the Rust workspace so you do not accidentally test a system-installed Codex:

```bash
cd /home/antonio/codex-dev/codex-rs
cargo build -p codex-cli --bin codex
./target/debug/codex -C /tmp/codex-loop-e2e --dangerously-bypass-approvals-and-sandbox --no-alt-screen
```

Then submit a short TUI command:

```text
/loop 5s --timeout 30s Create RESULT.txt containing exactly "loop smoke ok", verify it with cat RESULT.txt, and then finish.
```

Three practical caveats matter when testing:

- `/loop` is a TUI slash command. It is not expected to work through `codex exec "/loop ..."` because `exec` sends text directly as a model prompt rather than dispatching TUI slash commands.
- If you paste the whole command into the TUI and press Enter immediately, the composer can treat that Enter as part of a paste burst and insert a newline instead of submitting. Wait a moment after pasting, then press Enter.
- The loop only stops when the internal marker file exists. If the model reports that it is done but does not run the internal marker command, `/loop` will retry until the timeout. The hidden instruction now explicitly tells Codex to run that marker command as the final action after the task is truly complete.

Correct-dispatch signs:

- The transcript shows a `Loop armed: ...` info message immediately after submission.
- The user turn sent to the model no longer starts with `/loop`; it starts with the task text and includes the appended `This task is running in /loop mode...` instructions.
- If the prompt is recorded literally as `/loop 3s ...`, then the modified TUI did not handle the slash command. That usually means the test is running in a different Codex binary/session than `codex-dev/target/debug/codex`.

## What `/goal` Provides

`/goal` sets or manages the active persisted objective for a thread.

Behavior:

- Stores a thread goal in the state DB.
- Exposes goal updates through protocol events and app-server APIs.
- Supports status transitions such as active, paused, budget-limited, complete, and clear.
- Tracks token and wall-clock usage against the goal.
- Can continue work when the thread becomes idle.
- Uses structured completion through `update_goal`, rather than a temp-file marker.
- Survives thread persistence/resume because the goal is part of thread state.

`/goal` is not just a slash-command convenience. It is a core runtime feature.

## Key Differences

| Dimension | `/loop` | `/goal` |
| --- | --- | --- |
| Primary purpose | Retry one prompt on a timer | Track and drive a long-running objective |
| Owner | TUI-local state | Core runtime plus state DB |
| Persistence | Not durable beyond TUI/input-state lifetime | Persisted per materialized thread |
| Completion signal | Internal marker file touched by Codex | Structured `update_goal(status = complete)` |
| Scheduling | Fixed interval retry | Core continuation when idle and goal is active |
| Accounting | No usage accounting | Token and wall-clock accounting |
| Protocol/API surface | None beyond normal user turns | App-server methods and goal update events |
| Failure mode | May repeat noisy prompts until timeout | Can pause, budget-limit, clear, or complete |
| Best fit | Polling, retry experiments, custom command learning | Real long-running task management |

## Relevance After `/goal`

`/loop` remains useful because timed retry is not the same abstraction as a goal.

Useful cases:

- Polling for a local condition, file, build artifact, or external readiness signal.
- Running a task that should retry after a delay without user retyping.
- Prototyping custom slash commands in the TUI.
- Studying the full path from slash command parsing to model-submission mutation, chat history rendering, input-state preservation, and tests.

Less appropriate cases:

- Work that must survive process restarts.
- Work that needs status, accounting, pause/resume, or external API visibility.
- Work where repeated prompts could be costly or confusing.
- Product surfaces where completion should be auditable through structured state rather than a file marker.

## Implementation Shape

The port has four main pieces:

- `loop_mode.rs`: argument parsing, duration formatting, marker creation, hidden model instruction construction, and text-element rebasing.
- `slash_command.rs`: registers `/loop`, marks it as accepting inline args, and disables it while another task is running.
- `chatwidget/slash_dispatch.rs`: parses `/loop` args, creates loop state, renders the "Loop armed" notice, and submits or queues the task.
- `chatwidget.rs`: stores active loop state, schedules redraw wakeups, retries when due, injects hidden completion instructions, and clears loop state on interruption or replacement by normal user input.

One historical part of commit `59e4623` was not ported: the old `codex-rs/tui_app_server` mirror. Current `codex-dev` no longer has that crate, and the modern app-server split does not duplicate TUI slash-command widgets in the same way.

## Design Assessment

The feature is technically coherent and small enough to be worth keeping for experimentation. The strongest part is that it demonstrates a complete custom-command integration without touching core protocol design.

The weakest part is the completion marker. It is pragmatic, but it is not a durable contract. It relies on prompt compliance, local filesystem access, and an invisible side effect. That is acceptable for an experimental TUI retry loop; it would be weak as a product-grade long-running-task system.

The best future direction is not to expand `/loop` into a second goal system. If it proves useful, fold its scheduling idea into a structured mechanism: for example, a goal option, a task scheduler, or a command that creates a goal plus a retry policy. That would preserve `/goal` as the source of truth while keeping the useful timer behavior.
