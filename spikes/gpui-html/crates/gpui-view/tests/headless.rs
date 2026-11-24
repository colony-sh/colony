//! Runs the element through GPUI's headless test window: layout, then paint.
//!
//! This cannot say whether the glyphs look right. That needs a real window and
//! eyes. It does prove the element measures, paints every op without panicking,
//! and accounts for every glyph and font the lowering produced.

use colony_html_gpui::HtmlView;
use gpui::{px, size, TestAppContext};

const SAMPLE: &str = include_str!("../../../pages/sample.html");

#[gpui::test]
fn sample_page_lays_out_and_paints(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| HtmlView::new(SAMPLE).show_gaps(true));
    cx.simulate_resize(size(px(900.0), px(700.0)));
    cx.run_until_parked();

    let (stats, report) = view.update(cx, |view, _| (view.stats(), view.report()));

    assert!(report.native() > 0, "the page should lower to native ops");
    assert_eq!(stats.paths_failed, 0, "GPUI rejected a path");
    assert_eq!(
        stats.glyphs_painted + stats.glyphs_unmapped + stats.glyphs_failed,
        report.glyphs,
        "every glyph is painted, unmapped or failed: {stats:?}"
    );
    assert_eq!(
        stats.fonts_mapped + stats.fonts_unmapped,
        report.fonts,
        "every font is mapped or unmapped: {stats:?}"
    );
}

#[gpui::test]
fn resizing_relays_out_at_the_new_width(cx: &mut TestAppContext) {
    let page = format!(
        "<!doctype html><html><body style='margin:0'><p>{}</p></body></html>",
        "word ".repeat(300)
    );
    let (view, cx) = cx.add_window_view(|_, _| HtmlView::new(&page));

    cx.simulate_resize(size(px(1200.0), px(700.0)));
    cx.run_until_parked();
    let wide = view.update(cx, |view, _| view.document_height());

    cx.simulate_resize(size(px(320.0), px(700.0)));
    cx.run_until_parked();
    let narrow = view.update(cx, |view, _| view.document_height());

    assert!(wide > 0.0);
    assert!(
        narrow > wide,
        "narrow {narrow} should be taller than wide {wide}"
    );
}
