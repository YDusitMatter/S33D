//! A region index is meaningful only within its original profile geometry.
use super::*;
use std::hash::{Hash, Hasher};

/// Kept with transient selection, including click and background-work snapshots.
/// This is deliberately not a persistent topological naming scheme: editing a
/// profile invalidates the selection instead of guessing which new face it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RegionSelectionSource {
    owner: BodyId,
    geometry_fingerprint: u64,
    region_count: usize,
}

fn profile_fingerprint(sketch: &SketchFeature) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    s33d_model::entities_fingerprint(&sketch.entities).hash(&mut hash);
    for coordinate in sketch
        .plane
        .origin
        .iter()
        .chain(&sketch.plane.x_axis)
        .chain(&sketch.plane.y_axis)
    {
        coordinate.to_bits().hash(&mut hash);
    }
    sketch.world_xyz_coordinates.hash(&mut hash);
    sketch.on_body.hash(&mut hash);
    hash.finish()
}

impl S33DApp {
    fn region_geometry_fingerprint(&self, owner: BodyId) -> Option<u64> {
        // Ordinary Sketch is the frequent path; avoid cloning its entities.
        if let Some(sketch) = self.body(owner).and_then(Body::sketch) {
            return Some(profile_fingerprint(sketch));
        }
        self.profile_sketch(owner).as_ref().map(profile_fingerprint)
    }

    pub(crate) fn clear_region_selection(&mut self) {
        self.selected_regions.clear();
        self.selected_region_source = None;
    }

    /// Bind indices at the time of the pick. Never adopt bare indices after an
    /// owner change: an unbound nonempty vector is invalid input, even if its
    /// indices happen to fit the newly selected profile.
    pub(crate) fn set_region_selection(&mut self, owner: BodyId, mut indices: Vec<usize>) -> bool {
        if indices.is_empty() {
            self.clear_region_selection();
            return true;
        }
        let Some(geometry_fingerprint) = self.region_geometry_fingerprint(owner) else {
            self.clear_region_selection();
            return false;
        };
        let region_count = self.regions_of(owner).len();
        if self.selected.as_slice() != [owner]
            || !self.body(owner).is_some_and(|body| body.visible)
            || indices.iter().any(|&index| index >= region_count)
        {
            self.clear_region_selection();
            return false;
        }
        indices.sort_unstable();
        indices.dedup();
        self.selected_regions = indices;
        self.selected_region_source = Some(RegionSelectionSource {
            owner,
            geometry_fingerprint,
            region_count,
        });
        true
    }

    pub(crate) fn region_selection_is_current(&self, owner: BodyId) -> bool {
        let Some(source) = self.selected_region_source else {
            return false;
        };
        !self.selected_regions.is_empty()
            && source.owner == owner
            && self.selected.as_slice() == [owner]
            && self.body(owner).is_some_and(|body| body.visible)
            && self.region_geometry_fingerprint(owner) == Some(source.geometry_fingerprint)
            && self
                .selected_regions
                .iter()
                .all(|&index| index < source.region_count)
    }

    /// Rendering and command consumers must use this view, never a raw index
    /// vector. It remains safe even before a pending scene invalidation runs.
    pub(crate) fn selected_regions_for(&self, owner: BodyId) -> &[usize] {
        if self.region_selection_is_current(owner) {
            &self.selected_regions
        } else {
            &[]
        }
    }

    /// Returns true when a nonempty, invalid selection was discarded. Call at
    /// selection/revision boundaries and again before a modeling command.
    pub(crate) fn sanitize_region_selection(&mut self) -> bool {
        if self.selected_regions.is_empty() {
            self.selected_region_source = None;
            return false;
        }
        if self
            .selected_region_source
            .is_some_and(|source| self.region_selection_is_current(source.owner))
        {
            return false;
        }
        self.clear_region_selection();
        true
    }
}

#[cfg(test)]
#[path = "region_selection_qa.rs"]
mod qa;
