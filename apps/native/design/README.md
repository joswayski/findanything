# Graphite for Find Anything

The three desktop clients use the Graphite design system from
[DBM PR #26](https://github.com/joswayski/dbm/pull/26), adapted to a launcher.
Database grids, inspectors, staged edits, and connection colors are not launcher
features. The website is separate from this desktop contract.

## One source of tokens

Edit `tokens.json`, then run:

```sh
uv run --no-project python apps/native/design/generate.py
uv run --no-project python apps/native/design/generate.py --check
```

The checked-in Rust constants and AppKit colors/dimensions are generated. CI
rejects stale output. Do not edit generated files or introduce per-platform color
palettes. AppKit consumes the Swift values; the shared Windows/Linux egui frontend
consumes the Rust values. Platform window management and launch behavior remain
behind target adapters.

## Shared presentation contract

- Graphite is deliberately dark in both light and dark host appearances. This
  is the reference design, not three independent interpretations of OS themes.
- Canvas `#161618`, chrome `#1c1c1e`, controls `#2a2a2d`, subtle borders `#353538`.
  Strong text `#f5f5f7`, metadata `#a0a0a6`, focus/selection edge `#4c9aff`.
  Selection wash `#1e2838` is the reference's 14% accent over the canvas.
- Default content is 680 × 440 logical units, with a minimum of 560 × 360,
  20-unit insets and 8-unit gaps. A 52-unit search field uses 18-unit text.
- Results are 60 units high: 16-unit semibold title above 12-unit metadata
  (`subtitle · reason`), a neutral 20-unit stroke icon, and a 2-unit blue
  selection edge. Do not merge metadata into titles or use colored initial tiles.
- The 28-unit footer has indexing/search mode on the left and navigation hints
  on the right. Empty/error messages belong in the center of the result area.
- Use sentence case: “Apps & actions”, “Best matches”, “Keyword mode”.
- Preserve keyboard editing and selection behavior. Blue denotes focus/selection;
  red denotes an error, not decoration. No animated transitions are required.

All platforms bundle and use static Inter 4.1 Regular and SemiBold from the
[official release](https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip)
(`extras/ttf`, unchanged). The font license is shipped with every package; see
[`fonts/README.md`](fonts/README.md). No fonts or icons are fetched at runtime.
All clients use the shared Lucide icons below. Native title bars and system menus may differ;
macOS keeps its existing menu-bar controls, while Windows/Linux use a header menu.
Windows high-contrast colors take precedence at startup.

## One icon set

The six SVGs in `lucide/` are vendored unchanged from Lucide 1.48.0,
[commit f53e5bf](https://github.com/lucide-icons/lucide/tree/f53e5bfff0f909f3f451933538744330650d3ce0/icons).
Its full ISC and Feather-derived MIT notices are in `lucide/LICENSE` and shipped
as `Lucide-LICENSE.txt` in every native package.

| Role | Lucide icon | Logical size |
| --- | --- | --- |
| Application result | `app-window` | 20 |
| File result | `file` | 20 |
| System action result | `sliders-horizontal` | 20 |
| Search / macOS status item | `search` | 16 |
| Header menu (Windows/Linux) | `menu` | 16 |
| Clear search | `x` | 16 |

Keep the 24-unit view box, 2-unit stroke, round caps/joins, and neutral secondary
color. Do not substitute SF Symbols, theme icons, emoji, or hand-drawn variants.
The generator compiles the SVG subset to identical Rust/Swift paths. AppKit draws
the Swift paths with its drawing APIs; the shared egui frontend draws the Rust
paths. Rasterization can differ, but the source geometry does not.

```sh
uv run --no-project python apps/native/design/generate_icons.py
uv run --no-project python apps/native/design/generate_icons.py --check
```

CI checks generated output. New icons must come from this pinned source (or an
explicitly reviewed upgrade), extend the role mapping, and render on all clients.

## Review before extending UI

Any new component or state must be considered on **all three clients**. Update
this contract and shared tokens first; platform-specific code is for native API
integration, not a separate design direction. Token checks prevent value drift,
but cannot prove that widgets use the tokens correctly: inspect native captures.

Use the existing smoke tools documented in `DEVELOPMENT.md`. Compare the same
Browser / Displays / Project notes fixtures, selection, typed query, empty and
error states, and minimum size. Run under both host appearances. CI uploads
native screenshots for macOS ARM64/Intel, Windows, and Linux. Cross-compilation
is not visual verification. Physical IME, screen readers, mixed-DPI movement,
Wayland, and live accessibility-theme changes need target-machine acceptance.
