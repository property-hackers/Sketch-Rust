//! Native, in-process Apex desktop protocol compatibility.
/// The embedded Apex component. Empty when the build had no Apex SDK.
#[cfg(all(windows, apex_component))]
const COMPONENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/apex-compat.dll"));
#[cfg(all(windows, not(apex_component)))]
const COMPONENT: &[u8] = &[];
#[cfg(windows)]
const NOT_INCLUDED: &str = "This build does not include Apex compatibility.";

#[cfg(windows)]
mod clr {
    use std::path::{Path, PathBuf};
    use windows::{
        core::{w, PCWSTR},
        Win32::System::ClrHosting::*,
    };
    pub struct Runtime {
        pub host: ICLRRuntimeHost,
        pub component: PathBuf,
    }
    impl Runtime {
        pub fn load(root: &Path) -> Result<Self, String> {
            if super::COMPONENT.is_empty() {
                return Err(super::NOT_INCLUDED.into());
            }
            std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
            let component = root.join("apex-compat.dll");
            std::fs::write(&component, super::COMPONENT).map_err(|e| e.to_string())?;
            unsafe {
                let meta: ICLRMetaHost =
                    CLRCreateInstance(&CLSID_CLRMetaHost).map_err(|e| e.to_string())?;
                let info: ICLRRuntimeInfo = meta
                    .GetRuntime(w!("v4.0.30319"))
                    .map_err(|e| e.to_string())?;
                let host: ICLRRuntimeHost = info
                    .GetInterface(&CLRRuntimeHost)
                    .map_err(|e| e.to_string())?;
                host.Start().map_err(|e| e.to_string())?;
                Ok(Self { host, component })
            }
        }
        pub fn call(&self, method: &str, arg: &str) -> Result<u32, String> {
            let wide = |s: &str| {
                s.encode_utf16()
                    .chain(std::iter::once(0))
                    .collect::<Vec<_>>()
            };
            let path = wide(&self.component.to_string_lossy());
            let method = wide(method);
            let arg = wide(arg);
            unsafe {
                self.host
                    .ExecuteInDefaultAppDomain(
                        PCWSTR(path.as_ptr()),
                        w!("SketchRust.Compat.Boot"),
                        PCWSTR(method.as_ptr()),
                        PCWSTR(arg.as_ptr()),
                    )
                    .map_err(|e| e.to_string())
            }
        }
    }
}
#[cfg(windows)]
pub fn probe() -> Result<u32, String> {
    let root = std::env::temp_dir().join(format!("SketchRust-apex-probe-{}", std::process::id()));
    let bytes = COMPONENT;
    if bytes.is_empty() {
        return Err(NOT_INCLUDED.into());
    }
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    if u16::from_le_bytes(bytes[pe + 92..pe + 94].try_into().unwrap()) != 2 {
        return Err("The embedded host component could create a console".into());
    }
    let runtime = clr::Runtime::load(&root)?;
    let embedded = runtime.call("ProbeEmbedded", "")?;
    if embedded != std::process::id() {
        return Err("Embedded transport check left the sketch process".into());
    }
    let pid = runtime.call("Probe", "")?;
    if pid != std::process::id() {
        return Err("Compatibility component is not in the sketch process".into());
    }
    Ok(pid)
}

#[cfg(windows)]
pub(crate) fn load_host_codes(address: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("SketchRust-Codes-{}-{stamp}", std::process::id()));
    let runtime = clr::Runtime::load(&root)?;
    let output = root.join("codes.json");
    let config = root.join("codes-input.json");
    std::fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({"url":address,"output":output}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    runtime.call("ImportCodes", &config.to_string_lossy())?;
    let text = std::fs::read_to_string(&output).map_err(|e| e.to_string())?;
    sketch_core::geometry::area_codes::from_json(&text).map_err(|e| e.to_string())?;
    Ok(output)
}

#[cfg(windows)]
pub struct Session {
    pub root: std::path::PathBuf,
    _runtime: clr::Runtime,
}
#[cfg(windows)]
impl Session {
    pub fn start_legacy(mode: &str, path: &Path) -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("SketchRust-Legacy-{}-{stamp}", std::process::id()));
        let runtime = clr::Runtime::load(&root)?;
        let boot = root.join("boot.json");
        std::fs::write(
            &boot,
            serde_json::to_vec(
                &serde_json::json!({"session":root,"mode":mode,"path":path,"gui":"true"}),
            )
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if runtime.call("Start", &boot.to_string_lossy())? != 0 {
            return Err("A native host session is already active".into());
        }
        let session = Self {
            root,
            _runtime: runtime,
        };
        session.wait_for("context.json", std::time::Duration::from_secs(30))?;
        let checked = read_context(&session.root)
            .and_then(|c| c.document.as_deref().map(read_document).transpose());
        if let Err(error) = checked {
            session.cancel();
            return Err(error);
        }
        std::env::set_var("SKETCH_APEX_SESSION", &session.root);
        Ok(session)
    }
    #[cfg(debug_assertions)]
    pub fn start_in(config: &std::path::Path, root: std::path::PathBuf) -> Result<Self, String> {
        Self::start_mode(config, root, false)
    }
    pub(crate) fn start_mode(
        config: &std::path::Path,
        root: std::path::PathBuf,
        gui: bool,
    ) -> Result<Self, String> {
        let text = std::fs::read_to_string(config).map_err(|e| e.to_string())?;
        if text.len() > 65536 {
            return Err("Invalid integration configuration".into());
        }
        let guid = text
            .split("<server>")
            .nth(1)
            .and_then(|s| s.split("</server>").next())
            .ok_or("Missing integration session ID")?
            .trim();
        if guid.len() != 36
            || !guid.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
        {
            return Err("Invalid integration session ID".into());
        }
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let normalized = root.join("integration.xml");
        std::fs::write(&normalized,format!(r#"<product name="ApexSketch" xmlns="urn:xmlns:apex:integration:configuration"><server>{guid}</server></product>"#)).map_err(|e|e.to_string())?;
        let runtime = clr::Runtime::load(&root)?;
        let boot = root.join("boot.json");
        std::fs::write(
            &boot,
            serde_json::to_vec(&serde_json::json!({"session":root,"config":normalized,"gui":if gui {"true"} else {"false"}}))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if runtime.call("Start", &boot.to_string_lossy())? != 0 {
            return Err("Integration session is already active".into());
        }
        let session = Self {
            root,
            _runtime: runtime,
        };
        session.wait_for("server-ready", std::time::Duration::from_secs(10))?;
        Ok(session)
    }
    pub fn wait_for(
        &self,
        name: &str,
        timeout: std::time::Duration,
    ) -> Result<std::path::PathBuf, String> {
        let started = std::time::Instant::now();
        loop {
            let error = self.root.join("error.txt");
            if error.exists() {
                return Err(std::fs::read_to_string(error).unwrap_or_else(|e| e.to_string()));
            }
            let path = self.root.join(name);
            if path.exists() {
                return Ok(path);
            }
            if started.elapsed() > timeout {
                return Err(format!(
                    "Timed out waiting for the host application ({name})"
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    pub fn cancel(&self) {
        let _ = std::fs::write(self.root.join("cancel"), "");
        let started = std::time::Instant::now();
        while !self.root.join("stopped").exists()
            && started.elapsed() < std::time::Duration::from_secs(2)
        {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Context {
    version: u32,
    #[serde(default)]
    file_mode: bool,
    host_path: PathBuf,
    document: Option<PathBuf>,
    subject: BTreeMap<String, String>,
    process_id: u32,
    #[serde(default)]
    page_index: usize,
}
fn read_context(root: &Path) -> Result<Context, String> {
    let context: Context = serde_json::from_slice(
        &std::fs::read(root.join("context.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if context.version != 1 || context.process_id != std::process::id() {
        return Err("Invalid report-linked session".into());
    }
    Ok(context)
}
fn read_document(path: &Path) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let model = if path.extension().is_some_and(|e| e == "json") {
        serde_json::from_slice(&bytes)
            .map_err(|e| format!("Cannot read native editor model: {e}"))?
    } else {
        sketch_core::skx::read(&bytes)
            .map_err(|e| format!("Cannot open the host's Sketch Rust data: {e}"))?
            .model
    };
    if model["pages"]
        .as_array()
        .is_none_or(|pages| pages.is_empty())
    {
        return Err("The report-linked sketch has no valid pages".into());
    }
    let mut checked = model.clone();
    let mut layers = crate::layers::Layers::default();
    layers.ensure(checked.get("layers"), checked.get("activeLayerId"));
    crate::App::normalize_entity_layers(&layers, &mut checked);
    for (index, page) in checked["pages"]
        .as_array()
        .ok_or("Missing native pages")?
        .iter()
        .enumerate()
    {
        serde_json::from_value::<crate::pages::Page>(page.clone())
            .map_err(|e| format!("Cannot load native page {}: {e}", index + 1))?;
    }
    Ok(model)
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaData {
    id: i32,
    code: String,
    name: String,
    base_area: f64,
    net_area: f64,
    factor: f64,
    perimeter: f64,
    description: String,
    average_wall_height: f64,
    width: f64,
    height: f64,
    tags: BTreeMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolData {
    id: i32,
    name: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageData {
    image: PathBuf,
    image_dpi: f32,
    areas: Vec<AreaData>,
    geo_reference_points: Vec<crate::pages::GeoPoint>,
    geo_reference_coordinate_system: String,
    symbols: Vec<SymbolData>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ReturnData {
    pages: Vec<PageData>,
    revision: String,
}
fn area_data(
    source: &sketch_core::model::Polygon,
    poly: &sketch_core::export_model::ExportPolygon,
) -> Result<AreaData, String> {
    let (code, name) = match poly.area_type.as_str() {
        "BSMT" | "BSMT1" => (
            "BSMT".to_string(),
            if poly.name.starts_with("Finished") {
                poly.name.clone()
            } else {
                format!("Finished {}", poly.name)
            },
        ),
        "BSMT2" => (
            "BSMT".to_string(),
            if poly.name.starts_with("Unfinished") {
                poly.name.clone()
            } else {
                format!("Unfinished {}", poly.name)
            },
        ),
        _ => (poly.area_type.clone(), poly.name.clone()),
    };
    let net = poly.net.unwrap_or(poly.area);
    if !net.is_finite() {
        return Err(format!("Invalid area measurement: {}", poly.name));
    }
    Ok(AreaData {
        id: i32::try_from(source.id).map_err(|_| "Area ID exceeds the host protocol range")?,
        code,
        name,
        base_area: poly.gross,
        net_area: net,
        factor: poly.effective_multiplier.unwrap_or(1.),
        perimeter: poly.perimeter,
        description: source
            .attributes
            .get("apex.description")
            .cloned()
            .unwrap_or_else(|| poly.name.clone()),
        average_wall_height: source
            .attributes
            .get("apex.averageWallHeight")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.)
            .unwrap_or(0.),
        width: poly
            .points
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max)
            - poly
                .points
                .iter()
                .map(|p| p.x)
                .fold(f64::INFINITY, f64::min),
        height: poly
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max)
            - poly
                .points
                .iter()
                .map(|p| p.y)
                .fold(f64::INFINITY, f64::min),
        tags: source
            .attributes
            .iter()
            .filter(|(k, _)| {
                k.as_str() != "apex.description"
                    && k.as_str() != "apex.averageWallHeight"
                    && k.as_str() != "apex.calculation"
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    })
}

#[derive(Debug)]
pub struct PreparedSnapshot {
    model: serde_json::Value,
    document: Vec<u8>,
    packet: ReturnData,
}
#[derive(Debug, Clone)]
pub struct SnapshotReady {
    root: PathBuf,
    epoch: u64,
    result: Result<std::sync::Arc<PreparedSnapshot>, String>,
}
struct SnapshotJob {
    root: PathBuf,
    model: serde_json::Value,
    drawing: crate::drawing::Drawing,
    pages: crate::pages::Pages,
    area_codes: Vec<sketch_core::geometry::AreaCode>,
    library: sketch_core::symbols::SymbolLibrary,
    paper: sketch_core::print_layout::PaperSize,
    returning: bool,
    allow_empty: bool,
}
fn model_signature(model: &serde_json::Value) -> Result<u64, String> {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&serde_json::json!({"pages":model["pages"],"subject":model["subjectInfo"],"settings":model["settings"]}))
        .map_err(|e| e.to_string())?.hash(&mut hash);
    Ok(hash.finish())
}
impl SnapshotJob {
    fn render(self) -> Result<PreparedSnapshot, String> {
        let root = &self.root;
        let mut model = self.model;
        let mut definitions = std::collections::BTreeMap::new();
        if let Some(pages) = model["pages"].as_array() {
            for page in pages {
                if let Some(symbols) = page["symbols"].as_array() {
                    for symbol in symbols {
                        if let Some(name) = symbol
                            .get("symbolType")
                            .or_else(|| symbol.get("type"))
                            .and_then(|v| v.as_str())
                        {
                            if let Some(definition) = self.library.symbols.get(name) {
                                definitions.insert(name.to_owned(), definition.clone());
                            }
                        }
                    }
                }
            }
        }
        model["apexSymbols"] = serde_json::to_value(definitions).map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let pages =
            crate::exporter::build_export_pages(&self.drawing, &self.pages, &self.area_codes);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        serde_json::to_vec(&serde_json::json!({"pages":model["pages"],"subject":model["subjectInfo"],"settings":model["settings"]})).map_err(|e|e.to_string())?.hash(&mut hasher);
        let mut result = ReturnData {
            pages: Vec::new(),
            revision: format!("{:x}", hasher.finish()),
        };
        let mut defined = 0;
        for (index, page) in pages.iter().enumerate() {
            let mut drawing = self.drawing.clone();
            if index != self.pages.current {
                let stored = &self.pages.pages[index];
                drawing.lines = stored.lines.clone();
                drawing.polygons = stored.polygons.clone();
                drawing.texts = stored.labels.clone();
                drawing.symbols = stored.symbols.clone();
                drawing.dimension_chains = stored.dimension_chains.clone();
            }
            let calculated =
                drawing.recalculate_areas(&drawing.polygons, &drawing.lines, &self.area_codes);
            if let Some(polys) = model["pages"][index]["polygons"].as_array_mut() {
                for poly in polys {
                    if let Some(calc) = calculated
                        .polygons
                        .iter()
                        .find(|p| Some(p.id) == poly["id"].as_i64())
                    {
                        poly["apexBoundary"] =
                            serde_json::json!({"points":calc.points,"edges":calc.edges});
                        poly["apexFactor"] = serde_json::json!(calc.factor * calc.sign);
                        poly["apexNet"] = serde_json::json!(calc.net);
                        poly["apexGross"] = serde_json::json!(calc.gross);
                        poly["apexSubtractedFromId"] = serde_json::json!(calc.subtracted_from_id);
                    }
                }
            }
            let mut areas = Vec::new();
            for (source, poly) in drawing
                .polygons
                .iter()
                .filter(|p| p.points.len() >= 3)
                .zip(&page.polygons)
                .filter(|(_, p)| p.area_type != "UND" && !p.area_type.is_empty())
            {
                let mut data = area_data(source, poly)?;
                if let Some(calc) = calculated.polygons.iter().find(|p| p.id == source.id) {
                    data.width = calc
                        .points
                        .iter()
                        .map(|p| p.x)
                        .fold(f64::NEG_INFINITY, f64::max)
                        - calc
                            .points
                            .iter()
                            .map(|p| p.x)
                            .fold(f64::INFINITY, f64::min);
                    data.height = calc
                        .points
                        .iter()
                        .map(|p| p.y)
                        .fold(f64::NEG_INFINITY, f64::max)
                        - calc
                            .points
                            .iter()
                            .map(|p| p.y)
                            .fold(f64::INFINITY, f64::min);
                }
                areas.push(data);
                defined += 1;
            }
            let svg = crate::report_sketch::host_page(
                &drawing,
                page,
                &self.area_codes,
                &self.library,
                self.paper.sketch_height(),
            );
            let bitmap =
                crate::report_sketch::raster(&svg).ok_or("Could not render a sketch page")?;
            let path = root.join(format!("page-{index}-{stamp}.png"));
            image::RgbImage::from_raw(bitmap.width, bitmap.height, bitmap.rgb)
                .ok_or("Invalid sketch image")?
                .save(&path)
                .map_err(|e| e.to_string())?;
            let symbols = drawing
                .symbols
                .iter()
                .map(|symbol| {
                    Ok(SymbolData {
                        id: i32::try_from(symbol.id - 1)
                            .map_err(|_| "Symbol ID exceeds the host protocol range")?,
                        name: symbol.symbol_type.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            result.pages.push(PageData {
                symbols,
                image: path,
                image_dpi: 300.,
                areas,
                geo_reference_points: self.pages.pages[index].geo_reference_points.clone(),
                geo_reference_coordinate_system: self.pages.pages[index]
                    .geo_reference_coordinate_system
                    .clone(),
            });
        }
        if self.returning && !self.allow_empty && defined == 0 {
            return Err(
                "Define an area before returning measurements to the host application.".into(),
            );
        }
        let state =
            sketch_core::skx::write(&model, &Default::default()).map_err(|e| e.to_string())?;
        Ok(PreparedSnapshot {
            model,
            document: state,
            packet: result,
        })
    }
}
fn snapshot_reply(path: &Path, result: Result<(), &str>) {
    let value = match result {
        Ok(()) => serde_json::json!({"ok":true,"image":null}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    if let Ok(bytes) = serde_json::to_vec(&value) {
        let _ = crate::autosave::atomic_write(path, &bytes);
    }
}
fn publish_snapshot(
    root: &Path,
    prepared: &PreparedSnapshot,
    returning: bool,
) -> Result<u64, String> {
    let write = |name: &str, bytes: &[u8]| {
        crate::autosave::atomic_write(&root.join(name), bytes).map_err(|e| e.to_string())
    };
    write("document.skx", &prepared.document)?;
    write(
        "model.json",
        &serde_json::to_vec(&prepared.model).map_err(|e| e.to_string())?,
    )?;
    write(
        if returning {
            "result.json"
        } else {
            "snapshot.json"
        },
        &serde_json::to_vec(&prepared.packet).map_err(|e| e.to_string())?,
    )?;
    prune_snapshot_images(root, prepared.packet.pages.len() * 3);
    if returning {
        std::fs::copy(root.join("document.skx"), root.join("return-document.skx"))
            .map_err(|e| e.to_string())?;
        std::fs::copy(root.join("model.json"), root.join("return-model.json"))
            .map_err(|e| e.to_string())?;
        let receipt = root.join("return-receipt.json");
        if receipt.exists() {
            std::fs::remove_file(receipt).map_err(|e| e.to_string())?;
        }
        std::fs::write(root.join("return-ready"), "").map_err(|e| e.to_string())?;
    }
    u64::from_str_radix(&prepared.packet.revision, 16).map_err(|e| e.to_string())
}

impl crate::App {
    pub(crate) fn apply_host_codes(&mut self) -> Result<(), String> {
        if let Some(path) = std::env::var_os("SKETCH_APEX_CODES") {
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let codes =
                sketch_core::geometry::area_codes::from_json(&text).map_err(|e| e.to_string())?;
            let raw: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if !self.native_apex.is_object() {
                self.native_apex = serde_json::json!({});
            }
            self.native_apex["hostCodes"] = raw;
            self.dock.area_codes = codes.clone();
            self.area_codes = codes;
        }
        Ok(())
    }
    pub(crate) fn apply_host_code_properties(&mut self) {
        let codes = self.native_apex.get("hostCodes").and_then(|v| v.as_array());
        let metric = self.native_apex["units"].as_i64() == Some(2);
        for poly in self.drawing.polygons.iter_mut().chain(
            self.pages
                .pages
                .iter_mut()
                .flat_map(|page| page.polygons.iter_mut()),
        ) {
            if self.apex_session.is_some() {
                poly.attributes
                    .entry("apex.calculation".into())
                    .or_insert_with(|| "net".into());
            }
            let Some(codes) = codes else {
                continue;
            };
            if poly.attributes.contains_key("apex.averageWallHeight") {
                continue;
            }
            let Some(code) = codes
                .iter()
                .find(|c| c["code"].as_str() == Some(&poly.kind))
            else {
                continue;
            };
            if let Some(height) = code["nativeProperties"]["wallHeight"]
                .as_f64()
                .filter(|h| h.is_finite() && *h >= 0.)
            {
                poly.attributes.insert(
                    "apex.averageWallHeight".into(),
                    (height / if metric { 0.3048 } else { 1. }).to_string(),
                );
            }
            poly.attributes
                .entry("apex.description".into())
                .or_insert_with(|| code["description"].as_str().unwrap_or("").to_owned());
            if let Some(label) = code["nativeProperties"]["smartLabelCode"]
                .as_str()
                .filter(|s| !s.is_empty())
            {
                poly.attributes
                    .entry("SmartLabelCode".into())
                    .or_insert_with(|| label.to_owned());
            }
        }
    }
    pub(crate) fn load_apex_context(&mut self, root: &Path) -> Result<(), String> {
        let context = read_context(root)?;
        if let Some(path) = context.document {
            let model = read_document(&path)?;
            let _ = self.apply_model(&model, "Report sketch");
        } else {
            self.file_name = format!(
                "Report sketch - {}",
                context
                    .host_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
        }
        if context.page_index < self.pages.pages.len() {
            self.go_to_page(context.page_index);
        }
        let value = |key: &str| context.subject.get(key).cloned().unwrap_or_default();
        self.subject_info.file_no = value("FileNumber");
        self.subject_info.property_address = value("PropertyAddress");
        self.subject_info.city = value("City");
        self.subject_info.state = value("State");
        self.subject_info.zip_code = value("ZipCode");
        self.subject_info.county = value("County");
        self.subject_info.owner = value("Borrower");
        self.subject_info.client = value("LenderClient");
        self.subject_info.client_address = value("LenderClientAddress");
        self.subject_info.appraiser_name = value("Appraiser");
        self.apply_host_codes()?;
        self.apex_target = format!("{} - {}", value("FileNumber"), value("PropertyAddress"));
        self.file_name = if context.file_mode {
            context
                .host_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            format!("Report sketch - {}", value("FileNumber"))
        };
        if context.file_mode {
            self.apex_target = context.host_path.to_string_lossy().into_owned();
        }
        self.apex_snapshot_epoch = self.apex_snapshot_epoch.wrapping_add(1);
        self.apex_snapshot_busy = false;
        self.apex_last_signature = 0;
        self.apex_check_pending = true;
        self.apex_changed_at = std::time::Instant::now();
        for path in std::mem::take(&mut self.apex_snapshot_replies) {
            snapshot_reply(&path, Err("The host document changed."));
        }
        self.apex_session = Some(root.to_path_buf());
        self.autosave_prompt = false;
        std::fs::write(root.join("editor-ready"), "").map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Freeze a complete, calculated document for the originating host session.
    pub(crate) fn write_apex_return(&mut self, root: &Path) -> Result<(), String> {
        self.write_apex_snapshot(root, true)
    }
    fn freeze_apex_snapshot(&mut self, root: &Path, returning: bool) -> SnapshotJob {
        let model = self.build_save_model();
        SnapshotJob {
            root: root.into(),
            model,
            drawing: self.drawing.clone(),
            pages: self.pages.clone(),
            area_codes: self.area_codes.clone(),
            library: self.symbols_lib().library.clone(),
            paper: self.page_layout.paper_size,
            returning,
            allow_empty: self.apex_session.is_some(),
        }
    }
    pub(crate) fn write_apex_snapshot(
        &mut self,
        root: &Path,
        returning: bool,
    ) -> Result<(), String> {
        let prepared = self.freeze_apex_snapshot(root, returning).render()?;
        let signature = publish_snapshot(root, &prepared, returning)?;
        // A late background render must never overwrite an explicit Save/Return.
        self.apex_snapshot_epoch = self.apex_snapshot_epoch.wrapping_add(1);
        self.apex_snapshot_busy = false;
        self.apex_last_signature = signature;
        self.apex_pending_signature = signature;
        self.apex_check_pending = false;
        for path in std::mem::take(&mut self.apex_snapshot_replies) {
            snapshot_reply(&path, Ok(()));
        }
        Ok(())
    }
    fn start_apex_snapshot(
        &mut self,
        job: SnapshotJob,
        signature: u64,
        reply: Option<PathBuf>,
    ) -> iced::Task<crate::Message> {
        if self.apex_snapshot_busy && signature == self.apex_pending_signature {
            if let Some(path) = reply {
                self.apex_snapshot_replies.push(path);
            }
            return iced::Task::none();
        }
        for path in std::mem::take(&mut self.apex_snapshot_replies) {
            snapshot_reply(
                &path,
                Err("The host refresh was superseded by a newer request."),
            );
        }
        self.apex_snapshot_epoch = self.apex_snapshot_epoch.wrapping_add(1);
        self.apex_snapshot_busy = true;
        self.apex_pending_signature = signature;
        self.apex_check_pending = false;
        if let Some(path) = reply {
            self.apex_snapshot_replies.push(path);
        }
        let epoch = self.apex_snapshot_epoch;
        let root = job.root.clone();
        let (sender, receiver) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = sender.send(job.render().map(std::sync::Arc::new));
        });
        iced::Task::perform(
            async move {
                SnapshotReady {
                    root,
                    epoch,
                    result: receiver
                        .await
                        .unwrap_or_else(|_| Err("The background report renderer stopped.".into())),
                }
            },
            crate::Message::ApexSnapshotReady,
        )
    }
    pub(crate) fn apex_snapshot_completed(
        &mut self,
        ready: SnapshotReady,
    ) -> iced::Task<crate::Message> {
        if self.apex_session.as_deref() != Some(ready.root.as_path())
            || ready.epoch != self.apex_snapshot_epoch
            || !self.apex_snapshot_busy
        {
            return iced::Task::none();
        }
        self.apex_snapshot_busy = false;
        let result = ready
            .result
            .and_then(|packet| publish_snapshot(&ready.root, &packet, false));
        for path in std::mem::take(&mut self.apex_snapshot_replies) {
            snapshot_reply(&path, result.as_ref().map(|_| ()).map_err(|e| e.as_str()));
        }
        match result {
            Ok(signature) => {
                self.apex_last_signature = signature;
                iced::Task::none()
            }
            Err(error) => {
                self.apex_check_pending = true;
                self.apex_changed_at = std::time::Instant::now();
                self.toasts.show(error)
            }
        }
    }
    fn publish_apex_idle(&mut self, root: &Path) -> iced::Task<crate::Message> {
        if self.apex_returning
            || self.apex_snapshot_busy
            || !self.apex_check_pending
            || self.apex_changed_at.elapsed() < std::time::Duration::from_millis(300)
        {
            return iced::Task::none();
        }
        let job = self.freeze_apex_snapshot(root, false);
        let Ok(signature) = model_signature(&job.model) else {
            return iced::Task::none();
        };
        self.apex_check_pending = false;
        if signature == self.apex_last_signature && root.join("snapshot.json").exists() {
            return iced::Task::none();
        }
        self.start_apex_snapshot(job, signature, None)
    }
    pub(crate) fn apex_poll_is_read_only(&self) -> bool {
        let Some(root) = &self.apex_session else {
            return true;
        };
        let Ok(bytes) = std::fs::read(root.join("command.json")) else {
            return true;
        };
        serde_json::from_slice::<serde_json::Value>(&bytes).is_ok_and(|v| v["op"] == "snapshot")
    }
    pub(crate) fn apex_poll(&mut self) -> iced::Task<crate::Message> {
        let Some(root) = self.apex_session.clone() else {
            return iced::Task::none();
        };
        if root.join("host-closed").exists() {
            let _ = self.autosave_tick(true);
            return self.exit();
        }
        let command = root.join("command.json");
        let Ok(bytes) = std::fs::read(&command) else {
            return self.publish_apex_idle(&root);
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return iced::Task::none();
        };
        let Some(id) = value["id"]
            .as_str()
            .filter(|v| v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            return iced::Task::none();
        };
        let response = root.join(format!("response-{id}.json"));
        let _ = std::fs::remove_file(&command);
        if value["op"] == "snapshot" {
            let job = self.freeze_apex_snapshot(&root, false);
            let signature = match model_signature(&job.model) {
                Ok(signature) => signature,
                Err(error) => {
                    snapshot_reply(&response, Err(&error));
                    return iced::Task::none();
                }
            };
            if signature == self.apex_last_signature && root.join("snapshot.json").exists() {
                self.apex_check_pending = false;
                snapshot_reply(&response, Ok(()));
                return iced::Task::none();
            }
            return self.start_apex_snapshot(job, signature, Some(response));
        }
        let mut rendered_image = None;
        let result = (|| -> Result<(), String> {
            match value["op"].as_str().unwrap_or("") {
                "render" => {
                    let dpi = value["argument"]["dpi"].as_f64().unwrap_or(300.);
                    if !dpi.is_finite() || !(50. ..=2400.).contains(&dpi) {
                        return Err("DPI must be 50 through 2400".into());
                    }
                    let paper = match value["argument"]["paper"].as_str().unwrap_or("LEGAL") {
                        "A4" => sketch_core::print_layout::PaperSize::A4,
                        "LETTER" => sketch_core::print_layout::PaperSize::Letter,
                        _ => sketch_core::print_layout::PaperSize::Legal,
                    };
                    let (width, height) = paper.size();
                    let pixels = (width * dpi / 72.).round() as u32;
                    if width * height * dpi * dpi / (72. * 72.) > 64_000_000. {
                        return Err(
                            "Requested raster form exceeds the image memory limit; reduce EMFDPI"
                                .into(),
                        );
                    }
                    let original = self.page_layout.clone();
                    self.page_layout.paper_size = paper;
                    self.page_layout.show_page_numbers =
                        value["argument"]["pageIndex"].as_bool().unwrap_or(false);
                    self.page_layout.form = match value["argument"]["layout"]
                        .as_str()
                        .unwrap_or("CALCULATIONS")
                    {
                        "STANDARD" => crate::page_layout::Form::Standard,
                        "SKETCHONLY" => crate::page_layout::Form::SketchOnly,
                        "FULLPAGE" => crate::page_layout::Form::FullPage,
                        _ => crate::page_layout::Form::WithCalculations,
                    };
                    let pdf = self.report_pdf();
                    self.page_layout = original;
                    let images = crate::print::render_pages(&pdf, pixels)?;
                    let page = value["argument"]["page"]
                        .as_u64()
                        .ok_or("Missing render page")? as usize;
                    let bytes = images
                        .get(page)
                        .ok_or("Render page is outside the sketch")?;
                    let path = root.join(format!("form-{id}.png"));
                    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                    rendered_image = Some(path);
                }
                "subject" => {
                    let key = value["argument"]["key"]
                        .as_str()
                        .ok_or("Missing subject key")?;
                    let text = value["argument"]["value"].as_str().unwrap_or("").to_owned();
                    match key {
                        "FileNumber" => self.subject_info.file_no = text,
                        "Borrower" => self.subject_info.owner = text,
                        "PropertyAddress" => self.subject_info.property_address = text,
                        "City" => self.subject_info.city = text,
                        "County" => self.subject_info.county = text,
                        "State" => self.subject_info.state = text,
                        "ZipCode" => self.subject_info.zip_code = text,
                        "LenderClient" => self.subject_info.client = text,
                        "LenderClientAddress" => self.subject_info.client_address = text,
                        "Appraiser" => self.subject_info.appraiser_name = text,
                        _ => {}
                    }
                }
                "snapshot" => {}
                "page" => {
                    let page = value["argument"]["index"]
                        .as_u64()
                        .ok_or("Missing page index")? as usize;
                    if page >= self.pages.pages.len() {
                        return Err("Page is outside the sketch".into());
                    }
                    self.go_to_page(page);
                }
                "load" => {
                    let path = value["argument"]["path"]
                        .as_str()
                        .ok_or("Missing native model path")?;
                    let path = std::path::Path::new(path)
                        .canonicalize()
                        .map_err(|e| e.to_string())?;
                    let allowed = root.canonicalize().map_err(|e| e.to_string())?;
                    if !path.starts_with(allowed) {
                        return Err("Invalid native model path".into());
                    }
                    let model = read_document(&path)?;
                    self.apex_geo_pending = None;
                    let _ = self.apply_model(&model, "Apex sketch");
                    self.apply_host_codes()?;
                }
                "georef" => {
                    let index = value["argument"]["page"].as_u64().ok_or("Missing page")? as usize;
                    if index >= self.pages.pages.len() {
                        return Err("Page is outside the sketch".into());
                    }
                    match value["argument"]["operation"].as_str().unwrap_or("") {
                        "system" => {
                            self.pages.pages[index].geo_reference_coordinate_system = value
                                ["argument"]["value"]
                                .as_str()
                                .ok_or("Missing coordinate system")?
                                .to_owned()
                        }
                        "coordinates" => {
                            let point =
                                value["argument"]["index"].as_u64().ok_or("Missing point")?
                                    as usize;
                            let p = self.pages.pages[index]
                                .geo_reference_points
                                .get_mut(point)
                                .ok_or("Georeference point no longer exists")?;
                            p.coordinates = value["argument"]["value"]
                                .as_str()
                                .ok_or("Missing coordinates")?
                                .to_owned();
                        }
                        _ => return Err("Unsupported georeference operation".into()),
                    }
                }
                "collect-georef" => {
                    let index = value["argument"]["index"].as_u64().ok_or("Missing page")? as usize;
                    if index >= self.pages.pages.len() {
                        return Err("Page is outside the sketch".into());
                    }
                    self.go_to_page(index);
                    self.apex_geo_pending = Some((index, Vec::new()));
                    let _ = self.set_mode(crate::view_state::Mode::Select);
                    let _ = self
                        .toasts
                        .show("Click two known reference points. Esc cancels.".into());
                }
                "tag" => {
                    let id = value["argument"]["id"].as_i64().ok_or("Missing area ID")?;
                    self.save_current_page();
                    let page = self
                        .pages
                        .pages
                        .iter()
                        .position(|p| p.polygons.iter().any(|a| a.id == id))
                        .ok_or("Area no longer exists")?;
                    self.go_to_page(page);
                    self.push_undo("Area tag from host");
                    let area = self
                        .drawing
                        .polygons
                        .iter_mut()
                        .find(|p| p.id == id)
                        .ok_or("Area no longer exists")?;
                    match value["argument"]["operation"].as_str().unwrap_or("") {
                        "set" => {
                            area.attributes.insert(
                                value["argument"]["name"]
                                    .as_str()
                                    .ok_or("Missing tag name")?
                                    .to_owned(),
                                value["argument"]["value"].as_str().unwrap_or("").to_owned(),
                            );
                        }
                        "remove" => {
                            area.attributes.remove(
                                value["argument"]["name"]
                                    .as_str()
                                    .ok_or("Missing tag name")?,
                            );
                        }
                        "clear" => area.attributes.retain(|key, _| {
                            key == "apex.description"
                                || key == "apex.averageWallHeight"
                                || key == "apex.calculation"
                        }),
                        _ => return Err("Unknown tag operation".into()),
                    }
                    self.save_current_page();
                }
                "delete" | "zoom" => {
                    let id = value["argument"]["id"].as_i64().ok_or("Missing area ID")?;
                    self.save_current_page();
                    let page = self
                        .pages
                        .pages
                        .iter()
                        .position(|p| p.polygons.iter().any(|a| a.id == id))
                        .ok_or("Area no longer exists")?;
                    self.go_to_page(page);
                    if value["op"] == "delete" {
                        self.push_undo("Delete area from host");
                        let area = self
                            .drawing
                            .polygons
                            .iter()
                            .find(|p| p.id == id)
                            .ok_or("Area no longer exists")?;
                        let lines: std::collections::HashSet<_> =
                            area.edges.iter().filter_map(|e| e.line_id).collect();
                        self.drawing.polygons.retain(|p| p.id != id);
                        self.drawing.lines.retain(|l| !lines.contains(&l.id));
                        self.save_current_page();
                    } else {
                        let area = self
                            .drawing
                            .polygons
                            .iter()
                            .find(|p| p.id == id)
                            .ok_or("Area no longer exists")?;
                        let min = crate::view_state::World {
                            x: area
                                .points
                                .iter()
                                .map(|p| p.x)
                                .fold(f64::INFINITY, f64::min),
                            y: area
                                .points
                                .iter()
                                .map(|p| p.y)
                                .fold(f64::INFINITY, f64::min),
                        };
                        let max = crate::view_state::World {
                            x: area
                                .points
                                .iter()
                                .map(|p| p.x)
                                .fold(f64::NEG_INFINITY, f64::max),
                            y: area
                                .points
                                .iter()
                                .map(|p| p.y)
                                .fold(f64::NEG_INFINITY, f64::max),
                        };
                        self.view.center_and_fit(Some((min, max)));
                    }
                }
                _ => return Err("Unsupported host command".into()),
            }
            self.write_apex_snapshot(&root, false)
        })();
        let reply = match result {
            Ok(()) => serde_json::json!({"ok":true,"image":rendered_image}),
            Err(error) => serde_json::json!({"ok":false,"error":error}),
        };
        let staging = response.with_extension("new");
        if let Ok(bytes) = serde_json::to_vec(&reply) {
            if std::fs::write(&staging, bytes).is_ok() {
                let _ = std::fs::rename(staging, response);
            }
        }
        iced::Task::none()
    }
    pub(crate) fn save_to_host(&mut self) -> iced::Task<crate::Message> {
        if self.apex_returning {
            return iced::Task::none();
        }
        let Some(root) = self.apex_session.clone() else {
            return iced::Task::none();
        };
        let prepared = (|| -> Result<(), String> {
            self.write_apex_snapshot(&root, false)?;
            for (from, to) in [
                ("model.json", "return-model.json"),
                ("document.skx", "return-document.skx"),
                ("snapshot.json", "result.json"),
            ] {
                std::fs::copy(root.join(from), root.join(to)).map_err(|e| e.to_string())?;
            }
            let _ = std::fs::remove_file(root.join("save-receipt.json"));
            std::fs::write(root.join("save-ready"), "").map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = prepared {
            return self.toasts.show(error);
        }
        self.apex_returning = true;
        self.close_file_menu();
        host_receipt_task(root, false)
    }
    pub(crate) fn discard_host_changes(&mut self) -> iced::Task<crate::Message> {
        if self.apex_returning {
            return iced::Task::none();
        }
        let Some(root) = self.apex_session.as_ref() else {
            return iced::Task::none();
        };
        if let Err(error) = std::fs::write(root.join("cancel"), "") {
            return self
                .toasts
                .show(format!("Could not close the report session: {error}"));
        }
        self.apex_snapshot_epoch = self.apex_snapshot_epoch.wrapping_add(1);
        self.apex_snapshot_busy = false;
        self.apex_check_pending = false;
        for path in std::mem::take(&mut self.apex_snapshot_replies) {
            snapshot_reply(
                &path,
                Err("The sketch was closed without saving changes to the report."),
            );
        }
        self.exit()
    }
    pub(crate) fn return_to_host(&mut self) -> iced::Task<crate::Message> {
        if self.apex_returning {
            return iced::Task::none();
        }
        let Some(root) = self.apex_session.clone() else {
            return self
                .toasts
                .show("This sketch was not launched from the host application.".into());
        };
        if let Err(error) = self.write_apex_return(&root) {
            return self.toasts.show(error);
        }
        self.apex_returning = true;
        self.close_file_menu();
        host_receipt_task(root, true)
    }
}
// The application uses Iced's thread-pool executor, not a Tokio runtime.
// Wait without blocking drawing or requiring a Tokio timer/reactor.
fn host_receipt_task(root: PathBuf, returning: bool) -> iced::Task<crate::Message> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let result = wait_for_host_receipt(
            &root,
            returning,
            std::time::Duration::from_secs(if returning { 20 } else { 30 }),
        );
        let _ = sender.send(result);
    });
    iced::Task::perform(
        async move {
            receiver.await.unwrap_or_else(|_| {
                Err("The report transfer stopped. Your sketch remains open.".into())
            })
        },
        move |result| {
            if returning {
                crate::Message::ApexReturned(result)
            } else {
                crate::Message::ApexSaved(result)
            }
        },
    )
}
fn wait_for_host_receipt(
    root: &Path,
    returning: bool,
    timeout: std::time::Duration,
) -> Result<(), String> {
    let file = if returning {
        "return-receipt.json"
    } else {
        "save-receipt.json"
    };
    let start = std::time::Instant::now();
    loop {
        if let Ok(bytes) = std::fs::read(root.join(file)) {
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            return if value["ok"] == true {
                Ok(())
            } else {
                Err(value["error"]
                    .as_str()
                    .unwrap_or("The report could not receive the sketch")
                    .to_owned())
            };
        }
        if root.join("host-closed").exists() {
            return Err("The report-linked sketch session was closed. Your sketch remains available for recovery.".into());
        }
        if start.elapsed() >= timeout {
            return Err("The report did not acknowledge the transfer. Your sketch remains open and available for recovery.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(windows)]
pub fn start(config: &Path) -> Result<Session, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let session = Session::start_mode(
        config,
        std::env::temp_dir().join(format!("SketchRust-Apex-{}-{stamp}", std::process::id())),
        true,
    )?;
    let context = session.wait_for("context.json", std::time::Duration::from_secs(40))?;
    let mut last = std::fs::metadata(&context).and_then(|m| m.modified()).ok();
    let mut quiet = std::time::Instant::now();
    let started = quiet;
    while quiet.elapsed() < std::time::Duration::from_millis(250)
        && started.elapsed() < std::time::Duration::from_secs(3)
    {
        std::thread::sleep(std::time::Duration::from_millis(25));
        let stamp = std::fs::metadata(&context).and_then(|m| m.modified()).ok();
        if stamp != last {
            last = stamp;
            quiet = std::time::Instant::now();
        }
    }
    // Fail before opening a blank editor if its linked recovery state cannot
    // load. A toast after GUI startup could otherwise hide a failed reopen.
    let checked = read_context(&session.root)
        .and_then(|context| context.document.as_deref().map(read_document).transpose());
    if let Err(error) = checked {
        session.cancel();
        return Err(error);
    }
    std::env::set_var("SKETCH_APEX_SESSION", &session.root);
    Ok(session)
}

pub(crate) fn ticks() -> impl iced::futures::Stream<Item = ()> {
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        while !tx.is_closed() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if tx.unbounded_send(()).is_err() {
                break;
            }
        }
    });
    rx
}

fn prune_snapshot_images(root: &Path, keep: usize) {
    let protected: std::collections::HashSet<std::path::PathBuf> =
        if root.join("return-ready").exists() || root.join("save-ready").exists() {
            std::fs::read(root.join("result.json"))
                .ok()
                .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok())
                .and_then(|v| v["pages"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|p| p["image"].as_str().map(std::path::PathBuf::from))
                .collect()
        } else {
            Default::default()
        };
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("page-")
                .and_then(|s| s.strip_suffix(".png"))
                .and_then(|s| s.split_once('-'))
                .is_some_and(|(index, stamp)| {
                    !index.is_empty()
                        && !stamp.is_empty()
                        && index.bytes().all(|b| b.is_ascii_digit())
                        && stamp.bytes().all(|b| b.is_ascii_digit())
                })
        })
        .collect();
    files.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());
    let remove = files.len().saturating_sub(keep.max(3));
    for entry in files.into_iter().take(remove) {
        if protected.contains(&entry.path()) {
            continue;
        }
        let _ = std::fs::remove_file(entry.path());
    }
}

/// Called only after the editor has closed and its host session has stopped.
#[cfg(windows)]
pub(crate) fn finish_process(code: u32) -> ! {
    // CLR/PDF native workers can retain a lock needed by DLL_PROCESS_DETACH.
    // Avoid that exit-time deadlock after all document writes and receipts.
    // https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-exitprocess
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    unsafe {
        let _ = windows::Win32::System::Threading::TerminateProcess(
            windows::Win32::System::Threading::GetCurrentProcess(),
            code,
        );
    }
    std::process::abort()
}

#[cfg(test)]
mod receipt_tests {
    #[test]
    fn report_close_timeout_or_closed_host_is_an_error() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/report-close-022/no-receipt-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(
            super::wait_for_host_receipt(&root, true, std::time::Duration::ZERO)
                .unwrap_err()
                .contains("did not acknowledge")
        );
        std::fs::write(root.join("host-closed"), "").unwrap();
        assert!(
            super::wait_for_host_receipt(&root, true, std::time::Duration::ZERO)
                .unwrap_err()
                .contains("session was closed")
        );
    }
}
