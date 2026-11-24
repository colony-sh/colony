//! Lowers a recorded [`anyrender::Scene`] into the small vocabulary a GPUI
//! window can paint with its own primitives, and reports what did not fit.
//!
//! Blitz paints a document by pushing commands into an `anyrender::PaintScene`.
//! `anyrender::Scene` records those commands. This crate walks the recording
//! and turns each command into an [`Op`] that maps one to one onto a GPUI paint
//! call (`paint_quad`, `paint_path`, `paint_glyph`, shadows, a rectangular
//! content mask), or into an [`Op::Gap`] when GPUI has nothing to paint it with.
//!
//! Nothing here depends on GPUI. Keeping the GPUI imports in one crate is what
//! makes it cheap to move to a different GPUI pin later.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use anyrender::recording::RenderCommand;
use anyrender::{BoxShadowKind, Paint, Scene};
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape, StrokeOpts, Vec2};
use peniko::{BlendMode, Color, Compose, Fill, FontData, GradientKind, Mix, Style};

/// Distance, in document pixels, under which two coordinates count as equal.
const EPS: f64 = 1e-3;
/// Flattening tolerance used when a stroke is expanded into a fill.
const STROKE_TOLERANCE: f64 = 0.1;

/// Straight (not premultiplied) sRGB color with alpha, each channel in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    fn from_color(color: Color, alpha: f32) -> Self {
        let [r, g, b, a] = color.components;
        Self {
            r,
            g,
            b,
            a: a * alpha,
        }
    }
}

/// One glyph of a run. `x` and `y` are the glyph's baseline origin in document pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// A run of glyphs from one font at one size and color.
#[derive(Clone, Debug)]
pub struct GlyphRun {
    /// The font bytes and face index Parley shaped with. Glyph ids index into this face.
    pub font: FontData,
    /// Font size in document pixels, after the run's transform.
    pub size: f32,
    pub color: Rgba,
    pub glyphs: Vec<PlacedGlyph>,
    /// True when the run was shaped at a non-default position on a variable font's axes.
    /// GPUI picks a face by family, weight and style, so such a run may paint at the wrong
    /// instance. See `Report::varied_runs`.
    pub varied: bool,
}

/// A CSS box shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// The box casting the shadow, in document pixels.
    pub rect: Rect,
    /// Corner radii: top left, top right, bottom right, bottom left.
    pub radii: [f32; 4],
    pub offset: (f32, f32),
    /// CSS blur radius (twice the gaussian standard deviation).
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
    pub inset: bool,
}

/// A paint operation GPUI can carry out directly, or a recorded gap.
#[derive(Clone, Debug)]
pub enum Op {
    /// Axis-aligned rectangle with a solid fill. Maps to `Window::paint_quad`.
    Quad { rect: Rect, color: Rgba },
    /// Any other solid fill, already transformed into document pixels. Maps to `Window::paint_path`.
    FillPath {
        path: BezPath,
        color: Rgba,
        even_odd: bool,
    },
    /// Maps to one `Window::paint_glyph` call per glyph.
    Glyphs(GlyphRun),
    /// Maps to `Window::paint_drop_shadows` or `Window::paint_inset_shadows`.
    Shadow(Shadow),
    /// Start clipping to a rectangle. Maps to `Window::with_content_mask`.
    PushClip { rect: Rect },
    /// End the innermost [`Op::PushClip`].
    PopClip,
    /// Something GPUI was not asked to paint. `bounds` is where it would have been.
    Gap { gap: Gap, bounds: Option<Rect> },
}

/// Why a command could not be turned into a native GPUI paint call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gap {
    /// Two-stop linear gradient. GPUI has `linear_gradient`; this spike does not map it yet.
    LinearGradientTwoStop,
    /// Image fill. GPUI has `paint_image`; this spike does not map it yet.
    ImageFill,
    /// Linear gradient with more than two stops.
    LinearGradientMultiStop,
    RadialGradient,
    SweepGradient,
    /// Texture or other renderer resource registered with the backend.
    ResourceFill,
    PatternFill,
    CustomPaint,
    /// Clip to anything but an axis-aligned rectangle, for example `overflow: hidden`
    /// with `border-radius`. GPUI content masks are rectangles.
    NonRectClip,
    /// A layer with opacity below one. Painted with each primitive's alpha scaled instead,
    /// which is only the same picture when nothing inside the layer overlaps.
    GroupOpacity,
    BlendMode,
    Filter,
    MaskLayer,
    /// Glyph run under a rotation, skew or non-uniform scale.
    GlyphTransform,
    /// Per-glyph transform, which is how synthetic italics are drawn.
    GlyphSkew,
    /// Outlined (stroked) text.
    GlyphStroke,
    /// Text painted with a gradient or image.
    GlyphNonSolid,
    /// Box shadow under a rotation, skew or non-uniform scale.
    ShadowTransform,
}

/// Whether a gap is missing work in this spike or missing from GPUI itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapClass {
    /// GPUI has a primitive for this. The lowering just does not use it yet.
    NotYetMapped,
    /// GPUI's scene has nothing for this. It needs a raster fallback or a renderer patch.
    NoPrimitive,
    /// Painted, but only approximately.
    Approximated,
}

impl Gap {
    pub fn class(self) -> GapClass {
        match self {
            Gap::LinearGradientTwoStop | Gap::ImageFill => GapClass::NotYetMapped,
            Gap::GroupOpacity => GapClass::Approximated,
            _ => GapClass::NoPrimitive,
        }
    }
}

/// Counts of what a scene lowered into.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Commands in the recorded scene, including layer pushes and pops.
    pub commands: usize,
    pub quads: usize,
    pub paths: usize,
    pub glyph_runs: usize,
    pub glyphs: usize,
    pub shadows: usize,
    pub rect_clips: usize,
    /// Fully transparent fills and strokes, dropped without painting.
    pub transparent: usize,
    /// Distinct font faces used by the glyph runs.
    pub fonts: usize,
    /// Glyph runs shaped at a non-default variable font instance.
    pub varied_runs: usize,
    pub gaps: BTreeMap<Gap, usize>,
}

impl Report {
    /// Draw commands that became a native GPUI paint call.
    pub fn native(&self) -> usize {
        self.quads + self.paths + self.glyph_runs + self.shadows + self.rect_clips
    }

    pub fn gaps_in(&self, class: GapClass) -> usize {
        self.gaps
            .iter()
            .filter(|(gap, _)| gap.class() == class)
            .map(|(_, count)| count)
            .sum()
    }

    /// Share of commands that painted natively and exactly, from `0.0` to `1.0`.
    /// Approximated group opacity does not count against it, because the content
    /// inside the layer still painted.
    pub fn coverage(&self) -> f32 {
        let missed = self.gaps_in(GapClass::NotYetMapped) + self.gaps_in(GapClass::NoPrimitive);
        let total = self.native() + missed;
        if total == 0 {
            return 1.0;
        }
        self.native() as f32 / total as f32
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "recorded commands   {}", self.commands)?;
        writeln!(f, "native paint calls  {}", self.native())?;
        writeln!(f, "  quads             {}", self.quads)?;
        writeln!(f, "  paths             {}", self.paths)?;
        writeln!(
            f,
            "  glyph runs        {} ({} glyphs, {} fonts)",
            self.glyph_runs, self.glyphs, self.fonts
        )?;
        writeln!(f, "  shadows           {}", self.shadows)?;
        writeln!(f, "  rect clips        {}", self.rect_clips)?;
        writeln!(f, "transparent, dropped {}", self.transparent)?;
        writeln!(f, "variable-font runs  {}", self.varied_runs)?;
        for (label, class) in [
            ("not yet mapped", GapClass::NotYetMapped),
            ("no GPUI primitive", GapClass::NoPrimitive),
            ("approximated", GapClass::Approximated),
        ] {
            writeln!(f, "{label}: {}", self.gaps_in(class))?;
            for (gap, count) in self.gaps.iter().filter(|(gap, _)| gap.class() == class) {
                writeln!(f, "  {gap:?} {count}")?;
            }
        }
        write!(f, "native coverage     {:.1}%", self.coverage() * 100.0)
    }
}

/// The result of lowering a scene.
#[derive(Clone, Debug, Default)]
pub struct Lowered {
    pub ops: Vec<Op>,
    pub report: Report,
}

struct Layer {
    /// Whether this layer emitted an [`Op::PushClip`] that its pop must close.
    clipped: bool,
    /// Opacity of everything painted inside, including enclosing layers.
    alpha: f32,
}

/// Lower a recorded scene. Coordinates in the result are the scene's own.
pub fn lower(scene: &Scene) -> Lowered {
    lower_scaled(scene, 1.0)
}

/// Lower a recorded scene, multiplying every coordinate and size by `factor`.
///
/// Blitz lays text out in device pixels, so a scene has to be recorded at the display's
/// scale factor for its text and boxes to agree. GPUI paints in logical pixels. Passing
/// `1.0 / scale_factor` here converts one to the other.
pub fn lower_scaled(scene: &Scene, factor: f64) -> Lowered {
    let outer = Affine::scale(factor);
    let mut out = Lowered::default();
    let mut layers: Vec<Layer> = Vec::new();
    let mut fonts: HashSet<(u64, u32)> = HashSet::new();
    out.report.commands = scene.commands.len();

    for command in &scene.commands {
        let alpha = layers.last().map_or(1.0, |layer| layer.alpha);
        match command {
            RenderCommand::PushLayer(cmd) => {
                let mut clip = cmd.clip.clone();
                clip.apply_affine(outer * cmd.transform);
                let bounds = Some(clip.bounding_box());
                let clipped = push_clip(&mut out, &clip);
                if !is_plain_blend(cmd.blend) {
                    gap(&mut out, Gap::BlendMode, bounds);
                }
                if cmd.filter.is_some() || cmd.backdrop_filter.is_some() {
                    gap(&mut out, Gap::Filter, bounds);
                }
                if cmd.alpha < 1.0 {
                    gap(&mut out, Gap::GroupOpacity, bounds);
                }
                layers.push(Layer {
                    clipped,
                    alpha: alpha * cmd.alpha,
                });
            }
            RenderCommand::PushClipLayer(cmd) => {
                let mut clip = cmd.clip.clone();
                clip.apply_affine(outer * cmd.transform);
                let clipped = push_clip(&mut out, &clip);
                layers.push(Layer { clipped, alpha });
            }
            RenderCommand::PushMaskLayer(cmd) => {
                let mut clip = cmd.clip.clone();
                clip.apply_affine(outer * cmd.transform);
                gap(&mut out, Gap::MaskLayer, Some(clip.bounding_box()));
                layers.push(Layer {
                    clipped: false,
                    alpha,
                });
            }
            RenderCommand::PopLayer => {
                if let Some(layer) = layers.pop() {
                    if layer.clipped {
                        out.ops.push(Op::PopClip);
                    }
                }
            }
            RenderCommand::Fill(cmd) => {
                let mut path = cmd.shape.clone();
                path.apply_affine(outer * cmd.transform);
                fill(&mut out, path, &cmd.brush, cmd.fill == Fill::EvenOdd, alpha);
            }
            RenderCommand::Stroke(cmd) => {
                // Expand the stroke in the shape's own space so the transform scales its width.
                let mut path = kurbo::stroke(
                    cmd.shape.elements().iter().copied(),
                    &cmd.style,
                    &StrokeOpts::default(),
                    STROKE_TOLERANCE,
                );
                path.apply_affine(outer * cmd.transform);
                fill(&mut out, path, &cmd.brush, false, alpha);
            }
            RenderCommand::GlyphRun(cmd) => {
                let Some((scale, offset)) = uniform_scale_translate(outer * cmd.transform) else {
                    gap(&mut out, Gap::GlyphTransform, None);
                    continue;
                };
                // `cmd.embolden` is deliberately ignored. Blitz sets it, behind its
                // `apple-font-embolden` feature, to imitate the stem darkening CoreText
                // applies. GPUI rasterizes through the platform text system, which does
                // that itself.
                let reason = if cmd.glyph_transform.is_some() {
                    Some(Gap::GlyphSkew)
                } else if !matches!(cmd.style, Style::Fill(_)) {
                    Some(Gap::GlyphStroke)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    gap(&mut out, reason, None);
                    continue;
                }
                let Paint::Solid(color) = &cmd.brush else {
                    gap(&mut out, Gap::GlyphNonSolid, None);
                    continue;
                };
                let varied = cmd.normalized_coords.iter().any(|coord| *coord != 0);
                let glyphs: Vec<PlacedGlyph> = cmd
                    .glyphs
                    .iter()
                    .map(|glyph| PlacedGlyph {
                        id: glyph.id,
                        x: (glyph.x as f64 * scale + offset.x) as f32,
                        y: (glyph.y as f64 * scale + offset.y) as f32,
                    })
                    .collect();
                out.report.glyph_runs += 1;
                out.report.glyphs += glyphs.len();
                out.report.varied_runs += usize::from(varied);
                fonts.insert((cmd.font_data.data.id(), cmd.font_data.index));
                out.ops.push(Op::Glyphs(GlyphRun {
                    font: cmd.font_data.clone(),
                    size: (cmd.font_size as f64 * scale) as f32,
                    color: Rgba::from_color(*color, alpha * cmd.brush_alpha),
                    glyphs,
                    varied,
                }));
            }
            RenderCommand::BoxShadow(cmd) => {
                let Some((scale, offset)) = uniform_scale_translate(outer * cmd.transform) else {
                    gap(&mut out, Gap::ShadowTransform, None);
                    continue;
                };
                let place = Affine::scale(scale).then_translate(offset);
                let radii = cmd.box_shape.radii;
                out.report.shadows += 1;
                out.ops.push(Op::Shadow(Shadow {
                    rect: place.transform_rect_bbox(cmd.box_shape.rect),
                    radii: [
                        (radii.top_left.x * scale) as f32,
                        (radii.top_right.x * scale) as f32,
                        (radii.bottom_right.x * scale) as f32,
                        (radii.bottom_left.x * scale) as f32,
                    ],
                    offset: ((cmd.offset.x * scale) as f32, (cmd.offset.y * scale) as f32),
                    blur: (cmd.std_dev * 2.0 * scale) as f32,
                    spread: (cmd.spread * scale) as f32,
                    color: Rgba::from_color(cmd.color, alpha),
                    inset: cmd.kind == BoxShadowKind::Inset,
                }));
            }
        }
    }

    // A well-formed scene pops every layer. Close anything left open so the ops stay balanced.
    while let Some(layer) = layers.pop() {
        if layer.clipped {
            out.ops.push(Op::PopClip);
        }
    }
    out.report.fonts = fonts.len();
    out
}

fn gap(out: &mut Lowered, gap: Gap, bounds: Option<Rect>) {
    *out.report.gaps.entry(gap).or_default() += 1;
    out.ops.push(Op::Gap { gap, bounds });
}

/// Emit a rectangular clip if `clip` is one. Returns whether a clip was pushed.
fn push_clip(out: &mut Lowered, clip: &BezPath) -> bool {
    match axis_aligned_rect(clip) {
        Some(rect) => {
            out.report.rect_clips += 1;
            out.ops.push(Op::PushClip { rect });
            true
        }
        None => {
            gap(out, Gap::NonRectClip, Some(clip.bounding_box()));
            false
        }
    }
}

fn fill(out: &mut Lowered, path: BezPath, brush: &Paint, even_odd: bool, alpha: f32) {
    let bounds = Some(path.bounding_box());
    let reason = match brush {
        Paint::Solid(color) => {
            let color = Rgba::from_color(*color, alpha);
            if color.a <= 0.0 || path.elements().is_empty() {
                out.report.transparent += 1;
            } else if let Some(rect) = axis_aligned_rect(&path) {
                out.report.quads += 1;
                out.ops.push(Op::Quad { rect, color });
            } else {
                out.report.paths += 1;
                out.ops.push(Op::FillPath {
                    path,
                    color,
                    even_odd,
                });
            }
            return;
        }
        Paint::Gradient(gradient) => match gradient.kind {
            GradientKind::Linear(_) if gradient.stops.len() <= 2 => Gap::LinearGradientTwoStop,
            GradientKind::Linear(_) => Gap::LinearGradientMultiStop,
            GradientKind::Radial(_) => Gap::RadialGradient,
            GradientKind::Sweep(_) => Gap::SweepGradient,
        },
        Paint::Image(_) => Gap::ImageFill,
        Paint::Resource(_) => Gap::ResourceFill,
        Paint::Custom(_) => Gap::CustomPaint,
        Paint::Pattern(_) => Gap::PatternFill,
    };
    gap(out, reason, bounds);
}

fn is_plain_blend(blend: BlendMode) -> bool {
    blend.mix == Mix::Normal && blend.compose == Compose::SrcOver
}

/// Split a transform into a positive uniform scale and a translation, if that is all it is.
fn uniform_scale_translate(transform: Affine) -> Option<(f64, Vec2)> {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    let plain = b.abs() < EPS && c.abs() < EPS && (a - d).abs() < EPS && a > 0.0;
    plain.then_some((a, Vec2::new(e, f)))
}

/// The rectangle a path traces, if it is a single closed run of four axis-aligned edges.
fn axis_aligned_rect(path: &BezPath) -> Option<Rect> {
    let elements = path.elements();
    let mut points: Vec<Point> = Vec::with_capacity(5);
    for (index, element) in elements.iter().enumerate() {
        match element {
            PathEl::MoveTo(point) if index == 0 => points.push(*point),
            PathEl::LineTo(point) if index > 0 => points.push(*point),
            PathEl::ClosePath if index + 1 == elements.len() => {}
            _ => return None,
        }
    }
    if points.len() == 5 && points[0].distance(points[4]) < EPS {
        points.pop();
    }
    if points.len() != 4 {
        return None;
    }
    // Edges must alternate between horizontal and vertical.
    let horizontal = |a: Point, b: Point| (a.y - b.y).abs() < EPS;
    let vertical = |a: Point, b: Point| (a.x - b.x).abs() < EPS;
    let first_horizontal = horizontal(points[0], points[1]);
    for index in 0..4 {
        let (a, b) = (points[index], points[(index + 1) % 4]);
        let expect_horizontal = first_horizontal == (index % 2 == 0);
        let ok = if expect_horizontal {
            horizontal(a, b)
        } else {
            vertical(a, b)
        };
        if !ok {
            return None;
        }
    }
    let rect = Rect::from_points(points[0], points[2]);
    (rect.width() > 0.0 && rect.height() > 0.0).then_some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyrender::{AntiAliasing, Glyph, PaintScene};
    use kurbo::{Circle, RoundedRect, Stroke};
    use peniko::{Blob, Gradient};
    use std::sync::Arc;

    const RED: Color = Color::from_rgba8(255, 0, 0, 255);

    fn fill_solid(scene: &mut Scene, transform: Affine, shape: &impl Shape) {
        scene.fill(
            Fill::NonZero,
            transform,
            RED,
            None,
            shape,
            AntiAliasing::Enabled,
        );
    }

    #[test]
    fn solid_rect_becomes_a_quad_in_document_space() {
        let mut scene = Scene::new();
        fill_solid(
            &mut scene,
            Affine::translate((10.0, 20.0)),
            &Rect::new(0.0, 0.0, 30.0, 40.0),
        );
        let lowered = lower(&scene);
        assert_eq!(lowered.report.quads, 1);
        match &lowered.ops[..] {
            [Op::Quad { rect, color }] => {
                assert_eq!(*rect, Rect::new(10.0, 20.0, 40.0, 60.0));
                assert_eq!(color.r, 1.0);
                assert_eq!(color.a, 1.0);
            }
            other => panic!("expected one quad, got {other:?}"),
        }
    }

    #[test]
    fn lower_scaled_converts_device_pixels_to_logical() {
        let mut scene = Scene::new();
        fill_solid(
            &mut scene,
            Affine::translate((20.0, 40.0)),
            &Rect::new(0.0, 0.0, 60.0, 80.0),
        );
        let font = FontData::new(Blob::new(Arc::new(Vec::<u8>::new())), 0);
        scene.draw_glyphs(
            &font,
            32.0,
            false,
            &[],
            Vec2::ZERO,
            Fill::NonZero,
            RED,
            1.0,
            Affine::translate((100.0, 50.0)),
            None,
            [Glyph {
                id: 3,
                x: 11.0,
                y: 0.0,
            }]
            .into_iter(),
            AntiAliasing::Enabled,
        );
        let lowered = lower_scaled(&scene, 0.5);
        match &lowered.ops[..] {
            [Op::Quad { rect, .. }, Op::Glyphs(run)] => {
                assert_eq!(*rect, Rect::new(10.0, 20.0, 40.0, 60.0));
                assert_eq!(run.size, 16.0);
                assert_eq!((run.glyphs[0].x, run.glyphs[0].y), (55.5, 25.0));
            }
            other => panic!("expected a quad and a glyph run, got {other:?}"),
        }
    }

    #[test]
    fn rounded_and_rotated_rects_become_paths() {
        let mut scene = Scene::new();
        fill_solid(
            &mut scene,
            Affine::IDENTITY,
            &RoundedRect::new(0.0, 0.0, 30.0, 40.0, 6.0),
        );
        fill_solid(
            &mut scene,
            Affine::rotate(0.3),
            &Rect::new(0.0, 0.0, 30.0, 40.0),
        );
        let lowered = lower(&scene);
        assert_eq!(lowered.report.quads, 0);
        assert_eq!(lowered.report.paths, 2);
    }

    #[test]
    fn stroke_becomes_a_filled_path() {
        let mut scene = Scene::new();
        scene.stroke(
            &Stroke::new(2.0),
            Affine::IDENTITY,
            RED,
            None,
            &Rect::new(0.0, 0.0, 30.0, 40.0),
            AntiAliasing::Enabled,
        );
        let lowered = lower(&scene);
        assert_eq!(lowered.report.paths, 1);
        assert_eq!(lowered.report.gaps.len(), 0);
    }

    #[test]
    fn rect_clip_is_balanced_and_round_clip_is_a_gap() {
        let mut scene = Scene::new();
        scene.push_clip_layer(
            Fill::NonZero,
            Affine::IDENTITY,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        scene.pop_layer();
        scene.push_clip_layer(
            Fill::NonZero,
            Affine::IDENTITY,
            &Circle::new((5.0, 5.0), 5.0),
        );
        scene.pop_layer();
        let lowered = lower(&scene);
        let pushes = lowered
            .ops
            .iter()
            .filter(|op| matches!(op, Op::PushClip { .. }))
            .count();
        let pops = lowered
            .ops
            .iter()
            .filter(|op| matches!(op, Op::PopClip))
            .count();
        assert_eq!((pushes, pops), (1, 1));
        assert_eq!(lowered.report.gaps.get(&Gap::NonRectClip), Some(&1));
    }

    #[test]
    fn layer_opacity_scales_alpha_and_is_reported() {
        let mut scene = Scene::new();
        scene.push_layer(
            Fill::NonZero,
            Mix::Normal,
            0.5,
            Affine::IDENTITY,
            &Rect::new(0.0, 0.0, 100.0, 100.0),
            None,
            None,
        );
        fill_solid(
            &mut scene,
            Affine::IDENTITY,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        scene.pop_layer();
        let lowered = lower(&scene);
        let alpha = lowered.ops.iter().find_map(|op| match op {
            Op::Quad { color, .. } => Some(color.a),
            _ => None,
        });
        assert_eq!(alpha, Some(0.5));
        assert_eq!(lowered.report.gaps.get(&Gap::GroupOpacity), Some(&1));
        assert_eq!(lowered.report.coverage(), 1.0);
    }

    #[test]
    fn gradients_are_gaps_sorted_by_class() {
        let mut scene = Scene::new();
        let two_stop = Gradient::new_linear((0.0, 0.0), (10.0, 0.0))
            .with_stops([RED, Color::from_rgba8(0, 0, 255, 255)]);
        let radial = Gradient::new_radial((5.0, 5.0), 5.0)
            .with_stops([RED, Color::from_rgba8(0, 0, 255, 255)]);
        for gradient in [&two_stop, &radial] {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                gradient,
                None,
                &Rect::new(0.0, 0.0, 10.0, 10.0),
                AntiAliasing::Enabled,
            );
        }
        let report = lower(&scene).report;
        assert_eq!(report.gaps_in(GapClass::NotYetMapped), 1);
        assert_eq!(report.gaps_in(GapClass::NoPrimitive), 1);
        assert_eq!(report.coverage(), 0.0);
    }

    #[test]
    fn glyph_run_is_scaled_and_translated() {
        let mut scene = Scene::new();
        let font = FontData::new(Blob::new(Arc::new(Vec::<u8>::new())), 0);
        scene.draw_glyphs(
            &font,
            16.0,
            false,
            &[],
            Vec2::ZERO,
            Fill::NonZero,
            RED,
            1.0,
            Affine::scale(2.0).then_translate(Vec2::new(100.0, 50.0)),
            None,
            [Glyph {
                id: 7,
                x: 10.0,
                y: 20.0,
            }]
            .into_iter(),
            AntiAliasing::Enabled,
        );
        let lowered = lower(&scene);
        match &lowered.ops[..] {
            [Op::Glyphs(run)] => {
                assert_eq!(run.size, 32.0);
                assert_eq!(
                    run.glyphs,
                    vec![PlacedGlyph {
                        id: 7,
                        x: 120.0,
                        y: 90.0
                    }]
                );
                assert!(!run.varied);
            }
            other => panic!("expected one glyph run, got {other:?}"),
        }
        assert_eq!(lowered.report.fonts, 1);
    }

    #[test]
    fn emboldened_run_still_lowers_to_glyphs() {
        let mut scene = Scene::new();
        let font = FontData::new(Blob::new(Arc::new(Vec::<u8>::new())), 0);
        scene.draw_glyphs(
            &font,
            16.0,
            false,
            &[],
            Vec2::new(0.2, 0.2),
            Fill::NonZero,
            RED,
            1.0,
            Affine::IDENTITY,
            None,
            [Glyph {
                id: 1,
                x: 0.0,
                y: 0.0,
            }]
            .into_iter(),
            AntiAliasing::Enabled,
        );
        let report = lower(&scene).report;
        assert_eq!(report.glyph_runs, 1);
        assert!(report.gaps.is_empty());
    }

    #[test]
    fn rotated_text_is_a_gap() {
        let mut scene = Scene::new();
        let font = FontData::new(Blob::new(Arc::new(Vec::<u8>::new())), 0);
        scene.draw_glyphs(
            &font,
            16.0,
            false,
            &[],
            Vec2::ZERO,
            Fill::NonZero,
            RED,
            1.0,
            Affine::rotate(0.5),
            None,
            std::iter::empty(),
            AntiAliasing::Enabled,
        );
        let report = lower(&scene).report;
        assert_eq!(report.gaps.get(&Gap::GlyphTransform), Some(&1));
    }
}
