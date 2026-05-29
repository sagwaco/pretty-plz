#!/bin/sh

set -eu

RELEASE="${PLZ_RELEASE:-latest}"

REPO="sagwaco/pretty-plz"
BIN_NAME="plz"
BIN_DIR="${PLZ_INSTALL_DIR:-$HOME/.local/bin}"
BIN_PATH="$BIN_DIR/$BIN_NAME"

path_action="already"
path_profile=""
tmp_dir=""

step() {
  printf '==> %s\n' "$1"
}

warn() {
  printf 'WARNING: %s\n' "$1" >&2
}

normalize_version() {
  case "$1" in
    "" | latest)
      printf 'latest\n'
      ;;
    v*)
      printf '%s\n' "$1"
      ;;
    *)
      printf 'v%s\n' "$1"
      ;;
  esac
}

validate_version() {
  version="$1"

  if [ "$version" = "latest" ]; then
    return
  fi

  stripped="${version#v}"
  if ! printf '%s\n' "$stripped" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
    echo "Invalid plz release version: $version. Expected latest or vX.Y.Z." >&2
    exit 1
  fi
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --release)
        if [ "$#" -lt 2 ]; then
          echo "--release requires a value." >&2
          exit 1
        fi
        RELEASE="$2"
        shift
        ;;
      --help | -h)
        cat <<EOF
Usage: install.sh [--release VERSION]

Install the plz CLI from GitHub releases.

Environment:
  PLZ_RELEASE          Version to install (latest or vX.Y.Z); overridden by --release.
  PLZ_INSTALL_DIR      Directory for the plz binary (default: \$HOME/.local/bin).
EOF
        exit 0
        ;;
      *)
        echo "Unknown argument: $1" >&2
        exit 1
        ;;
    esac
    shift
  done
}

download_file() {
  url="$1"
  output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O "$output" "$url"
    return
  fi

  echo "curl or wget is required to install plz." >&2
  exit 1
}

download_text() {
  url="$1"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O - "$url"
    return
  fi

  echo "curl or wget is required to install plz." >&2
  exit 1
}

release_url_for_asset() {
  asset="$1"
  resolved_version="$2"

  printf 'https://github.com/%s/releases/download/%s/%s\n' "$REPO" "$resolved_version" "$asset"
}

file_sha256() {
  path="$1"

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
    return
  fi

  if command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$path" | sed 's/^.*= //'
    return
  fi

  echo "sha256sum, shasum, or openssl is required to verify the plz download." >&2
  exit 1
}

verify_archive_digest() {
  archive_path="$1"
  expected_digest="$2"
  actual_digest="$(file_sha256 "$archive_path")"

  if [ "$actual_digest" != "$expected_digest" ]; then
    echo "Downloaded plz archive checksum did not match expected digest." >&2
    echo "expected: $expected_digest" >&2
    echo "actual:   $actual_digest" >&2
    exit 1
  fi
}

archive_digest_from_manifest() {
  asset="$1"
  manifest_path="$2"

  digest="$(awk -v asset="$asset" '
    $2 == asset && $1 ~ /^[0-9a-fA-F]{64}$/ {
      print tolower($1)
      found = 1
      exit
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "$manifest_path" 2>/dev/null || true)"

  if [ -z "$digest" ]; then
    echo "Could not find SHA-256 digest for $asset in SHA256SUMS." >&2
    exit 1
  fi

  printf '%s\n' "$digest"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required to install plz." >&2
    exit 1
  fi
}

resolve_version() {
  normalized_version="$(normalize_version "$RELEASE")"
  validate_version "$normalized_version"

  if [ "$normalized_version" != "latest" ]; then
    printf '%s\n' "$normalized_version"
    return
  fi

  release_json="$(download_text "https://api.github.com/repos/$REPO/releases/latest")"
  resolved="$(printf '%s\n' "$release_json" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"

  if [ -z "$resolved" ]; then
    echo "Failed to resolve the latest plz release version." >&2
    echo "No GitHub release found yet. Install from source instead:" >&2
    echo "  cargo install --git https://github.com/$REPO" >&2
    exit 1
  fi

  validate_version "$resolved"
  printf '%s\n' "$resolved"
}

pick_profile() {
  case "$os:${SHELL:-}" in
    darwin:*/zsh)
      printf '%s\n' "$HOME/.zprofile"
      ;;
    darwin:*/bash)
      printf '%s\n' "$HOME/.bash_profile"
      ;;
    linux:*/zsh)
      printf '%s\n' "$HOME/.zshrc"
      ;;
    linux:*/bash)
      printf '%s\n' "$HOME/.bashrc"
      ;;
    *)
      printf '%s\n' "$HOME/.profile"
      ;;
  esac
}

add_to_path() {
  path_action="already"
  path_profile=""

  case ":$PATH:" in
    *":$BIN_DIR:"*)
      return
      ;;
  esac

  profile="$(pick_profile)"
  path_profile="$profile"
  begin_marker="# >>> plz installer >>>"
  end_marker="# <<< plz installer <<<"
  path_line="export PATH=\"$BIN_DIR:\$PATH\""

  if [ -f "$profile" ] && grep -F "$begin_marker" "$profile" >/dev/null 2>&1; then
    if grep -F "$path_line" "$profile" >/dev/null 2>&1; then
      path_action="configured"
      return
    fi

    if grep -F "$end_marker" "$profile" >/dev/null 2>&1; then
      rewrite_path_block "$profile" "$begin_marker" "$end_marker" "$path_line"
      path_action="updated"
      return
    fi
  fi

  append_path_block "$profile" "$begin_marker" "$end_marker" "$path_line"
  path_action="added"
}

append_path_block() {
  profile="$1"
  begin_marker="$2"
  end_marker="$3"
  path_line="$4"

  {
    printf '\n%s\n' "$begin_marker"
    printf '%s\n' "$path_line"
    printf '%s\n' "$end_marker"
  } >>"$profile"
}

rewrite_path_block() {
  profile="$1"
  begin_marker="$2"
  end_marker="$3"
  path_line="$4"
  tmp_profile="$tmp_dir/profile.$$.tmp"

  awk -v begin="$begin_marker" -v end="$end_marker" -v line="$path_line" '
    BEGIN {
      in_block = 0
      replaced = 0
    }
    $0 == begin {
      if (!replaced) {
        print begin
        print line
        print end
        replaced = 1
      }
      in_block = 1
      next
    }
    in_block {
      if ($0 == end) {
        in_block = 0
      }
      next
    }
    {
      print
    }
    END {
      if (in_block != 0) {
        exit 1
      }
    }
  ' "$profile" >"$tmp_profile"
  mv "$tmp_profile" "$profile"
}

print_launch_instructions() {
  case "$path_action" in
    added | updated | configured)
      step "Added $BIN_DIR to PATH in $path_profile"
      printf 'Restart your shell or run: export PATH="%s:$PATH"\n' "$BIN_DIR"
      ;;
  esac

  printf '\nRun guided setup once:\n'
  printf '  plz configure\n'
}

parse_args "$@"

require_command mktemp
require_command tar

case "$(uname -s)" in
  Darwin)
    os="darwin"
    ;;
  Linux)
    os="linux"
    ;;
  *)
    echo "install.sh supports macOS and Linux." >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64 | amd64)
    arch="x86_64"
    ;;
  arm64 | aarch64)
    arch="aarch64"
    ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if [ "$os" = "darwin" ] && [ "$arch" = "x86_64" ]; then
  if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = "1" ]; then
    arch="aarch64"
  fi
fi

if [ "$os" = "darwin" ]; then
  if [ "$arch" = "aarch64" ]; then
    target="aarch64-apple-darwin"
    platform_label="macOS (Apple Silicon)"
  else
    target="x86_64-apple-darwin"
    platform_label="macOS (Intel)"
  fi
else
  if [ "$arch" = "aarch64" ]; then
    target="aarch64-unknown-linux-musl"
    platform_label="Linux (ARM64)"
  else
    target="x86_64-unknown-linux-musl"
    platform_label="Linux (x64)"
  fi
fi

resolved_version="$(resolve_version)"
asset="plz-${resolved_version}-${target}.tar.gz"
download_url="$(release_url_for_asset "$asset" "$resolved_version")"
checksum_url="$(release_url_for_asset "SHA256SUMS" "$resolved_version")"

step "Installing plz CLI"
step "Detected platform: $platform_label"
step "Resolved version: $resolved_version"

tmp_dir="$(mktemp -d)"
cleanup() {
  if [ -n "$tmp_dir" ]; then
    rm -rf "$tmp_dir"
  fi
}
trap cleanup EXIT INT TERM

archive_path="$tmp_dir/$asset"
checksum_path="$tmp_dir/SHA256SUMS"

step "Downloading plz"
download_file "$download_url" "$archive_path"
download_file "$checksum_url" "$checksum_path"
expected_digest="$(archive_digest_from_manifest "$asset" "$checksum_path")"
verify_archive_digest "$archive_path" "$expected_digest"

extract_dir="$tmp_dir/extract"
mkdir -p "$extract_dir"
tar -xzf "$archive_path" -C "$extract_dir"

if [ -f "$extract_dir/$BIN_NAME" ]; then
  staged_binary="$extract_dir/$BIN_NAME"
elif [ -f "$extract_dir/plz-$resolved_version-$target/$BIN_NAME" ]; then
  staged_binary="$extract_dir/plz-$resolved_version-$target/$BIN_NAME"
else
  staged_binary="$(find "$extract_dir" -type f -name "$BIN_NAME" | head -n 1)"
fi

if [ -z "$staged_binary" ] || [ ! -f "$staged_binary" ]; then
  echo "Could not find $BIN_NAME in the downloaded archive." >&2
  exit 1
fi

mkdir -p "$BIN_DIR"
install -m 0755 "$staged_binary" "$BIN_PATH"

add_to_path

if ! "$BIN_PATH" --version >/dev/null 2>&1; then
  echo "Installed plz binary failed verification." >&2
  exit 1
fi

case "$path_action" in
  added | updated | configured)
    print_launch_instructions
    ;;
  *)
    step "$BIN_DIR is already on PATH"
    print_launch_instructions
    ;;
esac

printf 'plz %s installed successfully to %s\n' "$resolved_version" "$BIN_PATH"
