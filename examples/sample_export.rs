//! Renders a sample annotated export to a PNG, for eyeballing the renderer:
//! `cargo run --example sample_export -- out.png [input.png]`

use escribo::doc::*;
use escribo::geom::*;
use escribo::render;
use escribo::settings::Settings;
use escribo::text::TextEngine;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "sample.png".into());
    let shot = match args.get(2) {
        Some(p) => {
            let img = image::open(p)?.to_rgba8();
            Shot::new(img.width(), img.height(), img.into_raw())
        }
        None => {
            let (w, h) = (1200u32, 700u32);
            let mut px = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    let stripe = ((x / 40 + y / 40) % 2) as u8 * 30;
                    px.extend_from_slice(&[230 - stripe, 235 - stripe, 240, 255]);
                }
            }
            Shot::new(w, h, px)
        }
    };
    let u = shot.unit();
    let mut doc = Document::new(shot, "sample.png".into());
    doc.add(Shape::Mark(BoxShape {
        frame: OrientedBox { center: v2(300.0, 150.0), w: 360.0, h: 70.0, angle: 0.0 },
        color: rgb("#FFB300"),
        stroke: 0.0,
        radius: 2.0 * u,
        alpha: 0.33,
    }));
    doc.add(Shape::Rect(BoxShape {
        frame: OrientedBox { center: v2(800.0, 300.0), w: 300.0, h: 180.0, angle: 0.2 },
        color: BRAND,
        stroke: 5.0 * u,
        radius: 2.0 * u,
        alpha: 1.0,
    }));
    doc.add(Shape::Arrow(Arrow { tail: v2(200.0, 500.0), head: v2(620.0, 330.0), unit: u, color: BRAND }));
    doc.add(Shape::Redact(Rect::new(100.0, 250.0, 260.0, 60.0)));
    doc.add(Shape::Text(Label {
        origin: v2(90.0, 560.0),
        angle: -0.05,
        scale: 1.0,
        width: 250.0 * u,
        font_size: 22.0 * u,
        unit: u,
        color: BRAND,
        font: "system".into(),
        text: "Look here 🔥 this label wraps nicely".into(),
    }));
    doc.add(Shape::Emoji(Emoji {
        center: v2(1050.0, 120.0),
        angle: 0.3,
        scale: 1.0,
        size: 100.0 * u,
        ch: "🚀".into(),
    }));
    let mut s = Settings::from_value(&serde_json::Value::Null);
    s.style.pad = 6;
    s.style.backdrop = 6;
    let t0 = std::time::Instant::now();
    let (w, h, rgba) = render::compose(&doc, &s, &mut TextEngine::new()).expect("compose");
    let png = escribo::raster::encode_png(w, h, &rgba)?;
    eprintln!("{w}x{h} in {:?}", t0.elapsed());
    std::fs::write(out, png)?;
    Ok(())
}
