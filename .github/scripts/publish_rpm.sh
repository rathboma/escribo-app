#!/bin/bash
# Publish .rpm packages to the dnf repo under the rpm/ prefix of the R2 bucket
# (served at https://repo.escriboapp.com/rpm).
#
# Each package is signed (gpgcheck=1) and the repo metadata is detach-signed
# (repo_gpgcheck=1) with the escribo signing key, matching the repo file the
# rpm-postinstall script writes. The public key is uploaded to the bucket root
# as escribo.key. GPG_KEY_ID must name a key already imported into the runner's
# gpg keyring (release-published.yml imports GPG_PRIVATE_KEY).
#
# The existing repo is pulled down, the new packages added, the metadata rebuilt
# with createrepo_c, then everything is synced back up. Re-running is safe.
#
# Usage: publish_rpm.sh <file.rpm> [<file.rpm> ...]
# Env:   R2_BUCKET R2_ENDPOINT GPG_KEY_ID
#        AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

set -euo pipefail

if [ $# -eq 0 ]; then
  echo "usage: publish_rpm.sh <file.rpm> ..." >&2
  exit 1
fi

if [ -z "${GPG_KEY_ID:-}" ]; then
  echo "GPG_KEY_ID is not set - refusing to publish an unsigned dnf repo." >&2
  echo "The installed packages pin gpgcheck=1, so an unsigned repo would break dnf." >&2
  exit 1
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
REPO_DIR="$WORK/rpm"
mkdir -p "$REPO_DIR"

# Existing repo (empty on the first publish).
aws s3 sync "s3://$R2_BUCKET/rpm" "$REPO_DIR" --endpoint-url "$R2_ENDPOINT" || true

cp "$@" "$REPO_DIR/"

# Sign each package so gpgcheck=1 passes. rpmsign drives gpg non-interactively;
# the key is passphrase-less in CI.
cat > "$HOME/.rpmmacros" <<EOF
%_gpg_name $GPG_KEY_ID
%__gpg_sign_cmd %{__gpg} gpg --batch --no-armor --pinentry-mode loopback --no-secmem-warning -u "%{_gpg_name}" --detach-sign %{__plaintext_filename}
EOF
rpmsign --addsign "$REPO_DIR"/*.rpm

createrepo_c --update "$REPO_DIR"

# Detach-sign repomd.xml so repo_gpgcheck=1 passes.
gpg --batch --yes --detach-sign --armor -u "$GPG_KEY_ID" "$REPO_DIR/repodata/repomd.xml"

# Publish the public key at the bucket root (referenced by both repo files).
if [ -f build/escribo.key ]; then
  aws s3 cp build/escribo.key "s3://$R2_BUCKET/escribo.key" \
    --endpoint-url "$R2_ENDPOINT" \
    --content-type "application/pgp-keys" \
    --cache-control "public, max-age=300"
fi

# Packages first (immutable), metadata last so the repo is never half-updated.
aws s3 sync "$REPO_DIR" "s3://$R2_BUCKET/rpm" \
  --endpoint-url "$R2_ENDPOINT" \
  --exclude "repodata/*" \
  --cache-control "public, max-age=31536000, immutable"

aws s3 sync "$REPO_DIR/repodata" "s3://$R2_BUCKET/rpm/repodata" \
  --endpoint-url "$R2_ENDPOINT" \
  --cache-control "public, max-age=300" \
  --delete
