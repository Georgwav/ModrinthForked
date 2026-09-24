#!/bin/sh
# Installs or updates Threadrinth on Linux from the latest GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/Georgwav/Threadrinth/main/scripts/install.sh | sh
#
# Installs the AppImage (which updates itself) into ~/.local, adds a
# `threadrinth` command and a menu entry. No root needed. The download is
# checked against the SHA-256 GitHub publishes for it.
set -eu

REPO="Georgwav/Threadrinth"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/threadrinth"
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_URL="https://raw.githubusercontent.com/$REPO/main/apps/app/icons/128x128@2x.png"

fail() {
	echo "Error: $*" >&2
	exit 1
}

[ "$(uname -s)" = "Linux" ] || fail "this script is for Linux. On Windows, see the README."
[ "$(uname -m)" = "x86_64" ] || fail "only x86_64 builds are published."
command -v curl >/dev/null || fail "curl is required."

echo "Finding the latest Threadrinth release..."
release=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest") ||
	fail "could not reach GitHub."

# The asset's digest comes before its download URL in the API response.
set -- $(printf '%s' "$release" | tr ',{}' '\n\n\n' | awk '
	/"digest": *"sha256:/ { sub(/.*sha256:/, ""); sub(/".*/, ""); digest = $0 }
	/"browser_download_url": *".*_amd64\.AppImage"/ {
		sub(/.*"browser_download_url": *"/, ""); sub(/".*/, "")
		print $0, digest; exit
	}')
url=${1:-}
sha256=${2:-}
[ -n "$url" ] || fail "the latest release has no Linux AppImage."

mkdir -p "$APP_DIR" "$BIN_DIR" "$DESKTOP_DIR"
tmp="$APP_DIR/Threadrinth.AppImage.download"
trap 'rm -f "$tmp"' EXIT

echo "Downloading $(basename "$url")..."
curl -fL --progress-bar "$url" -o "$tmp" || fail "download failed."

if [ -n "$sha256" ]; then
	echo "$sha256  $tmp" | sha256sum -c --status - || fail "the download is corrupted (checksum mismatch)."
fi

chmod +x "$tmp"
mv "$tmp" "$APP_DIR/Threadrinth.AppImage"
ln -sf "$APP_DIR/Threadrinth.AppImage" "$BIN_DIR/threadrinth"
curl -fsSL "$ICON_URL" -o "$APP_DIR/icon.png" || true

cat >"$DESKTOP_DIR/threadrinth.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Threadrinth
Comment=Minecraft launcher
Exec=$APP_DIR/Threadrinth.AppImage %u
Icon=$APP_DIR/icon.png
Categories=Game;
Terminal=false
EOF

echo "Threadrinth is installed. Start it from your app menu or run: threadrinth"
case ":$PATH:" in
*":$BIN_DIR:"*) ;;
*) echo "Note: $BIN_DIR is not on your PATH, so the threadrinth command needs its full path." ;;
esac
