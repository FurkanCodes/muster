# runs/work

## Overview

The submodule of `src/runs/work.rs` that does the work carried out by jobs in
a run's container: the building agent's turns, the project's checks, and the
reviewer. Each follows its job (`../job.rs`) until it ends, the container
stops under it, or the run moves past the generation it was asked under. The
`DockerWork` type that calls these is defined in `src/runs/work.rs`; see
[../README.md](../README.md).

## Interface

These are methods on `Inner`, the private state `DockerWork` wraps in an
`Arc` (`work.rs`):

- `Inner::build` (agents.rs) — runs a headless builder agent turn in the
  container and reports its outcome and cost.
- `Inner::check` (agents.rs) — runs the project's checks as a job and
  reports pass or fail with the last lines of output.
- `Inner::review` (agents.rs) — runs the reviewer agent, parses its JSON
  verdict (`ReviewVerdict` in `../prompts.rs`) and posts it on the pull
  request.

All three are `pub(super)`: only `work.rs` calls them.

## Invariants

- Work overtaken by a newer generation stops quietly and starts nothing more.
- Each step can be asked for again after a daemon restart and resumes the job
  already started rather than starting a second one (`ARC-04`).
- The agent credential reaches only the agent's `exec` environment, never the
  container's or the checks' (`PRD-06`).

## Side Effects

- `docker exec` into the run's container to start and follow jobs.
- Reads job output files under the container's state directory.
- `review` posts the verdict as a pull-request review comment through the git
  host (`host.review_comment`).
- Sends `Report`s back to the Supervisor over its channel.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon runs::work`
- End to end: `cargo nextest run -p muster --test runs --test docker_runs`
- Real Docker (before a release): `MUSTER_DOCKER_TESTS=1 cargo test -p muster-daemon`
