#!/bin/bash
# Publish .deb packages to an apt repo under the deb/ prefix of the R2 bucket
# (served at https://repo.escriboapp.com/deb). The repo is unsigned - there is
# no GPG key yet - so users add it with [trusted=yes]. Add `--sign=<keyid>` here
# once a signing key exists.
#
# Usage: publish_deb.sh <file.deb> [<file.deb> ...]
# Env:   R2_BUCKET R2_ENDPOINT AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

set -euo pipefail

if [ $# -eq 0 ]; then
  echo "usage: publish_deb.sh <file.deb> ..." >&2
  exit 1
fi

deb-s3 upload "$@" \
  --bucket="$R2_BUCKET" \
  --prefix=deb \
  --codename=stable \
  --endpoint="$R2_ENDPOINT" \
  --lock \
  --preserve-versions
