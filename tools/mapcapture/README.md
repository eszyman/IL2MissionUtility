# Editor map capture

Photographs the Korea map from the IL-2 mission editor at whatever zoom is on
screen, then stitches the shots into one image aligned to mission coordinates.
The status bar under the map (`Mouse(m)` and `Camera (X Z Zoom)`) is how the
scale is measured. Marked woods are whatever that view draws, so a later
placement pass can stay out of them. Hide the grid (Ctrl+G) if you do not
want grid lines in the picture.

The editor has to stay visible. A maximized window is left maximized. The
script opens View → Move Camera to (the Ctrl+M command) and fills X and Z.
Press Esc to stop. Run the same command again to resume.

## Once, after the editor opens

1. Open an empty Korea mission. Leave the editor visible and uncovered.
2. `py -3 tools/mapcapture/capture.py init`

   This presses Alt+F9 twice and then stops. Do this before framing the shot.
3. Set the flat 2D view and zoom all the way in. Leave the marked woods visible.
   Hide icons and the grid. Move Mission Properties off the map.

## A short check, then the full map

The south edge is empty, so a stitch check there cannot show whether the
tiles meet. `--sample` photographs a 2 by 2 around the middle of the map and
writes it under `References/MapCapture/sample`, apart from the full-map tiles.

Set the zoom you want to judge, leave the editor in front, then:

```text
py -3 tools/mapcapture/capture.py capture --sample
```

Open `References/MapCapture/sample/korea_editor_map_preview.png`. The seams
should meet. Then the full map:

```text
py -3 tools/mapcapture/capture.py capture --yes
```

Open water is filled in, not photographed. `--include-sea` captures it too.
`--x0 --x1 --z0 --z1` limit the run to one rectangle, in meters (X north from
the south edge, Z east from the west edge).

## Output

`References/MapCapture/` (git-ignored):

| File | What |
|---|---|
| `tiles/` | One PNG per view, plus `index.jsonl` |
| `korea_editor_map.png` or `.tif` | The stitched image, when it fits in `--max-gb` (default 16 GB uncompressed) |
| `korea_editor_map.json` | `Z = il2_z_west + col * meters_per_pixel_z`, `X = il2_x_north - row * meters_per_pixel_x` |
| `korea_editor_map_preview.png` | The same image scaled to 4096 px |

A partial run writes `korea_editor_map_sample.png` instead. `stitch` rebuilds
the image from the tiles without touching the editor.

`py -3 tools/mapcapture/capture.py selftest` checks the stitch math, a screen
grab, and Windows OCR. It does not send keys.

## Woods mask

Placement does not open these tiles. After a capture, bake the olive woods
into `assets/combined_terrain.bin` (bit 8). Water and road bits stay as they
are. The open bit is cleared on woods and set on beige fields. Run it again
after MapHelper rebuilds the bin, because MapHelper does not write bit 8.

```text
py -3 tools/mapcapture/bake_woods.py
```
