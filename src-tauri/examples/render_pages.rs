//! Debug helper: render pages of report markup to PNG for a visual check.
//! Usage: cargo run --example render_pages -- <report.typ> <out-dir> [first] [count]

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let markup = std::fs::read_to_string(&args[1])?;
    let out = std::path::Path::new(&args[2]);
    let first: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
    let count: usize = args.get(4).and_then(|v| v.parse().ok()).unwrap_or(4);
    std::fs::create_dir_all(out)?;
    let doc = coi_blast_lib::pdf::layout(markup)?;
    println!("{} pages", doc.pages.len());
    for (i, page) in doc.pages.iter().enumerate().skip(first).take(count) {
        let pixmap = typst_render::render(page, 1.6);
        let path = out.join(format!("page-{:02}.png", i + 1));
        std::fs::write(&path, pixmap.encode_png()?)?;
        println!("{}", path.display());
    }
    Ok(())
}
