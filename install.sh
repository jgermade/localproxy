#!/bin/sh
# localproxy installer. See usage() below or run with --help.

set -eu

REPO="jgermade/localproxy"
BLOCK_BEGIN="# --- localproxy -----------------------------------------------------------"
BLOCK_END="# --- end localproxy -------------------------------------------------------"

version="${LOCALPROXY_VERSION:-}"
install_dir="${LOCALPROXY_INSTALL_DIR:-$HOME/.local/bin}"
profile="${LOCALPROXY_PROFILE:-}"
modify_profile=1
[ -n "${LOCALPROXY_NO_MODIFY_PROFILE:-}" ] && modify_profile=0

tmp_file=""

log() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  [ -n "$tmp_file" ] && [ -f "$tmp_file" ] && rm -f "$tmp_file"
  return 0
}
trap cleanup EXIT INT TERM

usage() {
  cat << 'USAGE'
localproxy installer.

  curl -fsSL https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash
  wget -qO-  https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash

Options (each one has an environment variable equivalent):

  --version <vX.Y.Z>     LOCALPROXY_VERSION            release to install (default: latest)
  --dir <path>           LOCALPROXY_INSTALL_DIR        install directory (default: ~/.local/bin)
  --profile <path>       LOCALPROXY_PROFILE            shell profile to update (default: autodetect)
  --no-modify-profile    LOCALPROXY_NO_MODIFY_PROFILE  do not touch any shell profile
  --help

  GITHUB_TOKEN (or LOCALPROXY_GITHUB_TOKEN) is used to download release assets when
  the repository is private.

Pass options through a pipe with `-s --`:

  curl -fsSL .../install.sh | bash -s -- --version v0.1.1 --no-modify-profile
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
  --version)
    [ $# -ge 2 ] || die "--version needs an argument"
    version="$2"
    shift 2
    ;;
  --dir)
    [ $# -ge 2 ] || die "--dir needs an argument"
    install_dir="$2"
    shift 2
    ;;
  --profile)
    [ $# -ge 2 ] || die "--profile needs an argument"
    profile="$2"
    shift 2
    ;;
  --no-modify-profile)
    modify_profile=0
    shift
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *) die "unknown option: $1 (try --help)" ;;
  esac
done

# --- platform detection ----------------------------------------------------

detect_asset() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
  Darwin) os_name="macos" ;;
  Linux) os_name="linux" ;;
  *) die "unsupported operating system: $os (only macOS and Linux have release binaries)" ;;
  esac

  case "$arch" in
  x86_64 | amd64) arch_name="x86_64" ;;
  arm64 | aarch64) arch_name="aarch64" ;;
  *) die "unsupported architecture: $arch" ;;
  esac

  printf 'localproxy-%s-%s' "$os_name" "$arch_name"
}

# --- download --------------------------------------------------------------

token="${LOCALPROXY_GITHUB_TOKEN:-${GITHUB_TOKEN:-}}"

# fetch <url> <output|-> [accept] [authenticated]
fetch() {
  url="$1"
  target="$2"
  accept="${3:-}"
  authenticated="${4:-0}"

  if command -v curl > /dev/null 2>&1; then
    set -- curl -fsSL --proto '=https' --tlsv1.2 -o "$target"
    [ -n "$accept" ] && set -- "$@" -H "Accept: $accept"
    [ "$authenticated" -eq 1 ] && set -- "$@" -H "Authorization: Bearer $token"
    "$@" "$url"
  elif command -v wget > /dev/null 2>&1; then
    set -- wget -q --https-only -O "$target"
    [ -n "$accept" ] && set -- "$@" --header "Accept: $accept"
    [ "$authenticated" -eq 1 ] && set -- "$@" --header "Authorization: Bearer $token"
    "$@" "$url"
  else
    die "neither curl nor wget is available"
  fi
}

# Private repositories do not serve releases/latest/download; the asset has to
# be resolved through the API and downloaded by id with the token attached.
resolve_asset_url() {
  if [ -n "$version" ]; then
    api="https://api.github.com/repos/$REPO/releases/tags/$version"
  else
    api="https://api.github.com/repos/$REPO/releases/latest"
  fi

  release_json="$(mktemp "${TMPDIR:-/tmp}/localproxy-release.XXXXXX")"
  if ! fetch "$api" "$release_json" "application/vnd.github+json" 1; then
    rm -f "$release_json"
    die "cannot read $api (is the token valid?)"
  fi

  # One asset object per line, then keep the one whose name matches.
  asset_url="$(
    tr '{' '\n' < "$release_json" |
      grep -E "\"name\"[[:space:]]*:[[:space:]]*\"$asset\"" |
      grep -oE 'https://api\.github\.com/repos/[^"]+/releases/assets/[0-9]+' |
      head -n 1
  )"
  rm -f "$release_json"

  [ -n "$asset_url" ] || die "release does not contain the asset $asset"
  printf '%s' "$asset_url"
}

asset="$(detect_asset)"

if [ -n "$version" ]; then
  case "$version" in
  v*) : ;;
  *) version="v$version" ;;
  esac
  log "Installing localproxy $version ($asset)"
else
  log "Installing localproxy latest ($asset)"
fi

mkdir -p "$install_dir" || die "cannot create $install_dir"
[ -w "$install_dir" ] || die "$install_dir is not writable"

# Staged in the target directory so the final move is an atomic rename on the
# same filesystem; that also works while an older localproxy is still running.
tmp_file="$(mktemp "$install_dir/.localproxy.XXXXXX")"

if [ -n "$token" ]; then
  url="$(resolve_asset_url)" || exit 1
  [ -n "$url" ] || exit 1
  accept="application/octet-stream"
  authenticated=1
elif [ -n "$version" ]; then
  url="https://github.com/$REPO/releases/download/$version/$asset"
  accept=""
  authenticated=0
else
  url="https://github.com/$REPO/releases/latest/download/$asset"
  accept=""
  authenticated=0
fi

log "Downloading $url"
if ! fetch "$url" "$tmp_file" "$accept" "$authenticated"; then
  log ""
  log "If $REPO is private, export a token with 'repo' scope and retry:"
  log "  export GITHUB_TOKEN=ghp_..."
  die "download failed: $url"
fi
[ -s "$tmp_file" ] || die "downloaded file is empty: $url"

chmod 755 "$tmp_file"

if [ "$(uname -s)" = "Darwin" ] && command -v xattr > /dev/null 2>&1; then
  xattr -d com.apple.quarantine "$tmp_file" > /dev/null 2>&1 || true
fi

"$tmp_file" --version > /dev/null 2>&1 ||
  die "the downloaded binary does not run on this machine"

mv -f "$tmp_file" "$install_dir/localproxy"
tmp_file=""

log "Installed $("$install_dir/localproxy" --version) -> $install_dir/localproxy"

# If there is a config from a previous install, rewrite it with defaults from
# the current binary so missing fields introduced in new versions are added.
config_file="$HOME/.config/localproxy/config.toml"
if [ -f "$config_file" ]; then
  if "$install_dir/localproxy" config-extend > /dev/null 2>&1; then
    log "Extended existing config with missing fields: $config_file"
  else
    warn "could not extend existing config: $config_file"
  fi
fi

# --- shell profile ---------------------------------------------------------

detect_profile() {
  shell_name="$(basename "${SHELL:-}")"

  case "$shell_name" in
  zsh) printf '%s/.zshrc' "$HOME" ;;
  bash)
    if [ "$(uname -s)" = "Darwin" ]; then
      printf '%s/.bash_profile' "$HOME"
    else
      printf '%s/.bashrc' "$HOME"
    fi
    ;;
  *) printf '' ;;
  esac
}

# Prints the install dir with $HOME collapsed, so the profile stays portable.
portable_dir() {
  case "$install_dir" in
  "$HOME") printf '$HOME' ;;
  "$HOME"/*) printf '$HOME%s' "${install_dir#"$HOME"}" ;;
  *) printf '%s' "$install_dir" ;;
  esac
}

write_block() {
  target="$1"

  {
    printf '\n%s\n' "$BLOCK_BEGIN"
    printf 'export PATH="%s:$PATH"\n\n' "$(portable_dir)"
    cat << 'LOCALPROXY_BLOCK'
LOCALPROXY_NO_PROXY="localhost,127.0.0.1,::1"

if command -v localproxy > /dev/null 2>&1; then
  # The listen address lives in ~/.config/localproxy/config.toml, so the URL is read
  # from there on every shell start instead of being hardcoded here.
  LOCALPROXY_URL="$(localproxy url 2> /dev/null)"
  : "${LOCALPROXY_URL:=http://127.0.0.1:1234}"

  # `localproxy status` talks to the control socket, so it only succeeds when the
  # daemon is really listening. Start it in the background otherwise.
  localproxy status > /dev/null 2>&1 || localproxy start --detached > /dev/null 2>&1

  export http_proxy="$LOCALPROXY_URL"
  export https_proxy="$LOCALPROXY_URL"
  export all_proxy="$LOCALPROXY_URL"
  export HTTP_PROXY="$LOCALPROXY_URL"
  export HTTPS_PROXY="$LOCALPROXY_URL"
  export ALL_PROXY="$LOCALPROXY_URL"
  export no_proxy="$LOCALPROXY_NO_PROXY"
  export NO_PROXY="$LOCALPROXY_NO_PROXY"
fi
LOCALPROXY_BLOCK
    printf '%s\n' "$BLOCK_END"
  } >> "$target"
}

update_block() {
  target="$1"
  tmp="$(mktemp "${TMPDIR:-/tmp}/localproxy-profile.XXXXXX")"

  if ! awk -v begin="$BLOCK_BEGIN" -v end="$BLOCK_END" '
    $0 == begin { in_block = 1; replaced = 1; next }
    in_block {
      if ($0 == end) {
        in_block = 0
      }
      next
    }
    { print }
    END {
      if (in_block) {
        exit 2
      }
      if (!replaced) {
        exit 3
      }
    }
  ' "$target" > "$tmp"; then
    rm -f "$tmp"
    return 1
  fi

  mv "$tmp" "$target"
  write_block "$target"
}

profile_updated=0

if [ "$modify_profile" -eq 1 ]; then
  [ -n "$profile" ] || profile="$(detect_profile)"

  if [ -z "$profile" ]; then
    warn "unknown shell (${SHELL:-unset}); skipping profile setup"
  else
    touch "$profile" 2> /dev/null || warn "cannot write $profile"
    if [ ! -w "$profile" ]; then
      warn "cannot write $profile; skipping profile setup"
    elif grep -qF "$BLOCK_BEGIN" "$profile" 2> /dev/null; then
      if update_block "$profile"; then
        profile_updated=1
        log "Updated the localproxy block in $profile"
      else
        warn "found a localproxy block in $profile but failed to update it"
      fi
    else
      write_block "$profile"
      profile_updated=1
      log "Added the localproxy block to $profile"
    fi
  fi
fi

# --- next steps ------------------------------------------------------------

case ":$PATH:" in
*":$install_dir:"*) : ;;
*) warn "$install_dir is not in PATH for this shell" ;;
esac

log ""
log "Next steps:"
log "  1. localproxy config              # interactive wizard"
log "  2. localproxy service install     # run it as a user-level service (optional)"
log "  3. localproxy start"
if [ "$profile_updated" -eq 1 ]; then
  log ""
  log "Reload your shell to pick up the proxy variables:"
  log "  source $profile"
fi
log ""
log "Docs: https://github.com/$REPO#readme"
