# Merge queue runs the full CI tier; two required checks

Pull requests run a path-filtered tier behind one aggregator check `gate`; the merge queue runs every job without filters, so `main` only receives commits whose exact tree passed the full tier. A second required check, `guard`, runs from the base commit on `pull_request_target` and owns the PR title and the quality-relaxation check.

## Considered Options

- One flat workflow, full tier after merge on `main`: simpler, but `main` goes red after missed path filters or semantic conflicts.
- moon-orchestrated affected jobs: fewer, larger jobs and a second task-runner convention.

## Consequences

- Each merge waits about 20 minutes in the queue and costs about 2.2 times the CI of an unqueued merge (free on a public repo).
- Every required workflow must listen to `merge_group`.
