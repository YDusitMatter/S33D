//! Keep saved thread geometry and drawing annotations in the transformed frame.
use super::*;

impl S33DApp {
    pub(super) fn transform_thread_state(
        &mut self,
        id: BodyId,
        original: ShapeId,
        transformed: ShapeId,
        matrix: &glam::Mat4,
    ) {
        let Some(Feature::Derived { source, .. }) = self.doc.body(id).map(|b| &b.feature) else {
            return;
        };
        let Some(text) = source.strip_prefix("Thread v1: ") else {
            return;
        };
        let updated = (|| {
            let saved: crate::threads::Saved = serde_json::from_str(text).ok()?;
            let before = self.kernel.tessellate(original).ok()?;
            if saved.mesh_signature != crate::threads::mesh_signature(&before) {
                return None;
            }
            let after = self.kernel.tessellate(transformed).ok()?;
            let saved = saved.transformed(id, &matrix.as_dmat4(), &after)?;
            Some(format!(
                "Thread v1: {}",
                serde_json::to_string(&saved).ok()?
            ))
        })();
        if let Some(body) = self.doc.body_mut(id) {
            body.feature = Feature::Derived {
                source: updated.unwrap_or_else(|| "Transformed thread geometry".into()),
                operation: None,
            };
        }
    }
}
