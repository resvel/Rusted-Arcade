## Required Startup Context

Before doing any work, read these files in order:

1. `docs/llm-wiki/current-state.md`
2. `docs/llm-wiki/index.md`
3. `docs/llm-wiki/repo-overview.md`
4. Any relevant page linked from the index.

Then inspect the actual source files related to the task.


# Agent Instructions

This repo uses an LLM-maintained project wiki at:

`docs/llm-wiki/`

The wiki is persistent project memory. The source code remains the source of truth.

## Before Making Changes

1. Read `docs/llm-wiki/index.md`.
2. Read `docs/llm-wiki/repo-overview.md`.
3. Read any relevant architecture, bug, core, or decision pages.
4. Inspect the actual source files before editing.
5. Do not rely only on the wiki. Code wins.

## During Debugging

When debugging, maintain a clear chain of evidence:

- observed symptom
- command used
- output/error
- relevant files inspected
- hypothesis
- change made
- result
- next step

Prefer small, testable changes.

## After Meaningful Work

Update the wiki when you learn something durable.

Always update:

- `docs/llm-wiki/log.md`

Also update any relevant page under:

- `docs/llm-wiki/architecture/`
- `docs/llm-wiki/cores/`
- `docs/llm-wiki/bugs/`
- `docs/llm-wiki/debugging/`
- `docs/llm-wiki/decisions/`
- `docs/llm-wiki/external/`

## Session Log Format

Append entries to `docs/llm-wiki/log.md` like this:

```md
## YYYY-MM-DD - Short Title

### Goal

What was the session trying to accomplish?

### Findings

What was learned?

### Changes

What files changed?

### Tests

What commands were run and what happened?

### Next Steps

What should the next assistant do?
