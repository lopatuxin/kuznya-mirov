use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Stretch, SwashCache};

// Kuznya Mirov: `Attrs::stretch` sets the `wdth` axis of a variable font, the same way
// `Attrs::weight` sets `wght` — for the advances shaping lays the line out with and for the
// outlines swash draws. Tektur has `wdth` 75–100; Inter-Regular has no axes at all.

const TEXT: &str = "1 января 1930";

fn font_system(path: &str) -> FontSystem {
    let mut font_system =
        FontSystem::new_with_locale_and_db("ru-RU".into(), fontdb::Database::new());
    let font = std::fs::read(path).unwrap();
    font_system.db_mut().load_font_data(font);
    font_system
}

fn shaped(font_system: &mut FontSystem, family: &str, stretch: Stretch) -> Buffer {
    let mut buffer = Buffer::new(font_system, Metrics::new(20.0, 24.0));
    buffer.set_text(
        font_system,
        TEXT,
        &Attrs::new().family(Family::Name(family)).stretch(stretch),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);
    buffer
}

fn line_width(buffer: &Buffer) -> f32 {
    buffer
        .layout_runs()
        .fold(0.0f32, |width, run| width.max(run.line_w))
}

/// Width in pixels of the first glyph's raster.
fn first_glyph_raster_width(
    font_system: &mut FontSystem,
    cache: &mut SwashCache,
    buffer: &Buffer,
) -> u32 {
    let run = buffer.layout_runs().next().unwrap();
    let glyph = run.glyphs.first().unwrap().physical((0.0, 0.0), 1.0);
    cache
        .get_image(font_system, glyph.cache_key)
        .as_ref()
        .unwrap()
        .placement
        .width
}

#[test]
fn a_line_at_87_5_is_narrower_than_at_100_for_a_font_with_a_wdth_axis() {
    let mut font_system = font_system("fonts/Tektur.ttf");
    let normal = shaped(&mut font_system, "Tektur", Stretch::Normal);
    let narrow = shaped(&mut font_system, "Tektur", Stretch::SemiCondensed);
    let (normal_w, narrow_w) = (line_width(&normal), line_width(&narrow));
    assert!(narrow_w < normal_w, "87,5 — {narrow_w}, 100 — {normal_w}");

    let mut cache = SwashCache::new();
    let normal_raster = first_glyph_raster_width(&mut font_system, &mut cache, &normal);
    let narrow_raster = first_glyph_raster_width(&mut font_system, &mut cache, &narrow);
    assert!(
        narrow_raster < normal_raster,
        "рисунок буквы при 87,5 — {narrow_raster}, при 100 — {normal_raster}"
    );
}

#[test]
fn stretch_beyond_the_axis_is_clamped_to_its_end() {
    let mut font_system = font_system("fonts/Tektur.ttf");
    let condensed = line_width(&shaped(&mut font_system, "Tektur", Stretch::Condensed));
    let ultra = line_width(&shaped(&mut font_system, "Tektur", Stretch::UltraCondensed));
    assert_eq!(condensed, ultra, "у Tektur ось от 75: 50 прижимается к 75");
}

#[test]
fn stretch_changes_nothing_for_a_font_without_a_wdth_axis() {
    let mut font_system = font_system("fonts/Inter-Regular.ttf");
    let normal = line_width(&shaped(&mut font_system, "Inter", Stretch::Normal));
    let narrow = line_width(&shaped(&mut font_system, "Inter", Stretch::SemiCondensed));
    assert_eq!(normal, narrow);
}

#[test]
fn weight_and_stretch_change_nothing_for_a_font_without_axes_and_keep_its_face() {
    let mut font_system = font_system("fonts/Inter-Regular.ttf");
    let shaped_with = |font_system: &mut FontSystem, weight: u16, stretch: Stretch| {
        let mut buffer = Buffer::new(font_system, Metrics::new(20.0, 24.0));
        buffer.set_text(
            font_system,
            TEXT,
            &Attrs::new()
                .family(Family::Name("Inter"))
                .weight(cosmic_text::Weight(weight))
                .stretch(stretch),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(font_system, false);
        buffer
    };
    let plain = shaped_with(&mut font_system, 400, Stretch::Normal);
    let bold_narrow = shaped_with(&mut font_system, 700, Stretch::SemiCondensed);
    assert_eq!(line_width(&plain), line_width(&bold_narrow));
    let face = |buffer: &Buffer| buffer.layout_runs().next().unwrap().glyphs[0].font_id;
    assert_eq!(face(&plain), face(&bold_narrow));
}
