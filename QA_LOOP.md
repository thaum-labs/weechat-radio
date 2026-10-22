# QA loop (Grok QA ↔ Claude dev)

Grok (QA) and Claude (dev) talk through GitHub issues in this repo. Nobody
copies notes by hand. Labels say whose turn it is.

| Label | Set by | Meaning |
|---|---|---|
| `qa-report` | Grok | New QA → DEV note. Waiting for Claude. |
| `dev-working` | Claude | Claude picked it up and is fixing. |
| `needs-tobias` | Claude | Claude has a question only Tobias can answer. |
| `ready-for-qa` | Claude | Fix released. Waiting for Grok to test. |

## Grok (QA)

1. **Report:** open a new issue labelled `qa-report`.
   - Title: `QA → DEV vX.Y.Z` (the version you tested).
   - Body: the usual note (PASSED / FAILED / TASKS, what you tested on, steps,
     logs). One issue per QA round. Tobias's own findings go in the same way.
2. **Questions:** when an issue gets `needs-tobias`, read Claude's last
   comment, ask Tobias, post his answer as a comment starting `Tobias:`, then
   remove `needs-tobias`.
3. **Test:** when an issue gets `ready-for-qa`, read the DEV → QA comment on
   it, test the release it links, then close the issue and file the next
   `qa-report` issue with the results.

## Claude (dev)

A Claude task checks every hour for open issues labelled `qa-report` without
`dev-working` or `needs-tobias`, and for `needs-tobias` issues whose label
Grok has removed.

1. Add `dev-working`. If the note only says PASSED with nothing to fix,
   comment that and close the issue.
2. Fix on a fresh branch off main. One PR with the fixes, the version bump
   (next patch after the latest tag) and the web/changelog.html entry. The PR
   says `Fixes #<issue>`. Merge once the fast CI jobs pass; long timed jobs
   finish on main.
3. If a fix needs a product decision, comment the question, add
   `needs-tobias`, and wait.
4. Release: run the **release** workflow (Actions → release → Run workflow)
   with the new version, e.g. `v0.1.111`. It tags main, builds all 15 assets,
   publishes, checks the assets and syncs the install manifests.
5. When the release run is green, comment the DEV → QA note on the issue
   (release link, what changed, what to test), remove `dev-working` and
   `qa-report`, add `ready-for-qa`. If the release run fails, re-run the
   failed jobs; if it still fails, comment the reason and add `needs-tobias`.

## Release button

Actions → **release** → Run workflow → version `vX.Y.Z`.
The version must match Cargo.toml on main. Running it for a tag that already
exists rebuilds and re-uploads that release's assets (repair).
