#!/usr/bin/env bash
set -euo pipefail

node scripts/ci/compile-out.mjs

rm -rf signed extract
gh release download "$TAG" --repo "$REPO" --pattern '*-setup.exe' --dir signed
shopt -s nullglob
installers=(signed/*-setup.exe)
if [ "${#installers[@]}" -ne 1 ]; then
  echo "::error::$TAG needs exactly one uploaded installer, found ${#installers[@]}" >&2
  exit 1
fi
installer="${installers[0]}"

7z x -y -oextract "$installer" mivia-desktop.exe > /dev/null
out/xtask.exe authenticode same-app extract/mivia-desktop.exe out/mivia-desktop.exe
out/xtask.exe authenticode verify "$installer" extract/mivia-desktop.exe

target="$RUNNER_TEMP\\installed"
"$installer" /S "/D=$target"
out/xtask.exe authenticode verify "$target\\mivia-desktop.exe" "$target\\uninstall.exe"
cmp "$target\\mivia-desktop.exe" extract/mivia-desktop.exe

{
  echo "name=${installer#signed/}"
  echo "digest=$(sha256sum "$installer" | cut -d' ' -f1)"
} >> "$GITHUB_OUTPUT"
