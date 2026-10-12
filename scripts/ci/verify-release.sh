#!/usr/bin/env bash
set -euo pipefail

if ! printf '%s' "${TAG:-}" | grep -Eq '^(beta-)?v[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$'; then
  echo "::error::TAG is not a release tag" >&2
  exit 1
fi

dir=release-assets
rm -rf "$dir"
mkdir -p "$dir"
gh release download "$TAG" --repo "$REPO" --dir "$dir"

shopt -s nullglob
installers=("$dir"/*-setup.exe)
if [ "${#installers[@]}" -ne 1 ] || [ ! -f "${installers[0]}.sig" ]; then
  echo "::error::$TAG needs exactly one installer and its .sig" >&2
  exit 1
fi

for asset in "$dir"/*; do
  gh attestation verify "$asset" \
    --repo "$REPO" \
    --signer-workflow "$REPO/.github/workflows/build-release.yml" \
    --source-ref refs/heads/main \
    --deny-self-hosted-runners
done
