# P0-D6 Non-Interactive Security Migration

Last updated: 2026-02-26

## 1) Scope

This note finalizes `P0-D6` for daemon/gRPC/CI channels:

- Clarify security gateway defaults and override semantics.
- Provide a migration profile for non-interactive workloads.
- Define verifiable rollout checks.

## 2) Runtime Behavior Matrix

`NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY=true` (default):

- `shell` critical risk: hard deny (cannot be approved by ask flow).
- `shell` high risk: ask.
- `shell` medium risk: controlled by `NDC_SECURITY_MEDIUM_RISK_ACTION`.
- `git commit`: controlled by `NDC_SECURITY_GIT_COMMIT_ACTION`.
- file path outside project root: controlled by `NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION`.

Ask response format:

- `requires_confirmation permission=<permission> risk=<risk> <detail>`

## 3) Channel Semantics

Interactive REPL:

- `ask` can be confirmed in terminal.
- single-call override is applied through runtime security overrides.

Non-interactive daemon/gRPC/CI:

- no stdin prompt is attempted.
- request fails fast with `non_interactive confirmation required: ...`.
- permission lifecycle events are still visible in timeline replay/subscription.

## 4) Recommended Profiles

Profile A (`local dev`, default-safe):

- `NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY=true`
- `NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION=ask`
- `NDC_SECURITY_MEDIUM_RISK_ACTION=ask`
- `NDC_SECURITY_GIT_COMMIT_ACTION=ask`

Profile B (`CI smoke`, deterministic allow for known cases):

- keep gateway enabled.
- use targeted overrides in test commands, not global disable:
  - `NDC_SECURITY_OVERRIDE_PERMISSIONS=external_directory,git_commit`
- avoid broad `NDC_AUTO_APPROVE_TOOLS=1` except in controlled test jobs.

Profile C (`service prod`, strict):

- `NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY=true`
- `NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION=deny`
- `NDC_SECURITY_MEDIUM_RISK_ACTION=ask` (or `deny` for hardened setups)
- `NDC_SECURITY_GIT_COMMIT_ACTION=deny` unless explicitly required

## 5) Migration Checklist

1. Pin project root per deployment unit:
   - `NDC_PROJECT_ROOT=<absolute project path>`
2. Apply one of the profiles above.
3. Run timeline checks:
   - verify permission events are present in gRPC/SSE replay.
4. Run non-interactive failure check:
   - ensure `ask` returns `non_interactive confirmation required`.
5. Roll forward only after checks pass.

## 6) Validation Commands

```bash
cargo test -q -p ndc-runtime security::
cargo test -q -p ndc-interface --features grpc
```

## 7) Notes for Tests

In `cfg(test)`, runtime security action defaults are intentionally relaxed to `allow`
unless env vars are set, to prevent unrelated unit tests from failing due to global
policy gates. Security-focused tests must set explicit env values.
