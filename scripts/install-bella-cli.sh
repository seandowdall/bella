#!/usr/bin/env sh
set -eu

repo="seandowdall/bella"
channel="prod"
install_dir="/usr/local/bin"

usage() {
  cat <<'USAGE'
Install the Bella CLI beta.

Usage:
  install-bella-cli.sh [--channel prod|qa] [--install-dir DIR]

Options:
  --channel      Release channel to install. Defaults to prod.
  --install-dir  Directory for the bella binary. Defaults to /usr/local/bin.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --channel)
      channel="${2:-}"
      shift 2
      ;;
    --channel=*)
      channel="${1#*=}"
      shift
      ;;
    --install-dir)
      install_dir="${2:-}"
      shift 2
      ;;
    --install-dir=*)
      install_dir="${1#*=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$channel" in
  prod|qa) ;;
  *)
    echo "--channel must be prod or qa" >&2
    exit 2
    ;;
esac

os=$(uname -s)
arch=$(uname -m)

case "$os:$arch" in
  Darwin:arm64) artifact="bella-cli-macos-aarch64" ;;
  Darwin:x86_64) artifact="bella-cli-macos-x86_64" ;;
  Linux:x86_64) artifact="bella-cli-linux-x86_64" ;;
  *)
    echo "unsupported platform: $os $arch" >&2
    exit 1
    ;;
esac

tag="bella-cli-$channel"
base_url="https://github.com/$repo/releases/download/$tag"
archive="$artifact.tar.gz"
checksum="$archive.sha256"
tmp_dir=$(mktemp -d)

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

download() {
  url="$1"
  output="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$output"
  else
    echo "curl or wget is required" >&2
    exit 1
  fi
}

echo "Downloading Bella CLI $channel channel for $os $arch..."
download "$base_url/$archive" "$tmp_dir/$archive"
download "$base_url/$checksum" "$tmp_dir/$checksum"

if command -v shasum >/dev/null 2>&1; then
  (cd "$tmp_dir" && shasum -a 256 -c "$checksum")
elif command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmp_dir" && sha256sum -c "$checksum")
else
  echo "shasum or sha256sum is required" >&2
  exit 1
fi

tar -xzf "$tmp_dir/$archive" -C "$tmp_dir"

if [ ! -d "$install_dir" ]; then
  echo "install directory does not exist: $install_dir" >&2
  exit 1
fi

target="$install_dir/bella"
if [ -w "$install_dir" ]; then
  install -m 0755 "$tmp_dir/$artifact/bella" "$target"
elif command -v sudo >/dev/null 2>&1; then
  sudo install -m 0755 "$tmp_dir/$artifact/bella" "$target"
else
  echo "install directory is not writable and sudo is unavailable: $install_dir" >&2
  exit 1
fi

echo "Installed Bella CLI to $target"
echo "Run: bella --environment $channel login"
