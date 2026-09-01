#!/bin/sh

set -eu

book_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
site_dir="$book_dir/site"

rm -rf "$site_dir"
mkdir -p "$site_dir"
mdbook build "$book_dir" --dest-dir "$site_dir/docs"

cp "$book_dir/homepage/index.html" "$site_dir/index.html"
cp "$book_dir/homepage/landing.css" "$site_dir/landing.css"
cp "$book_dir/homepage/404.html" "$site_dir/404.html"
cp "$book_dir/theme/favicon.svg" "$site_dir/favicon.svg"
cp "$book_dir/theme/favicon.png" "$site_dir/favicon.png"
