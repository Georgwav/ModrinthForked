#!/usr/bin/env bash
# Rebuilds the app icons and installer artwork from the SVGs in this folder.
# Requires: python3, rsvg-convert (librsvg), ImageMagick (convert), png2icns (icnsutils).
set -euo pipefail
cd "$(dirname "$0")"
python3 generate.py >/dev/null

ICONS=../apps/app/icons
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

png() { rsvg-convert -w "$1" -h "$1" threadrinth-icon.svg -o "$2"; }

png 128 "$ICONS/128x128.png"
png 256 "$ICONS/128x128@2x.png"
png 512 "$ICONS/icon.png"
png 50 "$ICONS/StoreLogo.png"
for size in 30 44 71 89 107 142 150 284 310; do
	png "$size" "$ICONS/Square${size}x${size}Logo.png"
done

for size in 16 24 32 48 64 256; do png "$size" "$tmp/ico-$size.png"; done
convert "$tmp"/ico-{16,24,32,48,64,256}.png "$ICONS/icon.ico"
png 512 "$tmp/icns-512.png"
png2icns "$ICONS/icon.icns" "$tmp/icns-512.png" >/dev/null

# macOS 26 Icon Composer layer (the composer draws the background).
mkdir -p "$ICONS/apple.icon/Assets"
cp threadrinth-mark.svg "$ICONS/apple.icon/Assets/Threadrinth mark.svg"

# DMG installer background (802x488, app at 188,212 and Applications at 475,212
# in the 661x432 window).
wordmark_inner=$(sed -e '1d' -e '$d' threadrinth-wordmark.svg)
cat >"$tmp/dmg.svg" <<SVG
<svg xmlns="http://www.w3.org/2000/svg" width="802" height="488" viewBox="0 0 802 488">
<defs><radialGradient id="bg" cx=".5" cy=".35" r=".8"><stop offset="0" stop-color="#1D232D"/><stop offset="1" stop-color="#0C0F13"/></radialGradient></defs>
<rect width="802" height="488" fill="url(#bg)"/>
<svg x="163" y="48" width="320" height="100" viewBox="0 0 1640 512">$wordmark_inner</svg>
<path d="M292 178 Q 330 150 366 170" fill="none" stroke="#E6EDF3" stroke-width="4" stroke-linecap="round"/>
<path d="M352 160 L 367 170 L 350 175" fill="none" stroke="#E6EDF3" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>
<rect x="106" y="282" width="158" height="29" rx="14.5" fill="#FFFFFF" fill-opacity=".12" stroke="#FFFFFF" stroke-opacity=".2"/>
<rect x="395" y="282" width="158" height="29" rx="14.5" fill="#FFFFFF" fill-opacity=".12" stroke="#FFFFFF" stroke-opacity=".2"/>
</svg>
SVG
rsvg-convert "$tmp/dmg.svg" -o ../apps/app/dmg/dmg-background.png
echo "Assets regenerated."
