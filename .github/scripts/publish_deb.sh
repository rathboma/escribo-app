#!/bin/bash
# Publish .deb packages to the apt repo under the deb/ prefix of the R2 bucket
# (served at https://repo.escriboapp.com/deb).
#
# The repo Release file is GPG-signed with the escribo signing key so that
# `apt update` verifies it against the key the deb-postinstall script installs
# (/usr/share/keyrings/escribo.asc). GPG_KEY_ID must name a key already imported
# into the runner's gpg keyring (release-published.yml imports GPG_PRIVATE_KEY).
#
# Usage: publish_deb.sh <file.deb> [<file.deb> ...]
# Env:   R2_BUCKET R2_ENDPOINT GPG_KEY_ID
#        AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

set -euo pipefail

if [ $# -eq 0 ]; then
  echo "usage: publish_deb.sh <file.deb> ..." >&2
  exit 1
fi

if [ -z "${GPG_KEY_ID:-}" ]; then
  echo "GPG_KEY_ID is not set - refusing to publish an unsigned apt repo." >&2
  echo "The installed packages pin signed-by, so an unsigned repo would break apt." >&2
  exit 1
fi

deb-s3 upload "$@" \
  --bucket="$R2_BUCKET" \
  --prefix=deb \
  --codename=stable \
  --endpoint="$R2_ENDPOINT" \
  --sign="$GPG_KEY_ID" \
  --lock \
  --preserve-versions
