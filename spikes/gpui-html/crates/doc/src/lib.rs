//! An HTML string, laid out by Blitz, lowered for GPUI.
//!
//! [`HtmlDoc`] owns a Blitz document. Give it a width, and it resolves style
//! (Stylo) and layout (Taffy and Parley), records Blitz's paint commands into an
//! `anyrender::Scene`, and lowers that scene with [`colony_html_paint::lower`].
//!
//! Nothing here depends on GPUI.

use anyrender::Scene;
use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use colony_html_paint::{lower_scaled, Lowered};

/// What the current layout and paint were computed for.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    /// Width in physical pixels. Layout depends on nothing finer than this.
    physical_width: u32,
    scale: f32,
}

/// A static HTML document.
pub struct HtmlDoc {
    doc: HtmlDocument,
    key: Option<Key>,
    height: f32,
    lowered: Option<Lowered>,
}

impl HtmlDoc {
    /// Parse an HTML string. Subresources are not fetched: there is no network provider,
    /// so external stylesheets, images and fonts are skipped.
    pub fn from_html(html: &str) -> Self {
        Self {
            doc: HtmlDocument::from_html(html, DocumentConfig::default()),
            key: None,
            height: 0.0,
            lowered: None,
        }
    }

    /// Lay the document out `width` logical pixels wide and return its height in logical
    /// pixels. `scale` is the display's scale factor. Blitz shapes text in device pixels,
    /// so it has to know the real scale; [`lowered`](Self::lowered) converts back.
    ///
    /// Repeated calls with the same width and scale return the cached height.
    pub fn layout(&mut self, width: f32, scale: f32) -> f32 {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let key = Key {
            physical_width: (width.max(1.0) * scale).round() as u32,
            scale,
        };
        if self.key == Some(key) {
            return self.height;
        }

        // The viewport's height only matters to viewport units and to how tall an
        // otherwise short page's background is. A fixed, ordinary height keeps layout a
        // function of width alone, which is what a scrolling host wants.
        const VIEWPORT_HEIGHT: f32 = 800.0;
        self.doc.set_viewport(Viewport::new(
            key.physical_width,
            (VIEWPORT_HEIGHT * scale).round() as u32,
            scale,
            ColorScheme::Light,
        ));
        self.doc.resolve(0.0);

        let root = self.doc.root_element();
        self.height = root.final_layout().size.height.max(root.scroll_height());
        self.key = Some(key);
        self.lowered = None;
        self.height
    }

    /// Height in logical pixels from the last [`layout`](Self::layout) call.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Record Blitz's paint commands for the whole document, in device pixels.
    /// Returns an empty scene before the first [`layout`](Self::layout) call.
    ///
    /// The scene is recorded at the display's scale factor, not at `1.0`, because Blitz
    /// lays text out in device pixels and scales only boxes at paint time. Recording at
    /// any other scale leaves text and boxes at different sizes.
    pub fn record(&mut self) -> Scene {
        let mut scene = Scene::new();
        let Some(key) = self.key else {
            return scene;
        };
        let height = (self.height * key.scale).ceil() as u32;
        blitz_paint::paint_scene(
            &mut scene,
            &mut self.doc,
            key.scale as f64,
            key.physical_width,
            height,
            0,
            0,
        );
        scene
    }

    /// The document lowered into GPUI-shaped paint ops in logical pixels, cached until
    /// the next layout that changes the width or scale.
    pub fn lowered(&mut self) -> &Lowered {
        if self.lowered.is_none() {
            let scale = self.key.map_or(1.0, |key| key.scale);
            let scene = self.record();
            self.lowered = Some(lower_scaled(&scene, 1.0 / scale as f64));
        }
        self.lowered.as_ref().expect("just set")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_html_paint::Op;

    const PAGE: &str = r#"<!doctype html>
        <html><head><style>
          body { margin: 0; font-family: sans-serif; }
          .box { width: 200px; height: 100px; background: #c00; }
          p { margin: 16px; }
        </style></head>
        <body><div class="box"></div><p>Hello from Blitz.</p></body></html>"#;

    #[test]
    fn layout_reports_a_height_and_caches_it() {
        let mut doc = HtmlDoc::from_html(PAGE);
        let first = doc.layout(400.0, 2.0);
        assert!(
            first >= 100.0,
            "the red box alone is 100px tall, got {first}"
        );
        assert_eq!(doc.layout(400.0, 2.0), first);
        assert_eq!(doc.height(), first);
        // Height is logical: the same page is the same height on a 1x display.
        let at_1x = HtmlDoc::from_html(PAGE).layout(400.0, 1.0);
        assert!((first - at_1x).abs() < 1.0, "2x {first} vs 1x {at_1x}");
    }

    #[test]
    fn the_red_box_lowers_to_a_quad_in_logical_pixels() {
        let mut doc = HtmlDoc::from_html(PAGE);
        // Scale 2.0: the quad must still be 200 by 100, not 400 by 200.
        doc.layout(400.0, 2.0);
        let lowered = doc.lowered();
        let red = lowered.ops.iter().find_map(|op| match op {
            Op::Quad { rect, color } if color.r > 0.7 && color.g < 0.1 => Some(*rect),
            _ => None,
        });
        let rect = red.expect("a red quad");
        assert_eq!((rect.width(), rect.height()), (200.0, 100.0));
        assert_eq!((rect.x0, rect.y0), (0.0, 0.0));
    }

    #[test]
    fn text_stays_in_logical_pixels_at_any_scale() {
        fn first_run(scale: f32) -> (f32, f32, f32) {
            let mut doc = HtmlDoc::from_html(PAGE);
            doc.layout(400.0, scale);
            let run = doc
                .lowered()
                .ops
                .iter()
                .find_map(|op| match op {
                    Op::Glyphs(run) => Some(run.clone()),
                    _ => None,
                })
                .expect("the paragraph should produce a glyph run");
            (run.size, run.glyphs[0].x, run.glyphs[0].y)
        }
        let (size_1x, x_1x, y_1x) = first_run(1.0);
        let (size_2x, x_2x, y_2x) = first_run(2.0);
        assert_eq!(size_1x, 16.0, "the default font size is 16 CSS pixels");
        assert_eq!(size_2x, 16.0);
        assert!((x_1x - x_2x).abs() < 0.51, "x differs: {x_1x} vs {x_2x}");
        assert!((y_1x - y_2x).abs() < 0.51, "y differs: {y_1x} vs {y_2x}");
    }

    #[test]
    fn narrower_layout_wraps_text_and_grows() {
        let long = format!(
            "<!doctype html><html><body><p>{}</p></body></html>",
            "word ".repeat(200)
        );
        let mut doc = HtmlDoc::from_html(&long);
        let wide = doc.layout(1200.0, 1.0);
        let narrow = doc.layout(300.0, 1.0);
        assert!(
            narrow > wide,
            "narrow {narrow} should be taller than wide {wide}"
        );
    }
}
