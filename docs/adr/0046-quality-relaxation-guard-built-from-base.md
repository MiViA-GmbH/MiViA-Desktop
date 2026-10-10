# Quality-gate relaxation fails CI, checked by code from the base commit

Any pull request that loosens a lint, threshold, floor, suppression budget, filter or CI rule fails `guard`, which builds `xtask` from the base commit and reads the PR head only as git objects. There is no label or approval input; the only way past it is the repository admin's logged ruleset bypass, used after a retrospective has agreed the change.

## Considered Options

- Run the check inside `ci.yml`: the PR could edit the check or the workflow that runs it.
- Org ruleset "require workflows to pass": the clean option, but it needs GitHub Team or Enterprise.

## Consequences

- `pull_request_target` is used in exactly one workflow, which never builds, installs or executes PR code.
