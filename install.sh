#!/bin/sh
# Install cad-cli from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/nazriel/cad-client/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/nazriel/cad-client/main/install.sh | sh -s -- --version v0.1.0
#
# Options (or the matching environment variables):
#   --version TAG      release to install (CAD_CLI_VERSION, default: latest)
#   --bin-dir DIR      where the binary goes (CAD_CLI_BIN_DIR, default: ~/.local/bin)
#
# CAD_CLI_DOWNLOAD_URL replaces the release download URL (a mirror, or a local
# server in CI); it must serve the archive and SHA256SUMS for the version.
set -eu

repo="nazriel/cad-client"
version="${CAD_CLI_VERSION:-latest}"
bin_dir="${CAD_CLI_BIN_DIR:-$HOME/.local/bin}"

say() { printf 'cad-cli-install: %s\n' "$*" >&2; }
die() { say "error: $*"; exit 1; }
usage() {
  cat <<'USAGE'
Usage: install.sh [--version TAG] [--bin-dir DIR]
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || die "--version needs a value"; version="$2"; shift 2 ;;
    --bin-dir) [ $# -ge 2 ] || die "--bin-dir needs a value"; bin_dir="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
done

need() { command -v "$1" >/dev/null 2>&1 || die "$1 is required"; }
need curl
need tar
need uname

case "$(uname -s)" in
  Darwin)
    os="apple-darwin"
    arch="$(uname -m)"
    # A shell running under Rosetta reports x86_64 on Apple Silicon.
    if [ "$arch" = "x86_64" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
      arch="arm64"
    fi
    ;;
  Linux)
    os="unknown-linux-musl"
    arch="$(uname -m)"
    ;;
  *) die "unsupported operating system: $(uname -s)" ;;
esac
case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) die "unsupported architecture: $arch" ;;
esac
target="${arch}-${os}"

if [ "$version" = "latest" ]; then
  url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${repo}/releases/latest")" ||
    die "could not resolve the latest release"
  version="${url##*/}"
  case "$version" in v*) ;; *) die "no published release found" ;; esac
fi

archive="cad-cli-${version}-${target}"
base="${CAD_CLI_DOWNLOAD_URL:-https://github.com/${repo}/releases/download/${version}}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading ${archive}.tar.gz"
curl -fsSL -o "${tmp}/${archive}.tar.gz" "${base}/${archive}.tar.gz" ||
  die "download failed: ${base}/${archive}.tar.gz"
curl -fsSL -o "${tmp}/SHA256SUMS" "${base}/SHA256SUMS" || die "download failed: ${base}/SHA256SUMS"

expected="$(awk -v f="${archive}.tar.gz" '$2 == f || $2 == "*" f { print $1 }' "${tmp}/SHA256SUMS")"
[ -n "$expected" ] || die "${archive}.tar.gz is not listed in SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "${tmp}/${archive}.tar.gz" | awk '{ print $1 }')"
else
  actual="$(shasum -a 256 "${tmp}/${archive}.tar.gz" | awk '{ print $1 }')"
fi
[ "$expected" = "$actual" ] || die "checksum mismatch for ${archive}.tar.gz"

tar -xzf "${tmp}/${archive}.tar.gz" -C "$tmp"

mkdir -p "$bin_dir"
cp "${tmp}/${archive}/cad-cli" "${bin_dir}/cad-cli.tmp"
chmod 755 "${bin_dir}/cad-cli.tmp"
mv -f "${bin_dir}/cad-cli.tmp" "${bin_dir}/cad-cli"
say "installed ${bin_dir}/cad-cli (${version})"
case ":${PATH}:" in
  *":${bin_dir}:"*) ;;
  *) say "note: ${bin_dir} is not on PATH; add it in your shell profile" ;;
esac
say "next: set CAD_API_KEY and CAD_API_SECRET, then run \`cad-cli completions <shell>\` for shell completion"
