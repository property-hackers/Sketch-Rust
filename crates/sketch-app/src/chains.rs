//! Chained dimensions (Screen 9, S9-024/035/040/045): the `state.dimensionChains`
//! records (sketch-app.js:83) and their live geometry.
//!
//! A chain wraps a seed wall; `computeChain(chain)` recomputes the maximal
//! collinear run from the CURRENT walls every render, so chains track edits
//! automatically (sketch-app.js:5013). The stored record stays small:
//! `{ id, seedLineId, side, offset, showOverall, hiddenSpans }`.
//!
//! The screen-space tier geometry (`getChainGeometry` / `getChainHit`) and
//! the rendering (`drawChains`) solve from `Drawing::compute_chain`; those
//! live here and in render.rs respectively.

use serde::{Deserialize, Serialize};

use sketch_core::geometry::{distance, DimensionChain, Pt};
use sketch_core::model::SketchLine;

/// A stored chain record (`state.dimensionChains[]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ChainRecord {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub seed_line_id: i64,
    /// +1 | −1, per `handleChainClick`'s cross-product sign.
    #[serde(default)]
    pub side: i32,
    /// World feet (`Number(options.dimensionOption) || 3` at add).
    #[serde(default)]
    pub offset: f64,
    #[serde(default = "default_true")]
    pub show_overall: bool,
    /// Indices of spans hidden by a click (drawChains skips them).
    #[serde(default)]
    pub hidden_spans: Vec<i64>,
    /// Keys this build does not model (Sketchy residue such as `sketchy_id`
    /// or `sketchy_other`), kept so a save writes them back unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl ChainRecord {
    /// `handleChainClick`'s record.
    pub fn new(id: i64, seed_line_id: i64, side: i32, offset: f64) -> Self {
        ChainRecord {
            id,
            seed_line_id,
            side: if side >= 0 { 1 } else { -1 },
            offset,
            show_overall: true,
            hidden_spans: Vec::new(),
            extra: Default::default(),
        }
    }
}

/// A chain's tier geometry (`getChainGeometry`): the computed run plus the
/// clamped `signedOffset`. All points are world feet; callers convert with
/// the view's screen mapping.
pub struct ChainTiers {
    pub computed: DimensionChain,
    /// `Math.max(0.5, offset) * (side || 1)`.
    pub signed: f64,
}

impl ChainTiers {
    pub fn solve(computed: DimensionChain, chain: &ChainRecord) -> Self {
        let mut offset = chain.offset;
        if !offset.is_finite() {
            offset = 0.0;
        }
        let signed = offset.max(0.5)
            * if chain.side == 0 {
                1.0
            } else {
                chain.side as f64
            };
        ChainTiers { computed, signed }
    }

    /// The point `t` along the run, `extra_offset` past the tier (`pointAt`).
    pub fn point_at(&self, t: f64, extra_offset: f64) -> Pt {
        let c = &self.computed;
        let n = self.signed + extra_offset;
        Pt::new(
            c.start.x + c.dir.x * t + c.normal.x * n,
            c.start.y + c.dir.y * t + c.normal.y * n,
        )
    }

    /// The point `t` ON the wall run (`wallAt`).
    pub fn wall_at(&self, t: f64) -> Pt {
        let c = &self.computed;
        Pt::new(c.start.x + c.dir.x * t, c.start.y + c.dir.y * t)
    }

    /// The second tier's extra offset (a half-step outward).
    pub fn overall_extra(&self) -> f64 {
        self.signed.signum() * 1.5f64.max(self.signed.abs() * 0.5)
    }
}

/// `handleChainClick`'s seed pass (sketch-app.js:5030): the nearest straight
/// wall within `getScreenToleranceWorld(8)` — bowed walls are skipped (v1)
/// — plus which side the click fell on (cross sign). `None` = the caller
/// toasts `Chain dimensions attach to straight walls.`
pub fn seed_wall(lines: &[SketchLine], world: Pt, zoom: f64) -> Option<(i64, i32)> {
    let geo = sketch_core::geometry::GeometryEngine::new();
    let limit = crate::selection::tolerance(zoom, 8.0);
    let (mut best, mut best_dist) = (None, limit);
    for line in lines {
        // `Math.abs(Number(line.bowAngle) || 0) >= 0.5` — straight runs only.
        if line.bow_angle.abs() >= 0.5 {
            continue;
        }
        let d = geo.distance_to_segment(world, line.start, line.end);
        if d < best_dist {
            best_dist = d;
            best = Some(line);
        }
    }
    let line = best?;
    let dx = line.end.x - line.start.x;
    let dy = line.end.y - line.start.y;
    let cross = dx * (world.y - line.start.y) - dy * (world.x - line.start.x);
    Some((line.id, if cross >= 0.0 { 1 } else { -1 }))
}

impl crate::drawing::Drawing {
    /// `computeChain(chain)` (sketch-app.js:5013): the maximal collinear run
    /// of the seed wall, split at junctions per the option, with hosted
    /// openings cutting spans. Spans recompute from the live walls.
    pub fn compute_chain(&self, chain: &ChainRecord) -> Option<DimensionChain> {
        let lines: Vec<sketch_core::geometry::Line> = self
            .lines
            .iter()
            .map(|l| sketch_core::geometry::Line {
                id: l.id,
                start: l.start,
                end: l.end,
                bow_angle: l.bow_angle,
                dimension_position: l.dimension_position,
                dimension_anchor: l.dimension_anchor,
            })
            .collect();
        let symbols: Vec<sketch_core::geometry::DimensionSymbol> = self
            .symbols
            .iter()
            .map(|s| sketch_core::geometry::DimensionSymbol {
                x: s.x,
                y: s.y,
                wall_id: s.wall_id,
                wall_gap_width: s.wall_gap_width,
                wall_snap: match s.wall_snap {
                    sketch_core::symbols::WallSnap::Side => Some("SIDE".to_string()),
                    sketch_core::symbols::WallSnap::Center => Some("CENTER".to_string()),
                    #[allow(unreachable_patterns)]
                    sketch_core::symbols::WallSnap::None => None,
                },
                wall_side_flip: s.wall_side_flip,
                flip_h: s.flip_h,
                symbol_type: Some(s.symbol_type.clone()),
            })
            .collect();
        self.geo.compute_dimension_chain(
            &lines,
            &symbols,
            chain.seed_line_id,
            0.25,
            self.options.split_walls_at_junctions,
            None,
        )
    }

    /// This chain's tiers (None when the run vanished).
    pub fn chain_tiers(&self, chain: &ChainRecord) -> Option<ChainTiers> {
        self.compute_chain(chain)
            .map(|c| ChainTiers::solve(c, chain))
    }

    /// `getChainHit(atPt)` — the hit-tested chain id, 8 px in screen space
    /// against each chain's tier lines.
    pub fn chain_hit(&self, at: Pt, zoom: f64) -> Option<i64> {
        let tolerance = crate::selection::tolerance(zoom, 8.0);
        for chain in self.dimension_chains.iter().rev() {
            let Some(tiers) = self.chain_tiers(chain) else {
                continue;
            };
            let length = tiers.computed.length;
            let (a, b) = (tiers.point_at(0.0, 0.0), tiers.point_at(length, 0.0));
            if distance_to_ray(at, a, b) <= tolerance {
                return Some(chain.id);
            }
            if chain.show_overall {
                let extra = tiers.overall_extra();
                let (a, b) = (tiers.point_at(0.0, extra), tiers.point_at(length, extra));
                if distance_to_ray(at, a, b) <= tolerance {
                    return Some(chain.id);
                }
            }
        }
        None
    }

    /// `getChainCoverage()` — wall ids covered by any chain; per-wall
    /// dimension labels for these walls are suppressed while a chain owns
    /// them (render-time only, never mutated).
    pub fn chain_coverage(&self) -> std::collections::HashSet<i64> {
        let mut covered = std::collections::HashSet::new();
        for chain in &self.dimension_chains {
            if let Some(computed) = self.compute_chain(chain) {
                covered.extend(computed.line_ids);
            }
        }
        covered
    }

    /// `handleChainClick`'s append (the undo push and the miss toast are the
    /// App's, mirroring the browser's `pushUndo('Dimension Chain')` order).
    pub fn add_chain(&mut self, seed_line_id: i64, side: i32) {
        let mut offset = self.options.dimension_option;
        if !offset.is_finite() || offset == 0.0 {
            offset = 3.0;
        }
        let id = self.gen_id();
        self.dimension_chains
            .push(ChainRecord::new(id, seed_line_id, side, offset));
    }
}

/// The closest distance from `p` to segment `ab` (the original's inline
/// `segDist` in `getChainHit`).
fn distance_to_ray(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return distance(p, a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    ((p.x - (a.x + t * dx)).powi(2) + (p.y - (a.y + t * dy)).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::geometry::GeometryEngine;
    use sketch_core::model::WallType;

    fn drawing_with_lines() -> crate::drawing::Drawing {
        let geo = GeometryEngine::new();
        let mut d = crate::drawing::Drawing::default();
        d.geo = geo;
        d
    }

    fn line(id: i64, sx: f64, sy: f64, ex: f64, ey: f64) -> SketchLine {
        SketchLine {
            id,
            start: Pt::new(sx, sy),
            end: Pt::new(ex, ey),
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
        }
    }

    #[test]
    fn a_chain_click_seeds_the_run_and_side() {
        // A T-run: the bottom wall + a perpendicular one joining at 20.
        let lines = vec![line(1, 0.0, 0.0, 40.0, 0.0), line(2, 20.0, 0.0, 20.0, 30.0)];
        // A click on the bottom wall from below seeds wall 1; the cross
        // (per the original's formula) is negative for the y-up side below.
        assert_eq!(seed_wall(&lines, Pt::new(8.0, -0.05), 1.0), Some((1, -1)));
        // From above: +1.
        assert_eq!(seed_wall(&lines, Pt::new(5.0, 0.05), 1.0), Some((1, 1)));
        // Off any wall (more than 8 px): no seed.
        assert_eq!(seed_wall(&lines, Pt::new(5.0, 3.0), 1.0), None);
        // A bowed wall is skipped ("straight runs only").
        let mut bowed = line(9, 0.0, 50.0, 10.0, 50.0);
        bowed.bow_angle = 12.0;
        let lines2 = [bowed.clone()];
        assert_eq!(seed_wall(&lines2, Pt::new(5.0, 50.05), 1.0), None);
        let _ = &drawing_with_lines;
    }

    #[test]
    fn added_chains_recompute_spans_from_the_live_walls() {
        let mut d = drawing_with_lines();
        d.lines = vec![line(1, 0.0, 0.0, 40.0, 0.0), line(2, 20.0, 0.0, 20.0, 30.0)];
        let seed = seed_wall(&d.lines, Pt::new(8.0, -0.05), 1.0).unwrap();
        d.add_chain(seed.0, seed.1);
        assert_eq!(d.dimension_chains.len(), 1);
        let chain = d.dimension_chains[0].clone();
        assert_eq!(chain.side, -1);
        assert_eq!(chain.seed_line_id, 1);
        // The run spans the whole bottom wall. The junction split is off by
        // default (`splitWallsAtJunctions`), so no middle ref appears.
        let computed = d.compute_chain(&chain).unwrap();
        assert_eq!(computed.refs, vec![0.0, 40.0]);
        // Coverage suppresses the per-wall label of the covered walls.
        assert_eq!(d.chain_coverage().into_iter().collect::<Vec<_>>(), vec![1]);
        // The hit-test finds the chain near its tier (0.5 ft out); the far
        // point misses both tiers (the overall tier sits 1.5 ft out).
        d.dimension_chains[0].offset = 0.5;
        assert_eq!(d.chain_hit(Pt::new(10.0, -0.6), 1.0), Some(chain.id));
        assert_eq!(d.chain_hit(Pt::new(10.0, -3.0), 1.0), None);
    }

    #[test]
    fn drag_moves_the_offset_and_flips_the_side() {
        use crate::chains::ChainTiers;
        let mut d = drawing_with_lines();
        d.lines = vec![line(1, 0.0, 0.0, 40.0, 0.0)];
        d.add_chain(1, 1);
        d.dimension_chains[0].offset = 3.0;
        let computed = d.compute_chain(&d.dimension_chains[0]).unwrap();
        let tiers = ChainTiers::solve(computed, &d.dimension_chains[0]);
        assert!((tiers.signed - 3.0).abs() < 1e-9);
        // Dragged to the negative side past 0.5: the side flips and the
        // offset clamps down to the distance.
        let flipped = ChainRecord {
            side: -1,
            offset: 2.0,
            ..d.dimension_chains[0].clone()
        };
        let computed = d.compute_chain(&flipped).unwrap();
        let tiers = ChainTiers::solve(computed, &flipped);
        assert!((tiers.signed + 2.0).abs() < 1e-9);
        assert_eq!(tiers.point_at(10.0, 0.0), Pt::new(10.0, -2.0));
    }
}
