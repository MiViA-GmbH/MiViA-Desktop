# Renovate with a 7-day release-age cooldown instead of Dependabot

Dependency, toolchain and action updates come from the Mend-hosted Renovate app with `minimumReleaseAge: 7 days` (strict), weekly groups and no automerge; pnpm's own `minimumReleaseAge` makes lockfile resolution obey the same delay. Renovate updates `.prototools`, tool versions inside workflows and action digests, which Dependabot cannot.

## Consequences

- A third-party app holds write access; required checks pin their source to GitHub Actions, and `main` has no bypass actor.
- Security fixes skip the cooldown but still need review.
