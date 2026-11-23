//! Checked region geometry is computed once per body revision, including failures.
use s33d_kernel::Region2D;
use s33d_model::{BodyId, ValidatedRegionMesh};
use std::{collections::HashMap, sync::Arc};

type MeshResult = Result<Arc<ValidatedRegionMesh>, String>;

#[derive(Default)]
pub(super) struct RegionMeshCache {
    bodies: HashMap<BodyId, (u64, HashMap<usize, MeshResult>)>,
}

impl RegionMeshCache {
    pub fn resolve(
        &mut self,
        body: BodyId,
        revision: u64,
        index: usize,
        region: &Region2D,
    ) -> (MeshResult, bool) {
        let entry = self
            .bodies
            .entry(body)
            .or_insert_with(|| (revision, HashMap::new()));
        if entry.0 != revision {
            *entry = (revision, HashMap::new());
        }
        if let Some(result) = entry.1.get(&index) {
            return (result.clone(), false);
        }
        let result = s33d_model::triangulate_region(&region.outer, &region.holes)
            .map(Arc::new)
            .map_err(|error| format!(
                "Sketch region validation failed: body={body}, revision={revision:016x}, region={index}, rings={}: {error}",
                region.holes.len() + 1,
            ));
        entry.1.insert(index, result.clone());
        (result, true)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(BodyId) -> bool) {
        self.bodies.retain(|id, _| keep(*id));
    }

    /// Reading an active selection's diagnostic must neither recalculate the
    /// region nor consume/repeat its transient notification.
    pub fn error(&self, body: BodyId, revision: u64, index: usize) -> Option<&str> {
        let (cached_revision, meshes) = self.bodies.get(&body)?;
        if *cached_revision != revision {
            return None;
        }
        meshes.get(&index)?.as_ref().err().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_success_and_failure_only_for_the_same_owner_and_revision() {
        let mut cache = RegionMeshCache::default();
        let valid = Region2D {
            outer: vec![[0., 0.], [2., 0.], [2., 2.], [0., 2.]],
            holes: vec![],
        };
        let (first, fresh) = cache.resolve(1, 7, 0, &valid);
        assert!(fresh);
        let (second, fresh) = cache.resolve(1, 7, 0, &valid);
        assert!(!fresh);
        assert!(Arc::ptr_eq(&first.unwrap(), &second.unwrap()));
        let invalid = Region2D {
            outer: vec![[0., 0.], [2., 2.], [0., 2.], [2., 0.]],
            holes: vec![],
        };
        assert!(cache.resolve(1, 8, 0, &invalid).0.is_err());
        assert!(!cache.resolve(1, 8, 0, &invalid).1);
        assert!(cache.resolve(2, 8, 0, &valid).0.is_ok());
        cache.retain(|id| id != 1);
        assert!(cache.resolve(1, 8, 0, &valid).1);
    }
}
