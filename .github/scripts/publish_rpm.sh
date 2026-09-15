#!/bin/bash
# Publish .rpm packages to a dnf repo under the rpm/ prefix of the R2 bucket
# (served at https://repo.escriboapp.com/rpm). The metadata is unsigned - there
# is no GPG key yet - so users add the repo with gpgcheck=0. Sign the repomd
# with `gpg --detach-sign` once a signing key exists.
#
# The existing repo is pulled down, the new packages added, the metadata rebuilt
# with createrepo_c, then everything is synced back up. Re-running is safe.
#
# Usage: publish_rpm.sh <file.rpm> [<file.rpm> ...]
# Env:   R2_BUCKET R2_ENDPOINT AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

set -euo pipefail

if [ $# -eq 0 ]; then
  echo "usage: publish_rpm.sh <file.rpm> ..." >&2
  exit 1
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
REPO_DIR="$WORK/repo"
mkdir -p "$REPO_DIR"

# Existing repo (empty on the first publish).
aws s3 sync "s3://$R2_BUCKET/rpm" "$REPO_DIR" --endpoint-url "$R2_ENDPOINT" || true

cp "$@" "$REPO_DIR/"

createrepo_c --update "$REPO_DIR"

# Packages first (immutable), metadata last so the repo is never half-updated.
aws s3 sync "$REPO_DIR" "s3://$R2_BUCKET/rpm" \
  --endpoint-url "$R2_ENDPOINT" \
  --exclude "repodata/*" \
  --cache-control "public, max-age=31536000, immutable"

aws s3 sync "$REPO_DIR/repodata" "s3://$R2_BUCKET/rpm/repodata" \
  --endpoint-url "$R2_ENDPOINT" \
  --cache-control "public, max-age=300" \
  --delete
