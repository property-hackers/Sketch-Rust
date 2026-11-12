//! sketch-core — headless kernel for the standalone Sketch app.
//!
//! Clean-room port of `00) sketch standalone.html` (Appraisal Sketch 1.1).
//! The browser sources extracted into `refs/` are the porting oracle:
//! every number, sign convention, and tolerance is transcribed, never invented.

pub mod area_calc;
pub mod breakdown;
pub mod cad_import;
pub mod disto;
pub mod export_dxf;
pub mod export_model;
pub mod export_pdf;
pub mod export_png;
pub mod export_svg;
pub mod geometry;
pub mod joins;
pub mod model;
pub mod print_layout;
mod report_fonts;
pub mod sketchy;
pub mod skx;
pub mod solids;
pub mod symbol_edit;
pub mod symbol_import;
pub mod symbols;
