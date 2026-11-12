//! The app-side glue for the exporters (the browser's `buildExportModel`
//! plus `exportPNG`/`exportSVG`/`exportDXF`/`exportPDF` calls): map the live
//! drawing + area codes + options into the headless core's `ExportPage`,
//! then let `sketch_core::export_*` produce the bytes/string.
//!
//! PNG uses the clean canvas renderer; PDF embeds the complete report sketch
//! and report fonts. SVG/DXF consume the shared export model.

use sketch_core::export_model::{
    ExportLabel, ExportLine, ExportOptions, ExportPage, ExportPolygon, ExportSolid,
};
use sketch_core::model::WallType;
use sketch_core::solids::compute_wall_solids;

/// Export the current drawing styles and auto-post preferences.
pub fn export_options(drawing: &crate::drawing::Drawing) -> ExportOptions {
    let o = &drawing.options;
    ExportOptions {
        exterior_color: o.exterior_color.clone(),
        interior_color: o.interior_color.clone(),
        exterior_width: o.exterior_width,
        interior_width: o.interior_width,
        wall_solids: o.wall_solids_enabled(),
        auto_post_name: drawing.auto_post.name,
        auto_post_calcs: drawing.auto_post.calcs,
        auto_post_suffix: drawing.auto_post.suffix.clone(),
        auto_post_font: o.auto_post_font.clone(),
        auto_post_font_size: o.auto_post_font_size,
        auto_post_bold: o.auto_post_bold,
        auto_post_italic: o.auto_post_italic,
        text_color: o.text_color.clone(),
    }
}

fn wall_type(line: &sketch_core::model::SketchLine, options: &ExportOptions) -> (String, f64) {
    match line.kind {
        WallType::Interior => ("interior".to_owned(), options.interior_width),
        WallType::Exterior => ("exterior".to_owned(), options.exterior_width),
    }
}

/// `buildExportModel()` for the live drawing (the active page). The
/// page's north angle rides in from the caller.
pub fn build_export_page(
    drawing: &crate::drawing::Drawing,
    north_angle: f64,
    area_codes: &[sketch_core::geometry::AreaCode],
) -> ExportPage {
    build_export_page_of(
        drawing,
        &drawing.lines,
        &drawing.polygons,
        &drawing.texts,
        &drawing.symbols,
        north_angle,
        area_codes,
    )
}

/// `buildExportModel().pages` - one entry per sketch page. The current
/// page comes from the live drawing (its edits are not stored until a page
/// switch), the others from their stored arrays; `drawing` supplies the
/// options and wall thicknesses both read.
pub fn build_export_pages(
    drawing: &crate::drawing::Drawing,
    pages: &crate::pages::Pages,
    area_codes: &[sketch_core::geometry::AreaCode],
) -> Vec<ExportPage> {
    pages
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            if index == pages.current {
                build_export_page(drawing, page.north_angle, area_codes)
            } else {
                build_export_page_of(
                    drawing,
                    &page.lines,
                    &page.polygons,
                    &page.labels,
                    &page.symbols,
                    page.north_angle,
                    area_codes,
                )
            }
        })
        .collect()
}

/// `SketchReport.normalizeLayout` for the app's Page Layout.
pub fn report_layout(
    layout: &crate::page_layout::PageLayout,
) -> sketch_core::export_pdf::ReportLayout {
    let c = &layout.captions;
    sketch_core::export_pdf::ReportLayout {
        paper: layout.paper_size,
        show_sketch_comments: layout.show_sketch_comments,
        show_comment_tables: layout.show_comment_tables,
        show_page_numbers: layout.show_page_numbers,
        captions: sketch_core::export_pdf::ReportCaptions {
            sketch_page_title: c.sketch_page_title.clone(),
            subject_label: c.subject_label.clone(),
            sketch_label: c.sketch_label.clone(),
            summary_label: c.summary_label.clone(),
            comment_table_labels: [
                c.comment_table1_label.clone(),
                c.comment_table2_label.clone(),
                c.comment_table3_label.clone(),
            ],
        },
    }
    .normalized()
}

fn build_export_page_of(
    drawing: &crate::drawing::Drawing,
    source_lines: &[sketch_core::model::SketchLine],
    source_polygons: &[sketch_core::model::Polygon],
    source_texts: &[crate::placed_text::PlacedText],
    source_symbols: &[crate::placed_symbol::PlacedSymbol],
    north_angle: f64,
    area_codes: &[sketch_core::geometry::AreaCode],
) -> ExportPage {
    let options = export_options(drawing);
    let mut lines = Vec::new();
    for line in source_lines {
        let (wall_type, default_width) = wall_type(line, &options);
        lines.push(ExportLine {
            id: line.id,
            start: line.start,
            end: line.end,
            wall_type,
            color: line.color.clone(),
            line_thickness: line.line_thickness.unwrap_or(default_width),
            line_style: line.line_style.clone(),
            bow_angle: line.bow_angle,
            openings: sketch_core::solids::visible_runs(
                line,
                &crate::placed_symbol::openings(source_symbols),
            ),
        });
    }

    // `buildExportModel` carries each polygon's recalculated values (net
    // after subtractions, factor, curve-aware gross and perimeter), so the
    // calculation rows and the Net LIVABLE total read the live numbers.
    let recalculated: std::collections::HashMap<i64, _> = drawing
        .recalculate_areas(source_polygons, source_lines, area_codes)
        .polygons
        .into_iter()
        .map(|c| (c.id, c))
        .collect();
    let mut polygons = Vec::new();
    for poly in source_polygons {
        if poly.points.len() < 3 {
            continue;
        }
        polygons.push(ExportPolygon {
            points: poly.points.clone(),
            edges: poly.edges.clone(),
            area_type: poly.kind.clone(),
            name: poly.name.clone(),
            net: recalculated.get(&poly.id).map(|c| c.net),
            area: recalculated.get(&poly.id).map_or(poly.area, |c| c.area),
            gross: recalculated.get(&poly.id).map_or(poly.gross, |c| c.gross),
            effective_multiplier: recalculated
                .get(&poly.id)
                .map(|c| c.factor)
                .or(poly.factor_override),
            perimeter: recalculated.get(&poly.id).map_or_else(
                || {
                    poly.points
                        .iter()
                        .zip(
                            poly.points
                                .iter()
                                .skip(1)
                                .chain(std::iter::once(&poly.points[0])),
                        )
                        .map(|(a, b)| (b.x - a.x).hypot(b.y - a.y))
                        .sum()
                },
                |c| c.perimeter,
            ),
            line_color: poly.line_color.clone(),
            fill_color: poly.fill_color.clone(),
            fill_opacity: poly.fill_opacity,
            fill_pattern: poly.fill_pattern.clone(),
            line_thickness: poly.line_thickness,
            label_position: poly.label_position,
            label_rotation: poly.label_rotation.unwrap_or(0.0),
            label_font_size: poly.label_font_size,
            label_font_family: poly.label_font_family.clone(),
            label_color: poly.label_color.clone(),
            label_bold: poly.label_bold,
            label_italic: poly.label_italic,
        });
    }

    let labels: Vec<ExportLabel> = source_texts
        .iter()
        .map(|t| ExportLabel {
            x: t.x,
            y: t.y,
            text: t.text.clone(),
            font_size: Some(t.font_size),
            rotation: t.rotation,
            // An automatic label (no colour, or the old `#e2e8f0`) prints in
            // the report's dark ink.
            color: if t.color.is_empty() || t.color.eq_ignore_ascii_case("#e2e8f0") {
                None
            } else {
                Some(t.color.clone())
            },
            font_family: Some(t.font_family.clone()),
            bold: t.bold,
            italic: t.italic,
        })
        .collect();

    let symbols: Vec<sketch_core::export_model::ExportSymbol> = source_symbols
        .iter()
        .map(|s| sketch_core::export_model::ExportSymbol {
            x: s.x,
            y: s.y,
            symbol_type: s.symbol_type.clone(),
            rotation: s.rotation,
            scale: s.scale,
            flip_h: s.flip_h,
            color: None,
        })
        .collect();

    let wall_solids: Vec<ExportSolid> = if options.wall_solids {
        let solids = compute_wall_solids(
            source_lines,
            &|line| drawing.wall_thickness_of(line),
            &crate::placed_symbol::openings(source_symbols),
        );
        solids
            .solids
            .iter()
            .map(|solid| ExportSolid {
                line_id: solid.line_id,
                pieces: solid.pieces.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    ExportPage {
        lines,
        polygons,
        labels,
        symbols,
        wall_solids,
        north_angle,
    }
}

/// The PDF/Print subject block (`state.subjectInfo` → `ReportSubject`).
pub fn report_subject(
    info: &crate::subject_info::SubjectInfo,
) -> sketch_core::export_pdf::ReportSubject {
    sketch_core::export_pdf::ReportSubject {
        file_no: info.file_no.clone(),
        parcel_no: info.parcel_no.clone(),
        property_address: info.property_address.clone(),
        city: info.city.clone(),
        county: info.county.clone(),
        state: info.state.clone(),
        zip_code: info.zip_code.clone(),
        owner: info.owner.clone(),
        client: info.client.clone(),
        client_address: info.client_address.clone(),
        appraiser_name: info.appraiser_name.clone(),
        inspection_date: info.inspection_date.clone(),
        general_comments: info.general_comments.clone(),
        comment_tables: info.comment_tables.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::geometry::Pt;

    #[test]
    fn a_rich_page_maps_into_the_export_model() {
        let mut drawing = crate::drawing::Drawing::default();
        drawing.lines.push(sketch_core::model::SketchLine {
            id: 3,
            start: Pt::new(0.0, 0.0),
            end: Pt::new(10.0, 0.0),
            kind: WallType::Exterior,
            color: None,
            line_thickness: None,
            thickness: None,
            dimension_hidden: false,
            layer_id: None,
            bow_angle: 0.0,
            arc_height: 0.0,
            line_style: None,
            dimension_position: None,
            dimension_anchor: None,
            extra: Default::default(),
        });
        drawing.polygons.push(sketch_core::model::Polygon {
            id: 4,
            points: vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 8.0)],
            edges: vec![],
            kind: "LIV".into(),
            name: "Living".into(),
            gross: 40.0,
            area: 40.0,
            factor_override: None,
            subtract_mode: "auto".into(),
            subtract_target_id: None,
            factor: 1.0,
            measure_to: "center".into(),
            auto_post: Default::default(),
            attributes: Default::default(),
            layer_id: None,
            fill_color: None,
            fill_pattern: None,
            fill_opacity: None,
            line_color: None,
            line_style: None,
            line_thickness: None,
            label_rotation: None,
            area_size_rotation: None,
            label_position: None,
            area_size_position: None,
            label_scale: None,
            area_size_scale: None,
            label_font_size: None,
            label_font_family: None,
            label_bold: None,
            label_italic: None,
            label_underline: None,
            label_color: None,
            extra: Default::default(),
        });
        drawing
            .texts
            .push(crate::placed_text::PlacedText::new(5, "kitchen"));
        let page = build_export_page(&drawing, 0.0, &[]);
        assert_eq!(page.lines.len(), 1);
        assert_eq!(page.lines[0].wall_type, "exterior");
        assert_eq!(page.lines[0].line_thickness, 4.0);
        assert_eq!(page.polygons[0].area_type, "LIV");
        assert_eq!(page.labels[0].text, "kitchen");
        // Solids stay off while the option is off.
        assert!(page.wall_solids.is_empty());
        let mut options = export_options(&drawing);
        options.wall_solids = true;
        let _ = options;
    }

    #[test]
    fn polygons_carry_their_recalculated_values() {
        // The report's calculation rows and Net LIVABLE read the live net,
        // factor, gross and perimeter, not the creation-time numbers.
        let mut drawing = crate::drawing::Drawing::default();
        drawing.polygons.push(sketch_core::model::Polygon {
            id: 1,
            points: vec![
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(10.0, 10.0),
                Pt::new(0.0, 10.0),
            ],
            edges: vec![],
            kind: "GLA1".into(),
            name: "First Floor".into(),
            gross: 0.0,
            area: 0.0,
            factor_override: None,
            subtract_mode: "auto".into(),
            subtract_target_id: None,
            factor: 1.0,
            measure_to: "center".into(),
            auto_post: Default::default(),
            attributes: Default::default(),
            layer_id: None,
            fill_color: None,
            fill_pattern: None,
            fill_opacity: None,
            line_color: None,
            line_style: None,
            line_thickness: None,
            label_rotation: None,
            area_size_rotation: None,
            label_position: None,
            area_size_position: None,
            label_scale: None,
            area_size_scale: None,
            label_font_size: None,
            label_font_family: None,
            label_bold: None,
            label_italic: None,
            label_underline: None,
            label_color: None,
            extra: Default::default(),
        });
        let codes = sketch_core::geometry::area_codes::default_area_codes();
        let page = build_export_page(&drawing, 0.0, &codes);
        let poly = &page.polygons[0];
        assert_eq!(poly.net, Some(100.0));
        assert_eq!(poly.gross, 100.0);
        assert_eq!(poly.effective_multiplier, Some(1.0));
        assert!((poly.perimeter - 40.0).abs() < 1e-9);
    }
    #[test]
    fn hosted_openings_reach_vector_exports_with_live_styles() {
        let mut d = crate::drawing::Drawing::default();
        d.lines.push(serde_json::from_value(serde_json::json!({"id":1,"type":"exterior","start":{"x":0,"y":0},"end":{"x":10,"y":0}})).unwrap());
        d.symbols.push(serde_json::from_value(serde_json::json!({"id":2,"symbolType":"windows/36in","x":5,"y":0,"wallId":1,"wallGapWidth":3})).unwrap());
        d.options.exterior_color = "#ff0000".into();
        d.options.exterior_width = 7.0;
        d.auto_post.name = false;
        let options = export_options(&d);
        assert!(!options.auto_post_name);
        let page = build_export_page(&d, 0.0, &[]);
        assert_eq!(page.lines[0].openings, vec![(0.0, 0.35), (0.65, 1.0)]);
        let svg = sketch_core::export_svg::export_svg(&page, &options, &[], None);
        assert!(svg.contains("#ff0000"));
        assert!(svg.contains("stroke-width=\"7\""));
        d.options.wall_solids = true;
        let solid = build_export_page(&d, 0.0, &[]);
        assert_eq!(solid.wall_solids[0].pieces.len(), 2);
    }
}
