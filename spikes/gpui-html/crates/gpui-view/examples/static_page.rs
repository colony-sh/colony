//! Opens a window that paints one static HTML page with GPUI primitives.
//!
//!     cargo run -p colony-html-gpui --example static_page -- [PAGE.html]
//!
//! With no argument it shows the bundled sample page. Set `COLONY_HTML_GAPS=1`
//! to outline, in magenta, every region that did not paint natively. A summary
//! of what painted is written to stderr whenever it changes.

use colony_html_gpui::HtmlView;
use gpui::{prelude::*, px, size, App, Bounds, WindowBounds, WindowOptions};
use gpui_platform::application;

const SAMPLE: &str = include_str!("../../../pages/sample.html");

fn main() {
    let html = match std::env::args().nth(1) {
        None => SAMPLE.to_owned(),
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(html) => html,
            Err(error) => {
                eprintln!("cannot read {path}: {error}");
                std::process::exit(1);
            }
        },
    };
    let show_gaps = std::env::var_os("COLONY_HTML_GAPS").is_some_and(|value| value != "0");

    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| cx.new(|_| HtmlView::new(&html).show_gaps(show_gaps)),
        )
        .expect("the window should open");
        cx.activate(true);
    });
}
