#!/usr/bin/env bash
# Regenerates every application icon from design/icons/app-icon.svg.
#
# The SVG is the source; nothing under src-tauri/icons/ is edited by hand.
# Needs rsvg-convert and ImageMagick (brew install librsvg imagemagick) and,
# for the .icns, macOS's iconutil.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
svg="$root/design/icons/app-icon.svg"
out="$root/src-tauri/icons"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

rsvg-convert -w 1024 -h 1024 "$svg" -o "$work/master.png"

# macOS insets the artwork inside the canvas; matching that keeps the Dock icon
# the same visual size as every other app's.
magick "$work/master.png" -resize 824x824 -background none -gravity center \
  -extent 1024x1024 "$work/macos.png"

mkdir -p "$work/rbl.iconset"
for size in 16 32 128 256 512; do
  magick "$work/macos.png" -resize "${size}x${size}" \
    "$work/rbl.iconset/icon_${size}x${size}.png"
  magick "$work/macos.png" -resize "$((size * 2))x$((size * 2))" \
    "$work/rbl.iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$work/rbl.iconset" -o "$out/icon.icns"

# Windows carries every size the shell asks for, full-bleed.
magick "$work/master.png" -define icon:auto-resize=256,128,64,48,32,16 "$out/icon.ico"

rsvg-convert -w 32  -h 32  "$svg" -o "$out/32x32.png"
rsvg-convert -w 128 -h 128 "$svg" -o "$out/128x128.png"
rsvg-convert -w 256 -h 256 "$svg" -o "$out/128x128@2x.png"
rsvg-convert -w 512 -h 512 "$svg" -o "$out/icon.png"

for entry in 30:Square30x30Logo 44:Square44x44Logo 71:Square71x71Logo \
             89:Square89x89Logo 107:Square107x107Logo 142:Square142x142Logo \
             150:Square150x150Logo 284:Square284x284Logo 310:Square310x310Logo \
             50:StoreLogo; do
  rsvg-convert -w "${entry%%:*}" -h "${entry%%:*}" "$svg" -o "$out/${entry##*:}.png"
done

echo "icons regenerated from $(basename "$svg")"
