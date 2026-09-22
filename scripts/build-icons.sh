#!/usr/bin/env bash
# Regenerates every application icon from the brand app icon.
#
# The PNG under docs/rbxport-brand-assets is the source (the brand package
# has no vector original); nothing under src-tauri/icons/ is edited by hand.
# Needs ImageMagick (brew install imagemagick) and, for the .icns, macOS's
# iconutil.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/docs/rbxport-brand-assets/app-icons/rbxport-app-icon-color-dark.png"
out="$root/src-tauri/icons"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The brand PNG carries its own transparent margin, plus stray alpha-1 pixels
# well outside the artwork that fool a plain -trim. Crop to where the alpha
# is real so the rounded square fills the master edge to edge; every output
# below chooses its own inset from there.
artwork="$(magick "$src" -alpha extract -threshold 3% -format '%@' info:)"
magick "$src" -crop "$artwork" +repage -background none -gravity center \
  -extent '%[fx:max(w,h)]x%[fx:max(w,h)]' -resize 1024x1024 "$work/master.png"

# The brand artwork's corners are tighter than Apple's icon grid (radius 22.5 %
# of the side). Re-mask to Apple's radius so it sits next to other Dock icons
# as one of them; only background is lost in the corners.
magick "$work/master.png" \
  \( -size 1024x1024 xc:none -fill white \
     -draw "roundrectangle 0,0 1023,1023 230,230" \) \
  -compose DstIn -composite "$work/master.png"

# macOS insets the artwork inside the canvas (824 of 1024); matching that
# keeps the Dock icon the same visual size as every other app's.
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

for entry in 32:32x32 128:128x128 256:128x128@2x 512:icon \
             30:Square30x30Logo 44:Square44x44Logo 71:Square71x71Logo \
             89:Square89x89Logo 107:Square107x107Logo 142:Square142x142Logo \
             150:Square150x150Logo 284:Square284x284Logo 310:Square310x310Logo \
             50:StoreLogo; do
  magick "$work/master.png" -resize "${entry%%:*}x${entry%%:*}" \
    "$out/${entry##*:}.png"
done

echo "icons regenerated from $(basename "$src")"
