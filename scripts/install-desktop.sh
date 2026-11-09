#!/usr/bin/env bash
set -euo pipefail

# Install a self-contained Tauri desktop binary for the current Linux user.
repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
app_dir="${HOME}/.local/opt/acervo"
launcher_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/applications"
binary="${repo_dir}/src-tauri/target/release/acervo"

if [[ ${1:-} != "--skip-build" ]]; then
  if [[ $# -gt 0 ]]; then
    printf 'Uso: %s [--skip-build]\n' "$0" >&2
    exit 2
  fi
  (cd "$repo_dir" && npm run tauri -- build --no-bundle)
fi
if [[ ! -x "$binary" ]]; then
  printf 'Executável de produção ausente: %s\n' "$binary" >&2
  exit 1
fi
if [[ $(id -u) -eq 0 ]]; then
  printf 'Execute como usuário comum, não com sudo.\n' >&2
  exit 1
fi

mkdir -p "$app_dir" "$launcher_dir"
if [[ -f "$app_dir/acervo" ]]; then
  cp -p -- "$app_dir/acervo" "$app_dir/acervo.previous"
fi
install -m 755 -- "$binary" "$app_dir/acervo.next"
mv -f -- "$app_dir/acervo.next" "$app_dir/acervo"
install -m 644 -- "$repo_dir/src-tauri/icons/128x128.png" "$app_dir/icon.png"

escape_sed() { printf '%s' "$1" | sed 's/[&|]/\\&/g'; }
sed -e "s|@ACERVO_BINARY@|$(escape_sed "$app_dir/acervo")|g" \
    -e "s|@ACERVO_ICON@|$(escape_sed "$app_dir/icon.png")|g" \
    "$repo_dir/packaging/acervo.desktop.in" > "$launcher_dir/acervo.desktop"
if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$launcher_dir/acervo.desktop"
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$launcher_dir"
fi
printf 'Acervo instalado em %s\nAtalho criado em %s\n' "$app_dir/acervo" "$launcher_dir/acervo.desktop"
