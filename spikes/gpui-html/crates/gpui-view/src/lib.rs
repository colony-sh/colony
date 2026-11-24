//! Paints an HTML document inside a GPUI window using GPUI's own primitives.
//!
//! This is the only crate in the spike that imports GPUI. [`HtmlView`] is a
//! scrollable view. [`HtmlElement`] is the leaf element inside it: GPUI sees one
//! node with a measured height, and everything inside is laid out by Blitz and
//! replayed from [`colony_html_paint::Op`]s.
//!
//! The open question this crate exists to answer is text. Parley shaped the
//! glyph runs against its own copy of each font. GPUI paints glyphs by id
//! through the platform text system. The ids only mean the same thing on both
//! sides if both sides hold the same face, so [`FontMap`] registers Parley's
//! font bytes with GPUI and refuses any face GPUI resolved by falling back.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::Location;
use std::rc::Rc;

use colony_html_doc::HtmlDoc;
use colony_html_paint::{GlyphRun, Op, Report, Rgba, Shadow};
use gpui::{
    div, fill, outline, point, prelude::*, px, relative, size, App, AvailableSpace, BorderStyle,
    Bounds, BoxShadow, ContentMask, Context, Corners, ElementId, FillOptions, FillRule, Font,
    FontFeatures, FontId, FontStyle, FontWeight, GlobalElementId, GlyphId, Hsla,
    InspectorElementId, LayoutId, PathBuilder, PathStyle, Pixels, Point, Style, Window,
};
use kurbo::{BezPath, PathEl, Rect};
use peniko::FontData;

/// Width used when GPUI asks for a size without offering a definite width.
const FALLBACK_WIDTH: Pixels = px(800.0);

/// What one paint pass did, for judging the spike.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintStats {
    /// Glyphs handed to `Window::paint_glyph` without an error.
    pub glyphs_painted: usize,
    /// Glyphs skipped because GPUI could not resolve their exact face.
    pub glyphs_unmapped: usize,
    /// Glyphs `Window::paint_glyph` rejected.
    pub glyphs_failed: usize,
    /// Paths GPUI's builder rejected.
    pub paths_failed: usize,
    /// Distinct faces GPUI resolved exactly.
    pub fonts_mapped: usize,
    /// Distinct faces GPUI could not resolve exactly.
    pub fonts_unmapped: usize,
}

/// Maps the font faces Parley shaped with onto GPUI font ids.
#[derive(Default)]
struct FontMap {
    /// Keyed by font blob id and face index. `None` records a face GPUI could not match,
    /// so the lookup is not retried every frame.
    faces: HashMap<(u64, u32), Option<FontId>>,
}

impl FontMap {
    fn font_id(&mut self, font: &FontData, cx: &App) -> Option<FontId> {
        *self
            .faces
            .entry((font.data.id(), font.index))
            .or_insert_with(|| resolve_face(font, cx))
    }

    fn counts(&self) -> (usize, usize) {
        let mapped = self.faces.values().filter(|id| id.is_some()).count();
        (mapped, self.faces.len() - mapped)
    }
}

/// Find the GPUI font id for exactly this face, registering its bytes if GPUI has
/// not seen the family.
fn resolve_face(font: &FontData, cx: &App) -> Option<FontId> {
    let bytes = font.data.data();
    let face = ttf_parser::Face::parse(bytes, font.index).ok()?;
    let families = family_names(&face);
    if families.is_empty() {
        return None;
    }

    let text_system = cx.text_system();
    let known = text_system.all_font_names();
    if !families.iter().any(|family| known.contains(family)) {
        // Hand GPUI the same bytes Parley shaped with, so glyph ids agree.
        text_system
            .add_fonts(vec![Cow::Owned(bytes.to_vec())])
            .ok()?;
    }

    let style = if face.is_italic() {
        FontStyle::Italic
    } else if face.is_oblique() {
        FontStyle::Oblique
    } else {
        FontStyle::Normal
    };
    for family in families {
        let wanted = Font {
            family: family.clone().into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight(face.weight().to_number() as f32),
            style,
        };
        let id = text_system.resolve_font(&wanted);
        // `resolve_font` substitutes a fallback face without saying so, and a different
        // face means different glyph ids. Only accept an id GPUI resolved directly to
        // the family that was asked for.
        let direct = text_system
            .get_font_for_id(id)
            .is_some_and(|found| found.family.eq_ignore_ascii_case(&family));
        if direct {
            return Some(id);
        }
    }
    None
}

/// Family names a face goes by, typographic family first.
fn family_names(face: &ttf_parser::Face<'_>) -> Vec<String> {
    let mut names = Vec::new();
    for wanted in [
        ttf_parser::name_id::TYPOGRAPHIC_FAMILY,
        ttf_parser::name_id::FAMILY,
    ] {
        for name in face.names() {
            if name.name_id != wanted {
                continue;
            }
            if let Some(family) = name.to_string() {
                if !family.is_empty() && !names.contains(&family) {
                    names.push(family);
                }
            }
        }
    }
    names
}

struct State {
    doc: HtmlDoc,
    fonts: FontMap,
    show_gaps: bool,
    stats: PaintStats,
    /// The last report and stats written to stderr, so each change is logged once.
    logged: Option<(usize, PaintStats)>,
}

/// A scrollable view of one static HTML document.
pub struct HtmlView {
    state: Rc<RefCell<State>>,
}

impl HtmlView {
    /// Parse `html`. Subresources are not fetched.
    pub fn new(html: &str) -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                doc: HtmlDoc::from_html(html),
                fonts: FontMap::default(),
                show_gaps: false,
                stats: PaintStats::default(),
                logged: None,
            })),
        }
    }

    /// Outline, in magenta, every region the lowering could not paint natively.
    pub fn show_gaps(self, show: bool) -> Self {
        self.state.borrow_mut().show_gaps = show;
        self
    }

    /// Stats from the most recent paint.
    pub fn stats(&self) -> PaintStats {
        self.state.borrow().stats
    }

    /// The lowering report for the most recent layout.
    pub fn report(&self) -> Report {
        self.state.borrow_mut().doc.lowered().report.clone()
    }

    /// Height of the document, in logical pixels, at the most recent layout.
    pub fn document_height(&self) -> f32 {
        self.state.borrow().doc.height()
    }
}

impl Render for HtmlView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("colony-html-page")
            .size_full()
            .overflow_y_scroll()
            .bg(gpui::white())
            .child(HtmlElement {
                state: self.state.clone(),
            })
    }
}

/// The leaf element that measures and paints the document.
pub struct HtmlElement {
    state: Rc<RefCell<State>>,
}

impl Element for HtmlElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let state = self.state.clone();
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, _cx| {
                let width = known
                    .width
                    .or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
                    })
                    .unwrap_or(FALLBACK_WIDTH);
                let height = state
                    .borrow_mut()
                    .doc
                    .layout(width.into(), window.scale_factor());
                size(width, px(height))
            });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut state = self.state.borrow_mut();
        let State {
            doc,
            fonts,
            show_gaps,
            stats,
            logged,
        } = &mut *state;

        // Normally a cache hit: the measure pass already laid out at this width.
        doc.layout(bounds.size.width.into(), window.scale_factor());
        let lowered = doc.lowered();

        let mut painter = Painter {
            origin: bounds.origin,
            fonts,
            show_gaps: *show_gaps,
            stats: PaintStats::default(),
        };
        let mut index = 0;
        painter.paint(&lowered.ops, &mut index, window, cx);

        let mut painted = painter.stats;
        (painted.fonts_mapped, painted.fonts_unmapped) = fonts.counts();
        *stats = painted;

        let key = (lowered.report.commands, painted);
        if *logged != Some(key) {
            *logged = Some(key);
            eprintln!(
                "colony-html: {} native ops, {:.1}% coverage; fonts {}/{} mapped; glyphs {} painted, {} unmapped, {} failed; {} paths failed",
                lowered.report.native(),
                lowered.report.coverage() * 100.0,
                painted.fonts_mapped,
                painted.fonts_mapped + painted.fonts_unmapped,
                painted.glyphs_painted,
                painted.glyphs_unmapped,
                painted.glyphs_failed,
                painted.paths_failed,
            );
        }
    }
}

struct Painter<'a> {
    /// Window position of the document's top left corner.
    origin: Point<Pixels>,
    fonts: &'a mut FontMap,
    show_gaps: bool,
    stats: PaintStats,
}

impl Painter<'_> {
    /// Paint ops from `index` until the slice ends or the enclosing clip is popped.
    /// GPUI scopes a content mask to a closure, so each clip recurses.
    fn paint(&mut self, ops: &[Op], index: &mut usize, window: &mut Window, cx: &mut App) {
        while let Some(op) = ops.get(*index) {
            *index += 1;
            match op {
                Op::PushClip { rect } => {
                    let mask = ContentMask {
                        bounds: self.bounds(*rect),
                    };
                    window
                        .with_content_mask(Some(mask), |window| self.paint(ops, index, window, cx));
                }
                Op::PopClip => return,
                Op::Quad { rect, color } => {
                    window.paint_quad(fill(self.bounds(*rect), hsla(*color)));
                }
                Op::FillPath {
                    path,
                    color,
                    even_odd,
                } => match self.path(path, *even_odd) {
                    Some(path) => window.paint_path(path, hsla(*color)),
                    None => self.stats.paths_failed += 1,
                },
                Op::Glyphs(run) => self.glyphs(run, window, cx),
                Op::Shadow(shadow) => self.shadow(shadow, window),
                Op::Gap { bounds, .. } => {
                    if let (true, Some(rect)) = (self.show_gaps, bounds) {
                        window.paint_quad(outline(
                            self.bounds(*rect),
                            gpui::rgb(0xff00ff),
                            BorderStyle::Solid,
                        ));
                    }
                }
            }
        }
    }

    fn glyphs(&mut self, run: &GlyphRun, window: &mut Window, cx: &mut App) {
        let Some(font_id) = self.fonts.font_id(&run.font, cx) else {
            self.stats.glyphs_unmapped += run.glyphs.len();
            return;
        };
        let color = hsla(run.color);
        for glyph in &run.glyphs {
            // `paint_glyph` takes the baseline origin, which is what Parley positions are.
            let origin = self.point(glyph.x as f64, glyph.y as f64);
            match window.paint_glyph(origin, font_id, GlyphId(glyph.id), px(run.size), color) {
                Ok(()) => self.stats.glyphs_painted += 1,
                Err(_) => self.stats.glyphs_failed += 1,
            }
        }
    }

    fn shadow(&mut self, shadow: &Shadow, window: &mut Window) {
        let [top_left, top_right, bottom_right, bottom_left] = shadow.radii.map(px);
        let radii = Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        };
        let shadows = [BoxShadow {
            color: hsla(shadow.color),
            offset: point(px(shadow.offset.0), px(shadow.offset.1)),
            blur_radius: px(shadow.blur),
            spread_radius: px(shadow.spread),
            inset: shadow.inset,
        }];
        let bounds = self.bounds(shadow.rect);
        if shadow.inset {
            window.paint_inset_shadows(bounds, radii, &shadows);
        } else {
            window.paint_drop_shadows(bounds, radii, &shadows);
        }
    }

    fn path(&self, path: &BezPath, even_odd: bool) -> Option<gpui::Path> {
        let fill_rule = if even_odd {
            FillRule::EvenOdd
        } else {
            FillRule::NonZero
        };
        let mut builder =
            PathBuilder::fill().with_style(PathStyle::Fill(FillOptions { fill_rule }));
        for element in path.elements() {
            match *element {
                PathEl::MoveTo(to) => builder.move_to(self.point(to.x, to.y)),
                PathEl::LineTo(to) => builder.line_to(self.point(to.x, to.y)),
                PathEl::QuadTo(control, to) => {
                    builder.curve_to(self.point(to.x, to.y), self.point(control.x, control.y))
                }
                PathEl::CurveTo(control_a, control_b, to) => builder.cubic_bezier_to(
                    self.point(to.x, to.y),
                    self.point(control_a.x, control_a.y),
                    self.point(control_b.x, control_b.y),
                ),
                PathEl::ClosePath => builder.close(),
            }
        }
        builder.build().ok()
    }

    /// A document-space point as a window-space point.
    fn point(&self, x: f64, y: f64) -> Point<Pixels> {
        point(self.origin.x + px(x as f32), self.origin.y + px(y as f32))
    }

    /// A document-space rectangle as window-space bounds.
    fn bounds(&self, rect: Rect) -> Bounds<Pixels> {
        Bounds {
            origin: self.point(rect.x0, rect.y0),
            size: size(px(rect.width() as f32), px(rect.height() as f32)),
        }
    }
}

fn hsla(color: Rgba) -> Hsla {
    gpui::Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
    .into()
}
