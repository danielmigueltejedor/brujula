#!/bin/bash
# Builds Brújula-<version>-x86_64.AppImage from a release build.
#
#   cargo build --release -p brujula
#   packaging/appimage/build.sh [output-dir]
#
# Needs on PATH (or in $TOOLS): linuxdeploy, linuxdeploy-plugin-gtk.sh and appimagetool, plus
# the GTK 4 and WebKitGTK 6.0 the binary was built against (they are copied into the image).
# The AppImage then runs on a distribution whose glibc is at least as new as the one it was
# built on, so it is built on Arch (see .github/workflows/appimage.yml).
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$root/dist}
target=${CARGO_TARGET_DIR:-$root/target}
bin=$target/${PROFILE:-release}/brujula
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
version=${VERSION:-${version:-dev}}
id=dev.danielmiguelt.Brujula
appdir=$out/AppDir
tools=${TOOLS:-}
tool() { if [ -n "$tools" ] && [ -x "$tools/$1" ]; then echo "$tools/$1"; else command -v "$1"; fi; }

[ -x "$bin" ] || { echo "build first: cargo build --release -p brujula ($bin not found)" >&2; exit 1; }

rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" "$appdir/apprun-hooks" "$out"
install -m755 "$bin" "$appdir/usr/bin/brujula"

# WebKit starts its web and network processes from its own directory; they are copied whole so
# their libraries are bundled too.
webprocess=$(find /usr/lib /usr/lib64 /usr/libexec -name WebKitWebProcess -path '*webkitgtk-6.0*' 2>/dev/null | head -1)
[ -n "$webprocess" ] || { echo "WebKitGTK 6.0 helper processes not found" >&2; exit 1; }
webdir=$(dirname "$webprocess")
mkdir -p "$appdir/usr/lib/webkitgtk-6.0"
cp -a "$webdir"/. "$appdir/usr/lib/webkitgtk-6.0/"

# Where the helper processes and the injected bundle are, relative to the mounted image.
cat > "$appdir/apprun-hooks/brujula-webkit.sh" <<'HOOK'
export WEBKIT_EXEC_PATH="$APPDIR/usr/lib/webkitgtk-6.0"
export WEBKIT_INJECTED_BUNDLE_PATH="$APPDIR/usr/lib/webkitgtk-6.0/injected-bundle"
# The icons of the app are found by name in the image.
export XDG_DATA_DIRS="$APPDIR/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
HOOK

extra=()
for helper in "$appdir"/usr/lib/webkitgtk-6.0/*; do
  [ -f "$helper" ] && [ -x "$helper" ] && extra+=(--executable "$helper")
done
for lib in "$appdir"/usr/lib/webkitgtk-6.0/injected-bundle/*.so; do
  [ -f "$lib" ] && extra+=(--library "$lib")
done

export DEPLOY_GTK_VERSION=4
export LINUXDEPLOY_OUTPUT_VERSION=$version
export APPIMAGE_EXTRACT_AND_RUN=1
export NO_STRIP=1
"$(tool linuxdeploy-x86_64.AppImage || tool linuxdeploy)" \
  --appdir "$appdir" \
  --plugin gtk \
  --executable "$appdir/usr/bin/brujula" "${extra[@]}" \
  --desktop-file "$root/packaging/$id.desktop" \
  --icon-file "$root/packaging/icons/$id.svg" \
  --icon-filename "$id"

# TLS for HTTPS comes from glib-networking, a GIO module that is not linked by name.
giodir=$(pkg-config --variable=giomoduledir gio-2.0 2>/dev/null || echo /usr/lib/gio/modules)
if [ -e "$giodir/libgiognutls.so" ]; then
  mkdir -p "$appdir/usr/lib/gio/modules"
  cp -a "$giodir"/libgio*.so "$appdir/usr/lib/gio/modules/" 2>/dev/null || true
  "$(tool linuxdeploy-x86_64.AppImage || tool linuxdeploy)" --appdir "$appdir" \
    --library "$appdir/usr/lib/gio/modules/libgiognutls.so"
  echo 'export GIO_EXTRA_MODULES="$APPDIR/usr/lib/gio/modules"' >> "$appdir/apprun-hooks/brujula-webkit.sh"
else
  echo "glib-networking not found: the image would not open HTTPS pages" >&2
  exit 1
fi

# A launcher that is not the default AppRun is not needed: linuxdeploy's runs the hooks above.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$(tool appimagetool-x86_64.AppImage || tool appimagetool)" \
  "$appdir" "$out/Brujula-$version-x86_64.AppImage"
echo "$out/Brujula-$version-x86_64.AppImage"
