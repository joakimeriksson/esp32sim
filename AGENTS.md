# Experiment history

- Before proposing, implementing or benchmarking an ESP32-S3 execution, browser-speed or timing experiment, search [docs/experiments.md](docs/experiments.md) by mechanism and aliases.
- Cite the existing experiment ID and say what materially differs before retrying: mechanism, workload, correctness contract or measurement quality. A renamed branch is not a new experiment. Inspect preserved patches before rebuilding them.
- Record the outcome in the same entry, keeping earlier results, with revisions, inputs, exact work and output checks, conditions, uncertainty, adoption and a receipt. Negative results count. Use a new stable ID only for a materially different idea and cross-reference related IDs. Do not start a second list.

# GitHub pull request stacks

- Use `gh stack` when one pull request depends on another. Setting a PR's base branch alone does not register a native GitHub stack.
- For existing PRs, use `gh stack link --remote origin --base main <bottom-PR-URL> <top-PR-URL>` in dependency order, bottom to top. Follow any explicitly requested remote or base instead.
- For new stacks, use `gh stack init` and `gh stack submit`; inspect their current `--help` before acting.
- After linking existing PRs remotely, use `gh stack checkout <stack-number>` to import local tracking, then `gh stack view --json` to verify it. An untracked-branch error from `view` alone does not prove that the remote stack is absent.
- Verify the native GitHub stack registration before describing PRs as stacked. Distinguish branch ancestry from GitHub's stack feature.
- Do not rewrite published branch history merely to register an existing stack.
- If merge commits prevent `gh stack modify`, preserve published history and use `gh stack link` to add new PRs. When local tracking needs a clean refresh, use `gh stack unstack --local` followed by `gh stack checkout <stack-number>`; the `--local` flag leaves the GitHub stack intact.
