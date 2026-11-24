# Spike: HTML painted with GPUI primitives

Colony needs a browser primitive. The direction under test is a composed engine: Colony owns the DOM and the agent interface, and borrows parsing (html5ever), style (Stylo), layout (Taffy, Parley) and paint structure from Blitz. This spike asks whether the last step, paint, can go through GPUI's own primitives instead of a second renderer.

## The question

Can a Blitz document paint inside a GPUI window as GPUI quads, paths, glyphs and shadows, with text as crisp as GPUI's own text?

Two things decide it:

1. **Text.** Parley shapes glyph runs against its own copy of each font. GPUI paints glyphs by id through the platform text system. The ids only agree if both sides hold the same face.
2. **The primitive gap.** GPUI's scene is a small fixed vocabulary. CSS asks for more: clips that are not rectangles, radial and multi-stop gradients, rotated text, blend modes, filters.

## Status

Not yet run in a real window. Everything below was built and tested on Linux without a display.

| Check | Result |
| --- | --- |
| `colony-html-paint` unit tests | 10 pass |
| `colony-html-doc` unit tests | 4 pass |
| `gap-report` on `pages/sample.html` | runs, numbers below |
| `colony-html-gpui` library and `static_page` example | type-check against the pinned GPUI commit |
| `colony-html-gpui` headless tests (`tests/headless.rs`) | 2 pass: the element lays out, paints every op and accounts for every glyph |
| `static_page` in a window on macOS | **not run** |

Two caveats on the GPUI rows:

- The build machine had Rust 1.97 and could not install 1.100, which the pinned commit needs. GPUI was compiled from a local checkout of that exact commit with one line of `gpui_macros` changed (`str::trim_prefix` swapped for `strip_prefix`), and without the `wayland` and `x11` features. The unmodified git dependency has been resolved (see `crates/gpui-view/Cargo.lock`) but not compiled.
- The headless test window uses GPUI's stub text system, so "fonts mapped" there says nothing about CoreText. The macOS run is the real test of text.

## First measurement

`cargo run --bin gap-report` on the bundled sample page, 900 logical pixels wide:

```
recorded commands   119
native paint calls  106
  quads             4
  paths             48
  glyph runs        43 (572 glyphs, 5 fonts)
  shadows           6
  rect clips        5
not yet mapped: 1
  LinearGradientTwoStop 1
no GPUI primitive: 5
  RadialGradient 2
  NonRectClip 2
  GlyphTransform 1
approximated: 1
  GroupOpacity 1
native coverage     94.6%
```

The five commands with no GPUI primitive are the two avatar circles (a radial gradient under `overflow: hidden` with `border-radius`) and the rotated "PARKED" stamp. Those are the cases that need either a raster fallback or a renderer patch.

## Run it

There are two Cargo workspaces here, so the half that does not need GPUI never fetches or compiles it.

```sh
cd spikes/gpui-html

# No GPUI, no display. Any recent stable Rust.
cargo test
cargo run --bin gap-report -- [PAGE.html] [WIDTH]
```

```sh
cd spikes/gpui-html/crates/gpui-view

# Needs Rust 1.100 or newer and GPUI's platform requirements (on macOS, Xcode).
# The first build fetches Zed, about 700 MB, and takes a while.
cargo run --example static_page -- [PAGE.html]
COLONY_HTML_GAPS=1 cargo run --example static_page

# Headless GPUI tests.
cargo test
```

`COLONY_HTML_GAPS=1` outlines in magenta every region that did not paint natively.

## What to look for

The window writes one line to stderr whenever what it painted changes:

```
colony-html: 106 native ops, 94.6% coverage; fonts 5/5 mapped; glyphs 572 painted, 0 unmapped, 0 failed; 0 paths failed
```

The spike passes if all of these hold on macOS:

- Every font maps (`fonts N/N mapped`) and no glyphs are unmapped or failed.
- The text reads correctly. Scrambled letters mean GPUI resolved a different face than Parley shaped with.
- Text is as sharp as text in a native GPUI element at the same size.
- Lines break where the same page breaks in a browser, or close to it.
- Scrolling the sample page holds the display's frame rate.

If text fails, the fallback is to let Vello draw the document into a texture GPUI composites. That path wants a GPUI built on wgpu on every platform, which upstream is not on macOS, so a failure here also reopens the choice of GPUI pin.

## Layout

```
Cargo.toml                            Workspace: crates/paint and crates/doc.
crates/paint      colony-html-paint   anyrender::Scene -> Op list + gap report. No GPUI, no Blitz.
crates/doc        colony-html-doc     HTML string -> Blitz layout -> lowered ops. No GPUI.
crates/gpui-view  colony-html-gpui    HtmlView and HtmlElement. Its own workspace. The only crate that imports GPUI.
pages/sample.html                     A report page of the kind Colony's agents will produce.
```

GPUI is imported in one crate on purpose. Moving to a different GPUI pin or fork should touch `crates/gpui-view` and nothing else.

How a frame is produced:

1. `HtmlElement::request_layout` asks GPUI for a measured node. The measure callback gives Blitz the width and the display's scale factor, and returns the document's height.
2. `HtmlDoc::lowered` records Blitz's paint commands into an `anyrender::Scene` at the display's scale factor, then lowers them with every coordinate divided by that factor. The result is in logical pixels, GPUI's unit, and is cached per width and scale.
3. `HtmlElement::paint` replays the ops: `paint_quad`, `paint_path`, `paint_glyph`, `paint_drop_shadows`, `paint_inset_shadows`, and `with_content_mask` for rectangular clips.

Scrolling is GPUI's: the element sits in a `div` with `overflow_y_scroll`.

## Found while building

- **Blitz lays text out in device pixels.** Its paint pass scales boxes by the scale factor but not text, which is already scaled. Recording the scene at scale 1.0 on a 2x display gives text twice the size of its boxes. `text_stays_in_logical_pixels_at_any_scale` in `crates/doc` guards the fix.
- **Blitz can embolden glyphs to imitate CoreText.** With blitz-paint's `apple-font-embolden` feature on, every glyph run on macOS carries a nonzero embolden. The lowering ignores it, because GPUI rasterizes through CoreText, which darkens stems itself. The feature is off by default and nothing here turns it on.
- **GPUI falls back to another font without saying so.** `TextSystem::resolve_font` always returns an id. `resolve_face` in `crates/gpui-view` only accepts an id that GPUI resolved directly to the family asked for, and counts the rest as unmapped.
- **GPUI quads take one border color and content masks are rectangles.** Per-side border colors and rounded clips cannot be quads or masks. Blitz already draws borders as filled paths, so only the clips show up as gaps.

## The GPUI pin

`crates/gpui-view/Cargo.toml` pins GPUI to upstream `zed-industries/zed` at `0be6d5990f9f6e3917464603a99808502cc824af` (`main` on 2026-11-24).

Upstream was chosen because every fork trails it and `gpui-component` targets it. The plan is to depend on it through `colony-sh/zed`, so renderer patches can live on a branch Colony controls. That fork has not been synced since March 2026. Once it is synced past the commit above, change the three `git =` URLs to the fork and regenerate the lockfile.

## Known limits

- Static documents only. No events, no hit testing, no selection, no focus, no accessibility tree.
- No network provider, so external stylesheets, images and fonts are not loaded.
- Every op is replayed every frame with no culling. Fine for the sample page, not for a long one.
- Rounded rectangles go through `paint_path`. They could be quads with corner radii, which are cheaper.
- Two-stop linear gradients and images are reported as gaps although GPUI can paint both. They are not mapped yet.
- Group opacity is approximated by scaling each primitive's alpha. That is only correct when nothing inside the layer overlaps.
- Text on a variable font is painted at whatever instance GPUI picks for the face's default weight and style. The report counts such runs as `variable-font runs`.
- Gaps are skipped, not rasterized. There is no fallback path yet.

## Next

If the macOS run passes: add the raster fallback for gaps, map gradients and images, then start on the DOM Colony owns.

If text fails: try the shared-texture path before giving up on a GPUI engine.
