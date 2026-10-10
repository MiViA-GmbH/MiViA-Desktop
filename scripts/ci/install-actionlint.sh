#!/usr/bin/env bash
set -euo pipefail
sums="$(pwd)/scripts/ci/tools.sha256"
version="$(sed -n 's|^# rhysd/actionlint v||p' "$sums")"
file="actionlint_${version}_linux_amd64.tar.gz"
dir="$(mktemp -d)"
curl -sSfL -o "$dir/$file" "https://github.com/rhysd/actionlint/releases/download/v${version}/${file}"
grep " ${file}\$" "$sums" | (cd "$dir" && sha256sum -c -)
tar -xzf "$dir/$file" -C "$dir" actionlint
mkdir -p "$HOME/.local/bin"
install -m 0755 "$dir/actionlint" "$HOME/.local/bin/actionlint"
echo "$HOME/.local/bin" >> "$GITHUB_PATH"
