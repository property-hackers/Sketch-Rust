//! Pages + underlay (refs/sketch-app.js addPage / deletePage / goToPage /
//! setUnderlay / getUnderlayPage, and refs/ui-manager.js
//! updatePageDisplay / setNorthAngle). Each page owns its north angle and,
//! like the original's `pages[]`, its content: lines, polygons, labels
//! (text), and symbols — the live drawing swaps on page switches
//! (`saveCurrentPage` / `loadPageData`). Dialog flow (confirm before
//! delete, alert on the last page) lives in the caller.

use serde::{Deserialize, Serialize};

use crate::placed_symbol::PlacedSymbol;
use crate::placed_text::PlacedText;
use sketch_core::model::{Polygon, SketchLine};

/// One page (`ensurePageModel`: `{ lines, polygons, labels, symbols,
/// northAngle, dimensionChains }`; dimension chains join with Screen 9).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    #[serde(default)]
    pub geo_reference_points: Vec<GeoPoint>,
    #[serde(default)]
    pub geo_reference_coordinate_system: String,
    /// `northAngle`, degrees 0..360 clockwise from up.
    #[serde(default)]
    pub north_angle: f64,
    #[serde(default)]
    pub lines: Vec<SketchLine>,
    #[serde(default)]
    pub polygons: Vec<Polygon>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "crate::area_repair::deserialize_records"
    )]
    pub reopened_areas: Vec<crate::area_repair::ReopenedArea>,
    /// `labels` — the original's name for placed text items.
    #[serde(default)]
    pub labels: Vec<PlacedText>,
    #[serde(default)]
    pub symbols: Vec<PlacedSymbol>,
    /// `dimensionChains` (screen 9).
    #[serde(default)]
    pub dimension_chains: Vec<crate::chains::ChainRecord>,
    /// Keys this build does not model (Sketchy residue such as `sketchy_id`
    /// or `sketchy_other`), kept so a save writes them back unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub coordinates: String,
}

#[derive(Debug, Clone)]
pub struct Pages {
    pub pages: Vec<Page>,
    /// `currentPage` (0-based).
    pub current: usize,
    /// `underlayPageIndex`.
    pub underlay: Option<usize>,
}

impl Default for Pages {
    fn default() -> Self {
        Pages {
            pages: vec![Page::default()],
            current: 0,
            underlay: None,
        }
    }
}

/// `normalizeNorthAngle`.
pub fn normalize_north(degrees: f64) -> f64 {
    ((degrees % 360.0) + 360.0) % 360.0
}

impl Pages {
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn page(&self) -> &Page {
        &self.pages[self.current]
    }

    /// `#page-display`: "N of M".
    pub fn display(&self) -> String {
        format!("{} of {}", self.current + 1, self.pages.len())
    }

    /// Previous is disabled on the first page.
    pub fn can_go_prev(&self) -> bool {
        self.current > 0
    }

    pub fn is_last(&self) -> bool {
        self.current + 1 >= self.pages.len()
    }

    /// `goToPage(index)`; out-of-range is ignored. Returns true when a page
    /// was loaded.
    pub fn go_to(&mut self, index: usize) -> bool {
        if index >= self.pages.len() {
            return false;
        }
        self.current = index;
        if self.underlay == Some(self.current) {
            self.underlay = None;
        }
        true
    }

    /// `addPage()`: insert a blank page after the current one and go there;
    /// the underlay keeps pointing at the same page.
    pub fn add_page(&mut self) {
        self.current += 1;
        self.pages.insert(self.current, Page::default());
        if let Some(u) = self.underlay.as_mut() {
            if *u >= self.current {
                *u += 1;
            }
        }
    }

    /// `deletePage()` after its confirm: false (nothing removed) on the
    /// last remaining page — the caller shows "Cannot delete the last page."
    pub fn delete_current(&mut self) -> bool {
        if self.pages.len() <= 1 {
            return false;
        }
        self.pages.remove(self.current);
        match self.underlay {
            Some(u) if u == self.current => self.underlay = None,
            Some(u) if u > self.current => self.underlay = Some(u - 1),
            _ => {}
        }
        if self.current >= self.pages.len() {
            self.current = self.pages.len() - 1;
        }
        if self.underlay == Some(self.current) {
            self.underlay = None;
        }
        true
    }

    /// `setUnderlay(index)`: only another existing page; anything else
    /// (incl. -1 "None") clears it.
    pub fn set_underlay(&mut self, index: Option<usize>) {
        self.underlay = index.filter(|&i| i < self.pages.len() && i != self.current);
    }

    /// The underlay select's options `(value, label)`: None, then every
    /// other page (updatePageDisplay).
    pub fn underlay_options(&self) -> Vec<(Option<usize>, String)> {
        let mut out = vec![(None, "Underlay: None".to_string())];
        for i in 0..self.pages.len() {
            if i != self.current {
                out.push((Some(i), format!("Underlay: Page {}", i + 1)));
            }
        }
        out
    }

    /// The label on display (`underlay.value`).
    pub fn underlay_label(&self) -> String {
        match self.underlay {
            Some(i) if i != self.current && i < self.pages.len() => {
                format!("Underlay: Page {}", i + 1)
            }
            _ => "Underlay: None".to_string(),
        }
    }

    /// `setNorthAngle(value)`: parseFloat; non-finite input is ignored.
    pub fn set_north(&mut self, text: &str) -> bool {
        match parse_float(text) {
            Some(v) => {
                self.pages[self.current].north_angle = normalize_north(v);
                true
            }
            None => false,
        }
    }
}

/// JS `parseFloat`: the longest leading decimal prefix ("90deg" -> 90,
/// " 45" -> 45, "abc" -> None).
pub fn parse_float(text: &str) -> Option<f64> {
    let t = text.trim_start();
    let mut end = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exp = false;
    let bytes = t.as_bytes();
    while end < bytes.len() {
        let c = bytes[end] as char;
        let ok = match c {
            '0'..='9' => {
                seen_digit = true;
                true
            }
            '+' | '-' => end == 0 || matches!(bytes[end - 1] as char, 'e' | 'E'),
            '.' if !seen_dot && !seen_exp => {
                seen_dot = true;
                true
            }
            'e' | 'E' if seen_digit && !seen_exp => {
                seen_exp = true;
                true
            }
            _ => false,
        };
        if !ok {
            break;
        }
        end += 1;
    }
    // Back off a dangling exponent/sign ("1e", "1e+").
    let mut s = &t[..end];
    while s.ends_with(['e', 'E', '+', '-']) {
        s = &s[..s.len() - 1];
    }
    if t[..].starts_with("Infinity") || t.starts_with("+Infinity") || t.starts_with("-Infinity") {
        return None; // not finite: setNorthAngle ignores it
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_add_prev_and_underlay_follow_the_original() {
        // Live original: Next on the last page -> confirm -> OK -> "2 of 2",
        // Previous enabled, underlay options None + Page 1; Previous ->
        // "1 of 2", disabled, options None + Page 2; underlay = Page 2.
        let mut p = Pages::default();
        assert_eq!(p.display(), "1 of 1");
        assert!(!p.can_go_prev() && p.is_last());
        p.add_page();
        assert_eq!((p.display(), p.can_go_prev()), ("2 of 2".to_string(), true));
        let labels: Vec<String> = p.underlay_options().into_iter().map(|o| o.1).collect();
        assert_eq!(labels, ["Underlay: None", "Underlay: Page 1"]);
        assert!(p.go_to(p.current - 1));
        assert_eq!(
            (p.display(), p.can_go_prev()),
            ("1 of 2".to_string(), false)
        );
        p.set_underlay(Some(1));
        assert_eq!(
            (p.underlay, p.underlay_label()),
            (Some(1), "Underlay: Page 2".to_string())
        );
        // Going to the underlay page drops it; a bad index clears it.
        p.go_to(1);
        assert_eq!(p.underlay, None);
        p.set_underlay(Some(1));
        assert_eq!(
            p.underlay, None,
            "the current page can't be its own underlay"
        );
        p.set_underlay(Some(7));
        assert_eq!(p.underlay, None);
    }

    #[test]
    fn add_keeps_and_delete_repoints_the_underlay() {
        let mut p = Pages::default();
        p.add_page(); // 2 pages, on 2
        p.add_page(); // 3 pages, on 3
        p.go_to(0);
        p.set_underlay(Some(2)); // page 3
        p.add_page(); // insert after page 1 -> old page 3 is index 3
        assert_eq!((p.current, p.underlay), (1, Some(3)));
        // Delete the current page (index 1): underlay index 3 -> 2.
        assert!(p.delete_current());
        assert_eq!((p.len(), p.current, p.underlay), (3, 1, Some(2)));
        // Delete the last page while it is the underlay: dropped, land on 2.
        p.go_to(2);
        assert_eq!(p.underlay, None);
        p.go_to(0);
        p.set_underlay(Some(1));
        p.go_to(1);
        assert_eq!(p.underlay, None);
        // "Cannot delete the last page."
        let mut one = Pages::default();
        assert!(!one.delete_current());
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn north_is_per_page_parsed_like_parse_float_and_normalized() {
        let mut p = Pages::default();
        assert!(p.set_north("90"));
        assert_eq!(p.page().north_angle, 90.0);
        assert!(p.set_north("-90"));
        assert_eq!(p.page().north_angle, 270.0);
        assert!(p.set_north("405.5deg"));
        assert_eq!(p.page().north_angle, 45.5);
        assert!(!p.set_north("abc"));
        assert!(!p.set_north(""));
        assert_eq!(p.page().north_angle, 45.5);
        p.add_page();
        assert_eq!(p.page().north_angle, 0.0, "a new page starts at 0");
        assert_eq!(parse_float(" 1e2x"), Some(100.0));
        assert_eq!(parse_float("1e"), Some(1.0));
        assert_eq!(parse_float(".5"), Some(0.5));
    }
}
