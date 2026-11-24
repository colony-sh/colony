//! Prints how much of a page's paint output lands on native GPUI primitives.
//!
//!     cargo run -p colony-html-doc --bin gap-report -- [PAGE.html] [WIDTH]
//!
//! With no arguments it reports on the bundled sample page at 900 logical pixels.

use std::process::ExitCode;

use colony_html_doc::HtmlDoc;

const SAMPLE: &str = include_str!("../../../../pages/sample.html");

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let path = args.next();
    let width = match args.next().map(|arg| arg.parse::<f32>()) {
        None => 900.0,
        Some(Ok(width)) if width > 0.0 => width,
        Some(_) => {
            eprintln!("WIDTH must be a positive number of logical pixels");
            return ExitCode::FAILURE;
        }
    };

    let html = match &path {
        None => SAMPLE.to_owned(),
        Some(path) => match std::fs::read_to_string(path) {
            Ok(html) => html,
            Err(error) => {
                eprintln!("cannot read {path}: {error}");
                return ExitCode::FAILURE;
            }
        },
    };

    let mut doc = HtmlDoc::from_html(&html);
    let height = doc.layout(width, 1.0);
    let report = &doc.lowered().report;

    println!(
        "page                {}",
        path.as_deref().unwrap_or("pages/sample.html")
    );
    println!("layout              {width} x {height} logical px");
    println!("{report}");
    ExitCode::SUCCESS
}
