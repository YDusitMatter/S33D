//! Non-geometric object groups. Every change is one document transaction.
use super::*;

impl S33DApp {
    pub fn can_group_selection(&self) -> bool {
        self.mode == Mode::Idle && self.selected.len() >= 2
    }

    pub fn can_ungroup_selection(&self) -> bool {
        self.mode == Mode::Idle
            && self
                .doc
                .groups
                .iter()
                .any(|g| g.members.iter().any(|id| self.selected.contains(id)))
    }

    pub fn group_selected(&mut self) {
        if !self.can_group_selection() {
            return;
        }
        let ids = self.selected.clone();
        self.begin_document_change();
        if let Some(id) = self.doc.group_objects(&ids) {
            self.record_history(format!("Group {id} ({} objects)", ids.len()));
            self.bump_scene();
        }
    }

    pub fn ungroup_selected(&mut self) {
        if !self.can_ungroup_selection() {
            return;
        }
        let ids = self.selected.clone();
        self.begin_document_change();
        self.doc.ungroup_objects(&ids);
        self.record_history("Ungroup selected objects");
        self.bump_scene();
    }

    pub fn select_group(&mut self, id: u64, additive: bool) {
        let Some(group) = self.doc.groups.iter().find(|g| g.id == id).cloned() else {
            return;
        };
        let all_selected = group.members.iter().all(|id| self.selected.contains(id));
        if !additive {
            self.selected.clear();
        }
        for member in group.members {
            if (self.align_active || self.mode == Mode::Boolean)
                && !self
                    .body(member)
                    .is_some_and(|b| b.kind == BodyKind::Solid && b.shape.is_some())
            {
                continue;
            }
            if additive && all_selected {
                self.selected.retain(|id| *id != member);
            } else if !self.selected.contains(&member) {
                self.selected.push(member);
            }
        }
        if self.selected.len() > 1 {
            if let Some(operation) = self.active_single_object_tool() {
                self.clear_object_selection_for_conflict(format!(
                    "{operation} accepts exactly one object; group selection cancelled"
                ));
                return;
            }
            if self
                .boolean_ctx
                .as_ref()
                .is_some_and(|ctx| ctx.op != BoolOp::Union)
            {
                self.clear_object_selection_for_conflict("Choose one Boolean operand at a time");
                return;
            }
        }
        self.selected_edge = None;
        self.hover_edge = None;
        self.hover_face = None;
        self.face_pick = None;
        self.clear_region_selection();
        if self.align_active {
            self.refresh_align_reference(None);
        }
        self.bump_scene();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_selection_and_shift_follow_display_order_including_hidden_rows() {
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        for id in 1..=4 {
            app.doc.add_body(Body {
                id,
                name: format!("Object {id}"),
                kind: BodyKind::Solid,
                feature: Feature::Imported {
                    path: "fixture".into(),
                },
                shape: None,
                material: Material::default(),
                visible: id != 4,
            });
        }
        app.selected = vec![1, 4];
        app.group_selected();
        let group = app.doc.groups[0].id;
        let state = app.agent_document_value();
        assert_eq!(state["groups"][0]["id"], group);
        assert_eq!(state["groups"][0]["members"], json!([1, 4]));
        assert_eq!(state["bodies"][3]["visible"], false);
        app.selected.clear();
        app.select_group(group, false);
        assert_eq!(app.selected, vec![1, 4]);
        assert!(!app.body(4).unwrap().visible);
        app.object_list_anchor = Some(4);
        app.select_object_range(2, false);
        assert_eq!(
            app.selected,
            vec![4, 2],
            "display order is group [1,4], then [2,3]"
        );
        app.array_active = true;
        app.select_group(group, false);
        assert!(
            app.selected.is_empty(),
            "single-object tools must reject a group"
        );
        assert!(!app.array_active);
        assert_eq!(app.doc.groups.len(), 1);
    }
}
