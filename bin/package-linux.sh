#!/bin/bash
# Packages the release build for Linux:
#
#   dist/escribo-<version>-linux-<arch>.AppImage
#   dist/escribo-<version>-linux-<arch>.deb
#   dist/escribo-<version>-linux-<arch>.rpm
#   dist/escribo-<version>-linux-<arch>.flatpak
#
# <arch> is x64 or arm64 — the names the website matches release assets by
# (`suffix` in escribo-web's _data/downloads.yml), so keep them in step.
#
# Usage: bin/package-linux.sh [appimage] [deb] [rpm] [flatpak]   (default: all)
# Env:   SKIP_BUILD=1  package target/release/escribo as it is
#
# Needs: dpkg-deb (deb), rpmbuild (rpm), flatpak-builder plus the flathub
# remote (flatpak). appimagetool is downloaded on first use.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

APP_ID=com.escriboapp.escribo
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
case "$(uname -m)" in
  x86_64) ARCH=x64; DEB_ARCH=amd64; RPM_ARCH=x86_64; AI_ARCH=x86_64 ;;
  aarch64|arm64) ARCH=arm64; DEB_ARCH=arm64; RPM_ARCH=aarch64; AI_ARCH=aarch64 ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac
FORMATS=("$@")
[ ${#FORMATS[@]} -eq 0 ] && FORMATS=(appimage deb rpm flatpak)
NAME="escribo-${VERSION}-linux-${ARCH}"
DIST="$ROOT/dist"
STAGE="$DIST/stage"
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/escribo"

if [ -z "${SKIP_BUILD:-}" ]; then
  cargo build --release --locked
fi
[ -x "$BIN" ] || { echo "no release binary at $BIN" >&2; exit 1; }

# --- The installed tree, shared by every format ---------------------------
rm -rf "$STAGE"
mkdir -p "$STAGE/usr/bin" "$STAGE/usr/share/applications" "$STAGE/usr/share/metainfo" \
  "$STAGE/usr/share/doc/escribo"
install -m 755 "$BIN" "$STAGE/usr/bin/escribo"
install -m 644 packaging/linux/$APP_ID.desktop "$STAGE/usr/share/applications/"
install -m 644 packaging/linux/$APP_ID.metainfo.xml "$STAGE/usr/share/metainfo/"
install -m 644 LICENSE "$STAGE/usr/share/doc/escribo/copyright"
for png in build/icons/*.png; do
  size="$(basename "$png" .png)"
  mkdir -p "$STAGE/usr/share/icons/hicolor/$size/apps"
  install -m 644 "$png" "$STAGE/usr/share/icons/hicolor/$size/apps/$APP_ID.png"
done

want() {
  local f
  for f in "${FORMATS[@]}"; do [ "$f" = "$1" ] && return 0; done
  return 1
}

# --- deb ---------------------------------------------------------------------
if want deb; then
  pkg="$DIST/deb"
  rm -rf "$pkg" && mkdir -p "$pkg/DEBIAN"
  cp -a "$STAGE/usr" "$pkg/"
  size_kb="$(du -sk "$pkg/usr" | cut -f1)"
  # The newest glibc symbol the binary links against, so a build on a newer
  # distro doesn't claim to run on an older one.
  glibc="$(objdump -T "$BIN" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -n1)"
  cat > "$pkg/DEBIAN/control" <<CONTROL
Package: escribo
Version: $VERSION
Architecture: $DEB_ARCH
Maintainer: Matthew Rathbone <matthew.rathbone@gmail.com>
Installed-Size: $size_kb
Depends: libc6 (>= ${glibc:-2.35}), libgcc-s1, libxkbcommon0, libxkbcommon-x11-0, libxcb1, libfontconfig1, libwayland-client0, libvulkan1 | libegl1
Recommends: wl-clipboard, xdg-desktop-portal
Section: graphics
Priority: optional
Homepage: https://escriboapp.com
Description: Annotate screenshots with arrows, boxes, highlights and text
 escribo is a lightweight annotation tool for screenshots: arrows, boxes, a
 highlighter, text labels, emoji and redaction, with padding, fills and
 export presets. It opens new screenshots from Pictures/Screenshots by itself.
CONTROL
  install -m 755 build/deb-postinstall "$pkg/DEBIAN/postinst"
  dpkg-deb --root-owner-group -Zxz --build "$pkg" "$DIST/$NAME.deb"
  rm -rf "$pkg"
fi

# --- rpm ---------------------------------------------------------------------
if want rpm; then
  top="$DIST/rpmbuild"
  rm -rf "$top" && mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  {
    cat <<SPEC
Name: escribo
Version: ${VERSION//-/~}
Release: 1
Summary: Annotate screenshots with arrows, boxes, highlights and text
License: MIT
URL: https://escriboapp.com
BuildArch: $RPM_ARCH
Requires: libxkbcommon, libxkbcommon-x11, fontconfig, libwayland-client
Recommends: wl-clipboard, xdg-desktop-portal, vulkan-loader

%description
escribo is a lightweight annotation tool for screenshots: arrows, boxes, a
highlighter, text labels, emoji and redaction, with padding, fills and export
presets. It opens new screenshots from Pictures/Screenshots by itself.

%install
cp -a "$STAGE/usr" %{buildroot}/

%files
/usr/bin/escribo
/usr/share/applications/$APP_ID.desktop
/usr/share/metainfo/$APP_ID.metainfo.xml
/usr/share/icons/hicolor/*/apps/$APP_ID.png
%doc /usr/share/doc/escribo/copyright

%post
SPEC
    cat build/rpm-postinstall
  } > "$top/SPECS/escribo.spec"
  rpmbuild --quiet --define "_topdir $top" --define "_build_id_links none" -bb "$top/SPECS/escribo.spec"
  cp "$top"/RPMS/*/escribo-*.rpm "$DIST/$NAME.rpm"
  rm -rf "$top"
fi

# --- AppImage ----------------------------------------------------------------
if want appimage; then
  tool="${APPIMAGETOOL:-$DIST/.tools/appimagetool-$AI_ARCH.AppImage}"
  if [ ! -x "$tool" ]; then
    mkdir -p "$(dirname "$tool")"
    curl -fsSL -o "$tool" \
      "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$AI_ARCH.AppImage"
    chmod +x "$tool"
  fi
  appdir="$DIST/escribo.AppDir"
  rm -rf "$appdir" && mkdir -p "$appdir"
  cp -a "$STAGE/usr" "$appdir/"
  cp packaging/linux/$APP_ID.desktop "$appdir/"
  cp build/icons/256x256.png "$appdir/$APP_ID.png"
  ln -s "$APP_ID.png" "$appdir/.DirIcon"
  ln -s usr/bin/escribo "$appdir/AppRun"
  # FUSE is not needed to build: let appimagetool unpack itself.
  ARCH="$AI_ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$appdir" "$DIST/$NAME.AppImage" >/dev/null
  rm -rf "$appdir"
fi

# --- Flatpak -----------------------------------------------------------------
if want flatpak; then
  fp="$DIST/flatpak"
  rm -rf "$fp" && mkdir -p "$fp"
  cp packaging/linux/$APP_ID.yml "$fp/"
  cp -a "$STAGE" "$fp/stage"
  (
    cd "$fp"
    flatpak-builder --user --install-deps-from=flathub --disable-rofiles-fuse --force-clean \
      --repo=repo build "$APP_ID.yml"
    flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
      repo "$DIST/$NAME.flatpak" "$APP_ID"
  )
  rm -rf "$fp"
fi

rm -rf "$STAGE"
ls -la "$DIST"/"$NAME".* 2>/dev/null
