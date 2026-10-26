# runs

## Overview

The submodules of `src/runs.rs`, which carries out runs' work
(`ARCHITECTURE.md` §8). The Supervisor steps each run's state machine and
stores every transition; the work a step asks for (start a container, an
agent, the checks; push and open a pull request; a review) is handed to a
`RunWork`, which does it off the Supervisor's loop and reports back. Reports
name the generation they were asked under, so the machine ignores overtaken
ones. Every piece of work is safe to ask for again after a restart.

## Interface

From the module root `src/runs.rs`:

- `RunWork` (runs.rs) — the trait the Supervisor hands work to (`ARC-03`).
- `RunContext` (runs.rs) — what a piece of work is given; `overtaken(generation)`.
- `Report`, `Reports` (runs.rs) — results sent back as `(RunId, Report)` on an
  unbounded channel.
- `NotYetBuilt` (runs.rs, test-only) — a `RunWork` that refuses everything.

In this directory:

- `DockerWork` (work.rs) — the real `RunWork`: mirror, container, jobs,
  bundle, push, pull request, review. Agent, check and review steps:
  [work/README.md](./work/README.md).
- `provision`, `Provisioned`, `container_name`, `STATE_DIR`, `Identity`,
  `identity`, `last_lines` (provision.rs) — find or make a run's container
  (memory limit, mirror mounted read-only) and clone the mirror on the run's
  branch.
- `Job`, `JobState`, `Progress`, `stop_before` (job.rs) — work in a container
  that outlives docker and the daemon (`ARC-04`), with files `.in`, `.runner`,
  `.pid`, `.out`, `.err`, `.exit`.
- `Mirror`, `refresh`, `path` (mirror.rs) — Muster's bare clone of each
  project; `take_bundle`, `push` one branch with the person's git credentials.
- `image_for`, `claude_version` (image.rs) — the project's Dockerfile or
  Muster's base image. A `MUSTER_RUN_IMAGE` override is read in
  `lib.rs::run_work` and kept as `Inner::image`, not here.
- `Credentials`, `PersonsCredentials`, `Secret`, `KEYCHAIN_SERVICE`,
  `KEYCHAIN_ACCOUNT`, `MISSING` (credentials.rs; `FixedCredentials` is
  test-only) — the Claude Code token from the macOS Keychain or the daemon's
  environment.
- `builder_first`, `builder_again`, `reviewer`, `review_by_hand`,
  `tidy_by_hand`, `ReviewVerdict`, `Decision` (prompts.rs) — what agents are
  told, and the reviewer's JSON verdict.

## Invariants

- Every piece of work is idempotent: a container is found by label before one
  is made, a clone is made once, a branch with the run's work is not reset.
- Reports from an overtaken generation are ignored; work stops once overtaken.
- Containers clone from a read-only mount of the mirror and cannot write to
  the host (`PRD-04`); branches come out as bundles and are pushed from the
  host with the person's git credentials, which never enter a container
  (`ARC-06`).
- The mirror is never a `--mirror` clone and pushes only the one named branch.
- The agent credential goes only into an agent `exec`'s environment, via
  docker's environment, never its arguments, and is never printed (`PRD-06`).
- Ticket text is data, not instructions: fenced by a random tag in the prompt
  (`PRD-07`). The builder never pushes (`ARC-06`).
- A job's process id is trusted only if the process carries `MUSTER_JOB` with
  the job's name.

## Side Effects

- Docker: builds images, creates and starts containers, `docker exec`s jobs.
- Git: fetches into and pushes from the bare mirror under the data directory.
- Reads the macOS Keychain (service `muster`) for the agent credential.
- Opens pull requests through the git host (`crate::githost`).
- Sends `Report`s to the Supervisor.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon runs`
- End to end: `cargo nextest run -p muster --test runs --test docker_runs`
- Real Docker (before a release): `MUSTER_DOCKER_TESTS=1 cargo test -p muster-daemon`
