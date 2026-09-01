#!/bin/sh

set -eu

book_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)

if ! command -v mdbook >/dev/null 2>&1; then
  mdbook_version=0.5.3

  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      mdbook_target=x86_64-unknown-linux-gnu
      ;;
    Linux-aarch64 | Linux-arm64)
      mdbook_target=aarch64-unknown-linux-musl
      ;;
    Darwin-x86_64)
      mdbook_target=x86_64-apple-darwin
      ;;
    Darwin-arm64)
      mdbook_target=aarch64-apple-darwin
      ;;
    *)
      echo "Unsupported platform for the mdBook binary: $(uname -s)-$(uname -m)" >&2
      exit 1
      ;;
  esac

  bin_dir="$book_dir/.bin"
  archive="$bin_dir/mdbook.tar.gz"
  download_url="https://github.com/rust-lang/mdBook/releases/download/v$mdbook_version/mdbook-v$mdbook_version-$mdbook_target.tar.gz"

  mkdir -p "$bin_dir"
  curl --fail --location --silent --show-error "$download_url" --output "$archive"
  tar -xzf "$archive" -C "$bin_dir" mdbook
  PATH="$bin_dir:$PATH"
  export PATH
fi

exec "$book_dir/build-site.sh"
