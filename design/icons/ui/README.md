# Interface icons

Our own drawings. Pioneer's SVGs under
`/Applications/rekordbox 7/rekordbox.app/Contents/Resources/skins/` are
**reference for geometry only** and are never shipped or copied — where a size
or proportion here matches theirs, it was measured and redrawn, not lifted.

Each file is a bare `<svg>` with `viewBox`, no width or height, and every
paintable attribute set to `currentColor` so the CSS token drives the colour.

`pnpm icons` regenerates `src/components/icons.tsx` from this directory. Edit
the SVGs, never the generated file.
