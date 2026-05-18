#!/usr/bin/env bash
# install.sh — installs or updates osu-collect from the latest GitHub release
# supports: linux x64 (primary), macOS (exits cleanly if no asset available)
set -euo pipefail

# ── constants ────────────────────────────────────────────────────────────────

readonly REPO="uwuclxdy/osu-collect"
readonly API_URL="https://api.github.com/repos/${REPO}/releases/latest"
readonly INSTALL_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
readonly BINARY_NAME="osu-collect"
readonly DESKTOP_DIR="${HOME}/.local/share/applications"
readonly DESKTOP_FILE="${DESKTOP_DIR}/osu-collect.desktop"

# ── helpers ──────────────────────────────────────────────────────────────────

info()  { printf '==> %s\n' "$*"; }
error() { printf 'error: %s\n' "$*" >&2; }

cleanup() {
  local tmp="${1:-}"
  [[ -n "$tmp" && -d "$tmp" ]] && rm -rf -- "$tmp"
}

require_cmd() {
  command -v "$1" &>/dev/null || { error "required command not found: $1"; exit 1; }
}

# parse_field <json_string> <field>
# minimal jq-free JSON field extractor for simple string values
parse_field() {
  printf '%s' "$1" | grep -o "\"$2\":[[:space:]]*\"[^\"]*\"" | head -1 \
    | sed 's/.*":[[:space:]]*"\(.*\)"/\1/'
}

# ── os / arch detection ──────────────────────────────────────────────────────

detect_asset() {
  local os
  os="$(uname -s)"
  local arch
  arch="$(uname -m)"

  case "$os" in
    Linux)
      if [[ "$arch" != "x86_64" ]]; then
        error "unsupported architecture: $arch (only x64 is supported)"
        exit 1
      fi
      printf 'osu-collect-linux-x64'
      ;;
    Darwin)
      error "macOS build is not published yet — no osu-collect-macos-x64 asset exists"
      info  "when a macOS asset is released, re-run this script"
      exit 0
      ;;
    *)
      error "unsupported OS: $os"
      exit 1
      ;;
  esac
}

# ── fetch release metadata ───────────────────────────────────────────────────

fetch_latest_release() {
  require_cmd curl
  curl -fsSL --retry 3 "$API_URL"
}

parse_release() {
  local json="$1"
  local asset_name="$2"

  local tag
  if command -v jq &>/dev/null; then
    tag="$(printf '%s' "$json" | jq -r '.tag_name')"
    DOWNLOAD_URL="$(printf '%s' "$json" \
      | jq -r --arg n "$asset_name" '.assets[] | select(.name == $n) | .browser_download_url')"
    SHA256_URL="$(printf '%s' "$json" \
      | jq -r --arg n "${asset_name}.sha256" '.assets[] | select(.name == $n) | .browser_download_url')"
  else
    tag="$(parse_field "$json" "tag_name")"
    # extract browser_download_url for asset_name
    # GitHub API asset blocks span ~30 lines; -A5 is too short
    DOWNLOAD_URL="$(printf '%s' "$json" \
      | grep -A30 "\"name\":.*\"${asset_name}\"" \
      | grep "browser_download_url" | head -1 \
      | sed 's/.*"browser_download_url":[[:space:]]*"\([^"]*\)".*/\1/')"
    SHA256_URL="$(printf '%s' "$json" \
      | grep -A30 "\"name\":.*\"${asset_name}\.sha256\"" \
      | grep "browser_download_url" | head -1 \
      | sed 's/.*"browser_download_url":[[:space:]]*"\([^"]*\)".*/\1/')"
  fi

  [[ -n "$tag" ]]          || { error "could not parse tag_name from release JSON"; exit 1; }
  [[ -n "$DOWNLOAD_URL" ]] || { error "asset '${asset_name}' not found in release ${tag}"; exit 1; }
  [[ -n "$SHA256_URL" ]]   || { error "checksum file '${asset_name}.sha256' not found in release ${tag}"; exit 1; }

  printf '%s' "$tag"
}

# ── sha256 verification ──────────────────────────────────────────────────────

verify_sha256() {
  local file="$1"
  local sha256_file="$2"

  # expected format: "<hex>  <filename>" — grab only the hex part
  local expected
  expected="$(awk '{print $1}' "$sha256_file")"

  local actual
  if command -v sha256sum &>/dev/null; then
    actual="$(sha256sum "$file" | awk '{print $1}')"
  elif command -v shasum &>/dev/null; then
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  else
    error "no sha256sum or shasum found — cannot verify download"
    exit 1
  fi

  if [[ "$actual" != "$expected" ]]; then
    error "sha256 mismatch"
    error "  expected: $expected"
    error "  actual:   $actual"
    return 1
  fi
}

# ── path advice ──────────────────────────────────────────────────────────────

check_path() {
  local dir="$1"
  case ":${PATH}:" in
    *":${dir}:"*) ;;
    *)
      info "${dir} is not in \$PATH"
      info "add this to your shell rc (~/.bashrc, ~/.zshrc, etc.):"
      # shellcheck disable=SC2016
      printf '    export PATH="%s:$PATH"\n' "$dir"
      ;;
  esac
}

# ── shortcut helpers ─────────────────────────────────────────────────────────

write_desktop_file() {
  local path="$1"
  local binary_path="${INSTALL_DIR}/${BINARY_NAME}"
  mkdir -p -- "$(dirname "$path")"
  cat > "$path" <<EOF
[Desktop Entry]
Type=Application
Name=osu-collect
Comment=download osu! collections (TUI)
Exec="${binary_path}"
Terminal=true
Categories=Utility;
EOF
  chmod +x -- "$path"
}

install_shortcuts() {
  write_desktop_file "$DESKTOP_FILE"
  info "shortcut created: $DESKTOP_FILE"

  local desktop_dir
  if command -v xdg-user-dir &>/dev/null; then
    desktop_dir="$(xdg-user-dir DESKTOP 2>/dev/null || printf '%s/Desktop' "$HOME")"
  else
    desktop_dir="${HOME}/Desktop"
  fi

  if [[ -d "$desktop_dir" ]]; then
    write_desktop_file "${desktop_dir}/osu-collect.desktop"
    info "desktop shortcut created: ${desktop_dir}/osu-collect.desktop"
  fi
}

# ── current install state ────────────────────────────────────────────────────

installed_hash() {
  local bin="${INSTALL_DIR}/${BINARY_NAME}"
  [[ -f "$bin" ]] || { printf ''; return; }
  if command -v sha256sum &>/dev/null; then
    sha256sum "$bin" | awk '{print $1}'
  elif command -v shasum &>/dev/null; then
    shasum -a 256 "$bin" | awk '{print $1}'
  else
    printf ''
  fi
}

shortcut_missing() {
  [[ ! -f "$DESKTOP_FILE" ]]
}

# ── main ─────────────────────────────────────────────────────────────────────

main() {
  local asset_name
  asset_name="$(detect_asset)"

  info "fetching latest release info..."
  local json
  json="$(fetch_latest_release)"

  local DOWNLOAD_URL SHA256_URL
  local tag
  tag="$(parse_release "$json" "$asset_name")"

  info "latest release: $tag"

  # download to a temp dir so we can verify before replacing
  local tmpdir
  tmpdir="$(mktemp -d)"
  trap 'cleanup "$tmpdir"' EXIT

  local tmp_bin="${tmpdir}/${asset_name}"
  local tmp_sha="${tmpdir}/${asset_name}.sha256"

  info "downloading checksum..."
  curl -fsSL --retry 3 -o "$tmp_sha" "$SHA256_URL"

  local remote_hash
  remote_hash="$(awk '{print $1}' "$tmp_sha")"

  # idempotency check: same hash already installed?
  local current_hash
  current_hash="$(installed_hash)"

  if [[ -n "$current_hash" && "$current_hash" == "$remote_hash" ]]; then
    info "already up to date ($tag)"
    shortcut_missing && install_shortcuts
    exit 0
  fi

  info "downloading osu-collect $tag..."
  curl -fsSL --retry 3 -o "$tmp_bin" "$DOWNLOAD_URL"

  info "verifying checksum..."
  if ! verify_sha256 "$tmp_bin" "$tmp_sha"; then
    rm -f -- "$tmp_bin"
    error "download aborted due to checksum failure"
    exit 1
  fi

  mkdir -p -- "$INSTALL_DIR"
  install -m 755 -- "$tmp_bin" "${INSTALL_DIR}/${BINARY_NAME}"
  info "installed to ${INSTALL_DIR}/${BINARY_NAME}"

  install_shortcuts
  check_path "$INSTALL_DIR"

  info "done — run 'osu-collect' to start"
}

main "$@"
