#!/usr/bin/env bash
set -euo pipefail

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  echo "::error::TAURI_SIGNING_PRIVATE_KEY is empty, run this step in the release environment" >&2
  exit 1
fi
if ! printf '%s' "${TAG:-}" | grep -Eq '^(beta-)?v[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$'; then
  echo "::error::TAG is not a release tag" >&2
  exit 1
fi
version="${TAG#beta-}"
version="${version#v}"

shopt -s nullglob
installers=(target/release/bundle/nsis/*-setup.exe)
if [ "${#installers[@]}" -ne 1 ]; then
  echo "::error::expected one installer, found ${#installers[@]}" >&2
  exit 1
fi

out/cargo-tauri.exe signer sign --password '' --app-version "$version" "${installers[0]}" < /dev/null
