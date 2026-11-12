//! Validated, undoable layer JSON import with independent IDs.
use crate::{layers, pages, App, Message};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
impl App {
    pub(crate) fn export_layer_data(&mut self, id: &str) -> Value {
        self.save_current_page();
        let branch = self.layers.descendant_layer_ids(id);
        let mut page = self.page_to_save(self.pages.page());
        // page_to_save's numeric conversion applies only to labels/symbols;
        // lines and polygons already carry named IDs.
        page["lines"] = serde_json::to_value(&self.drawing.lines).unwrap();
        page["polygons"] = serde_json::to_value(&self.drawing.polygons).unwrap();
        for key in ["lines", "polygons", "labels", "symbols"] {
            page[key].as_array_mut().unwrap().retain(|v| {
                let layer = v.get("layerId").and_then(Value::as_str).unwrap_or_else(|| {
                    self.layers
                        .items
                        .first()
                        .map(|l| l.id.as_str())
                        .unwrap_or("")
                });
                branch.iter().any(|id| id == layer)
            });
        }
        let live: BTreeSet<_> = page["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|l| l["id"].as_i64())
            .collect();
        page["dimensionChains"] = json!(self
            .drawing
            .dimension_chains
            .iter()
            .filter(|c| live.contains(&c.seed_line_id))
            .collect::<Vec<_>>());
        if let Some(records) = page.get_mut("reopenedAreas").and_then(Value::as_array_mut) {
            records.retain(|record| {
                let layer = record
                    .pointer("/original/layerId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| self.layers.default_layer_id());
                branch.contains(&layer)
            });
        }
        page["version"] = json!("Sketch-Layer");
        page["layers"] = json!(self
            .layers
            .items
            .iter()
            .filter(|l| branch.contains(&l.id))
            .collect::<Vec<_>>());
        page["customSymbols"] = json!(self.portable_custom_symbols());
        page
    }

    pub(crate) fn import_layer_file(&mut self) -> iced::Task<Message> {
        #[cfg(not(test))]
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Sketch layer", &["json"])
            .pick_file()
        {
            let result = std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|body| serde_json::from_str(&body).map_err(|e| e.to_string()))
                .and_then(|data| self.import_layer_data(&data));
            return self.toasts.show(match result {
                Ok(()) => "Layer imported.".into(),
                Err(e) => format!("Layer import failed: {e}"),
            });
        }
        iced::Task::none()
    }
    pub(crate) fn import_layer_data(&mut self, data: &Value) -> Result<(), String> {
        if data["version"] != "Sketch-Layer" {
            return Err("Expected a Sketch-Layer JSON file.".into());
        }
        let mut imported: Vec<layers::Layer> =
            serde_json::from_value(data["layers"].clone()).map_err(|e| e.to_string())?;
        if imported.is_empty() {
            return Err("The file has no layers.".into());
        }
        let ids: BTreeSet<_> = imported.iter().map(|l| l.id.clone()).collect();
        if ids.len() != imported.len() || ids.contains("") {
            return Err("Layer IDs must be unique and nonempty.".into());
        }
        for l in &imported {
            let mut visited = BTreeSet::new();
            let mut parent = l.parent_id.as_ref();
            while let Some(p) = parent {
                if p == &l.id || !visited.insert(p.clone()) {
                    return Err("Layer hierarchy contains a cycle.".into());
                }
                parent = imported
                    .iter()
                    .find(|l| &l.id == p)
                    .and_then(|l| l.parent_id.as_ref());
            }
        }
        let mut next = self.drawing.next_id();
        let mut map = BTreeMap::new();
        for l in &imported {
            loop {
                let id = format!("layer-import-{next}");
                next += 1;
                if self.layers.layer(&id).is_none() && !map.values().any(|v| v == &id) {
                    map.insert(l.id.clone(), id);
                    break;
                }
            }
        }
        let mut layers = self.layers.clone();
        for l in &mut imported {
            let old = l.id.clone();
            l.id = map[&old].clone();
            l.parent_id = l.parent_id.as_ref().and_then(|p| map.get(p)).cloned();
            l.name = layers.unique_layer_name(&l.name);
            layers.items.push(l.clone());
        }
        let mut page = data.clone();
        let mut entity_ids = BTreeMap::new();
        for key in ["lines", "polygons", "labels", "symbols", "dimensionChains"] {
            if page.get(key).is_some_and(|v| !v.is_array()) {
                return Err(format!("{key} must be an array."));
            }
            if page.get(key).is_none() {
                page[key] = json!([]);
            }
            for v in page[key].as_array().unwrap() {
                let id = v["id"].as_i64().ok_or("Entity ID must be an integer.")?;
                if entity_ids.insert(id, next).is_some() {
                    return Err("Duplicate entity IDs.".into());
                }
                next += 1;
            }
        }
        if let Some(records) = page.get("reopenedAreas").and_then(Value::as_array) {
            for record in records {
                if let Some(id) = record.pointer("/original/id").and_then(Value::as_i64) {
                    if let std::collections::btree_map::Entry::Vacant(slot) = entity_ids.entry(id) {
                        slot.insert(next);
                        next += 1;
                    }
                }
            }
        }
        fn remap(
            v: &mut Value,
            ids: &BTreeMap<i64, i64>,
            layers: &BTreeMap<String, String>,
            key: &str,
        ) {
            match v {
                Value::Object(o) => {
                    for (k, v) in o {
                        remap(v, ids, layers, k)
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        remap(v, ids, layers, key)
                    }
                }
                _ => {
                    if key == "layerId" {
                        if let Some(s) = v.as_str() {
                            *v = layers.get(s).map(|s| json!(s)).unwrap_or(Value::Null)
                        }
                    } else if matches!(
                        key,
                        "id" | "lineId" | "wallId" | "subtractTargetId" | "lineIds" | "seedLineId"
                    ) {
                        if let Some(id) = v.as_i64() {
                            *v = ids.get(&id).map(|n| json!(n)).unwrap_or(Value::Null)
                        }
                    }
                }
            }
        }
        remap(&mut page, &entity_ids, &map, "");
        if let Some(records) = page.get_mut("reopenedAreas").and_then(Value::as_array_mut) {
            for record in records {
                record["firstReplacementId"] = json!(next);
            }
        }
        // Older exports used 1-based numeric indices for text/symbol layers.
        for key in ["labels", "symbols"] {
            for v in page[key].as_array_mut().unwrap() {
                if let Some(i) = v["layerId"].as_u64() {
                    v["layerId"] = i
                        .checked_sub(1)
                        .and_then(|i| imported.get(i as usize))
                        .map(|l| json!(l.id))
                        .unwrap_or(Value::Null);
                }
            }
        }
        let mut wrapper = json!({"pages":[page]});
        Self::normalize_entity_layers(&layers, &mut wrapper);
        let mut page: pages::Page = serde_json::from_value(wrapper["pages"][0].clone())
            .map_err(|e| format!("Invalid geometry: {e}"))?;
        // Imported elements are new: they must not carry another file's ids.
        page.extra.clear();
        page.lines.iter_mut().for_each(|l| l.forget_sketchy_id());
        page.polygons.iter_mut().for_each(|p| p.forget_sketchy_id());
        page.labels.iter_mut().for_each(|t| t.forget_sketchy_id());
        page.symbols.iter_mut().for_each(|s| s.forget_sketchy_id());
        let custom: BTreeMap<String, sketch_core::symbols::Symbol> = match data.get("customSymbols")
        {
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| e.to_string())?,
            None => BTreeMap::new(),
        };
        for (id, s) in &custom {
            if id != &s.id || s.geometry.is_empty() {
                return Err("Invalid custom symbol definition.".into());
            }
            if self
                .symbols_lib()
                .symbol(id)
                .is_some_and(|existing| existing != s)
            {
                return Err(format!("Conflicting symbol definition: {id}"));
            }
        }
        self.resolve_symbol_aliases(&mut page.symbols);
        page.symbols
            .retain(|s| !self.symbol_changes.deleted.contains(&s.symbol_type));
        // JSON cannot carry non-finite numbers; typed parsing validates required geometry fields.
        self.push_undo("Import Layer");
        self.symbol_changes.custom.extend(custom);
        self.rebuild_symbols();
        self.layers = layers;
        self.layers.active = imported
            .iter()
            .find(|l| !l.is_group)
            .unwrap_or(&imported[0])
            .id
            .clone();
        for l in &mut self.layers.items {
            if map.values().any(|id| id == &l.id) {
                l.area_poly_id = l
                    .area_poly_id
                    .as_ref()
                    .and_then(|id| id.parse::<i64>().ok())
                    .and_then(|id| entity_ids.get(&id))
                    .map(ToString::to_string);
            }
        }
        self.drawing.lines.extend(page.lines);
        self.drawing.polygons.extend(page.polygons);
        self.drawing.reopened_areas.extend(page.reopened_areas);
        self.drawing.texts.extend(page.labels);
        self.drawing.symbols.extend(page.symbols);
        self.drawing.dimension_chains.extend(page.dimension_chains);
        self.drawing.set_next_id(next);
        self.selection.clear();
        self.sync_canvas();
        Ok(())
    }
}
