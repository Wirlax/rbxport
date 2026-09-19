# Interface icons

Our own drawings. Pioneer's SVGs under
`/Applications/rekordbox 7/rekordbox.app/Contents/Resources/skins/` are
**reference for geometry only** and are never shipped or copied — where a size
or proportion here matches theirs, it was measured and redrawn, not lifted.

Each file is a bare `<svg>` with `viewBox`, no width or height, and every
paintable attribute set to `currentColor` so the CSS token drives the colour.

`pnpm icons` regenerates `src/components/icons.tsx` from this directory. Edit
the SVGs, never the generated file.

An icon with more than one colour still paints everything in `currentColor`
and marks the other parts with a `class` — `dim` for beat lines, `faint` for
the beats an edit leaves alone, `head` for the downbeat's red — which the
stylesheet of whatever shows the icon maps to a token. The `grid-*` icons are
drawn in their button's 48 x 46 box (2x), so they fill the button and land
where the capture has them.
