//! Confirm workspace exit gestures without committing before consent.
use super::*;

pub(super) struct WorkspaceExitClick {
    pub response: egui::Response,
    pub modifiers: egui::Modifiers,
    pub count: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            events,
            ..Default::default()
        }
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]
    }

    fn open_prompt(ctx: &egui::Context, language: UiLanguage) -> S33DApp {
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.settings = AppSettings::default();
        app.settings.language = language;
        app.mode = Mode::PickPlane;
        let pos = egui::pos2(700., 500.);
        let _ = ctx.run(raw(vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_rect(ui.max_rect(), egui::Sense::click_and_drag());
            });
        });
        let _ = ctx.run(raw(pointer(pos, true)), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let response = ui.allocate_rect(ui.max_rect(), egui::Sense::click_and_drag());
                assert!(response.interact_pointer_pos().is_some());
                assert!(app.request_tool_exit_prompt(&response, egui::Modifiers::NONE, 2));
            });
        });
        frame(ctx, &mut app, pointer(pos, false));
        app
    }

    fn frame(
        ctx: &egui::Context,
        app: &mut S33DApp,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        fn collect(shape: &egui::Shape, text: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::Shape::Text(t) => text.push((
                    t.galley.job.text.clone(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()),
                )),
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect(s, text);
                    }
                }
                _ => {}
            }
        }
        let out = ctx.run(raw(events), |ctx| {
            app.tool_exit_prompt_ui(ctx);
        });
        let mut text = Vec::new();
        for shape in out.shapes {
            collect(&shape.shape, &mut text);
        }
        text
    }

    fn click(ctx: &egui::Context, app: &mut S33DApp, label: &str) {
        let text = frame(ctx, app, vec![]);
        let pos = text
            .iter()
            .find(|(t, _)| t == label)
            .unwrap_or_else(|| panic!("Missing {label}: {text:?}"))
            .1
            .center();
        frame(ctx, app, pointer(pos, true));
        frame(ctx, app, pointer(pos, false));
    }

    #[test]
    fn confirmation_modal_buttons_and_checkbox_work_in_both_languages() {
        for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            for confirm in [false, true] {
                let ctx = egui::Context::default();
                ctx.set_visuals(egui::Visuals::dark());
                let mut app = open_prompt(&ctx, language);
                let text = frame(&ctx, &mut app, vec![]);
                let message = text
                    .iter()
                    .find(|(t, _)| {
                        t.contains(language.text("current state will be kept", "当前状态将会保留"))
                    })
                    .unwrap();
                assert!(message
                    .0
                    .contains(language.text("Sketch Plane", "草图平面选择")));
                assert!(message.1.width() <= 430., "Text wraps within the dialog");
                click(
                    &ctx,
                    &mut app,
                    language.text("Don't show this again", "不再显示"),
                );
                assert!(app.tool_exit_prompt.as_ref().unwrap().dont_show_again);
                click(
                    &ctx,
                    &mut app,
                    if confirm {
                        language.text("OK", "确定")
                    } else {
                        language.text("Cancel", "取消")
                    },
                );
                assert!(!app.tool_exit_prompt_open());
                assert_eq!(app.mode, if confirm { Mode::Idle } else { Mode::PickPlane });
                assert_eq!(app.settings.show_tool_exit_confirmation, !confirm);
            }
        }
    }

    #[test]
    fn confirmation_modal_escape_cancels_and_enter_confirms() {
        for key in [egui::Key::Escape, egui::Key::Enter] {
            let ctx = egui::Context::default();
            let mut app = open_prompt(&ctx, UiLanguage::English);
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            assert!(!app.tool_exit_prompt_open());
            assert_eq!(
                app.mode,
                if key == egui::Key::Enter {
                    Mode::Idle
                } else {
                    Mode::PickPlane
                }
            );
            assert!(
                !ctx.input(|i| i.key_pressed(key)),
                "The underlying tool must not receive this key"
            );
            assert!(app.settings.show_tool_exit_confirmation);
        }
    }
}

pub(super) struct ToolExitPrompt {
    pub click: WorkspaceExitClick,
    pub tool: (&'static str, &'static str),
    pub dont_show_again: bool,
}

impl S33DApp {
    pub(crate) fn tool_exit_prompt_open(&self) -> bool {
        self.tool_exit_prompt.is_some()
    }

    pub(super) fn exit_tool_name(&self) -> Option<(&'static str, &'static str)> {
        Some(match self.mode {
            Mode::Sketch => ("Sketch", "草图"),
            Mode::Insert3D => ("Draw 3D", "绘制3D"),
            Mode::PickPlane => ("Sketch Plane", "草图平面选择"),
            Mode::Extrude if self.extrude_ctx.as_ref().is_some_and(|c| c.pocket) => {
                ("Pocket", "切除")
            }
            Mode::Extrude => ("Extrude", "拉伸"),
            Mode::Revolve | Mode::RevolveAxis => ("Revolve", "旋转体"),
            Mode::Boolean => match self.boolean_ctx.as_ref().map(|c| c.op) {
                Some(BoolOp::Union) => ("Union", "并集"),
                Some(BoolOp::Subtract) => ("Subtract", "差集"),
                Some(BoolOp::Intersect) => ("Intersect", "交集"),
                None => ("Boolean", "布尔"),
            },
            Mode::Split => ("Split", "切分"),
            Mode::Mirror => ("Mirror", "镜像"),
            Mode::SectionView => ("Section View", "剖切视图"),
            Mode::OffsetShell => ("Offset / Shell", "偏移 / 抽壳"),
            Mode::EquationCurve if self.surface_ctx.is_some() => ("Equation Surface", "方程曲面"),
            Mode::EquationCurve => ("Equation Curve", "方程曲线"),
            Mode::Idle => {
                if self.chamfer_active {
                    if self.chamfer_fillet {
                        ("Fillet", "圆角")
                    } else {
                        ("Chamfer", "倒角")
                    }
                } else if self.array_active {
                    ("Array", "阵列")
                } else if self.align_active {
                    ("Align", "对齐")
                } else {
                    match self.gizmo? {
                        GizmoMode::Translate => ("Move", "移动"),
                        GizmoMode::Rotate => ("Rotate", "旋转"),
                        GizmoMode::Scale => ("Scale", "缩放"),
                    }
                }
            }
        })
    }

    pub(super) fn request_tool_exit_prompt(
        &mut self,
        response: &egui::Response,
        modifiers: egui::Modifiers,
        count: u8,
    ) -> bool {
        if !self.settings.show_tool_exit_confirmation
            || self.confirmed_tool_exit_click
            || self.replaying_tool_exit
        {
            return false;
        }
        let Some(tool) = self.exit_tool_name() else {
            return false;
        };
        self.tool_exit_prompt = Some(ToolExitPrompt {
            click: WorkspaceExitClick {
                response: response.clone(),
                modifiers,
                count,
            },
            tool,
            dont_show_again: false,
        });
        true
    }

    pub(super) fn replay_workspace_exit_click(&mut self, click: WorkspaceExitClick, applied: bool) {
        let old_modifiers = self.workspace_click_modifiers.replace(click.modifiers);
        self.confirmed_tool_exit_click = true;
        self.replaying_tool_exit = applied;
        if let Some(pos) = click.response.interact_pointer_pos() {
            self.last_click = Some((std::time::Instant::now(), pos));
        }
        self.click_count = click.count.saturating_sub(1);
        self.on_left_click(&click.response);
        self.replaying_tool_exit = false;
        self.confirmed_tool_exit_click = false;
        self.workspace_click_modifiers = old_modifiers;
    }

    pub(super) fn resolve_tool_exit_prompt(&mut self, confirm: bool) {
        let Some(prompt) = self.tool_exit_prompt.take() else {
            return;
        };
        if !confirm {
            // Keep the restored tool and its draft. A subsequent gesture
            // starts a fresh sequence and asks again.
            self.last_click = None;
            self.click_count = 0;
            self.click_selection_base = None;
            self.bump_scene();
            return;
        }
        if prompt.dont_show_again {
            self.settings.show_tool_exit_confirmation = false;
            self.save_ui_preferences();
        }
        self.replay_workspace_exit_click(prompt.click, false);
    }

    pub(crate) fn reenable_all_hidden_tips(&mut self) {
        self.settings.show_first_use_tips = true;
        self.settings.show_tool_exit_confirmation = true;
        self.show_grid = true;
        self.bump_scene();
        self.save_ui_preferences();
        self.toast(self.settings.language.text(
            "All hidden tips have been re-enabled",
            "已重新启用所有隐藏提示",
        ));
    }

    pub(crate) fn tool_exit_prompt_ui(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.tool_exit_prompt.as_mut() else {
            return;
        };
        // The third release can arrive after the second release opened the
        // modal. Preserve triple-click intent without activating a dialog
        // button that happens to be beneath the original pointer position.
        let third_click = ctx.input(|i| {
            i.pointer
                .button_triple_clicked(egui::PointerButton::Primary)
                && i.pointer
                    .interact_pos()
                    .zip(prompt.click.response.interact_pointer_pos())
                    .is_some_and(|(a, b)| a.distance(b) < 7.)
        });
        if third_click {
            prompt.click.count = 3;
        }
        let language = self.settings.language;
        let tool = language.text(prompt.tool.0, prompt.tool.1);
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("workspace_tool_exit_confirmation")).show(ctx, |ui| {
            ui.set_max_width(420.);
            ui.heading(language.text("Exit current tool?", "退出当前工具？"));
            ui.add_space(6.);
            ui.label(match language {
                UiLanguage::English => format!("This double/triple-click will exit the {tool} tool. The current state will be kept."),
                UiLanguage::ChineseSimplified => format!("本次双击/三击将退出正在使用的「{tool}」工具，当前状态将会保留。"),
            });
            ui.add_space(8.);
            ui.add_enabled(!third_click, egui::Checkbox::new(&mut prompt.dont_show_again, language.text("Don't show this again", "不再显示")));
            ui.add_space(8.);
            ui.horizontal(|ui| {
                confirm = ui.add_enabled(!third_click, egui::Button::new(language.text("OK", "确定"))).clicked();
                cancel = ui.add_enabled(!third_click, egui::Button::new(language.text("Cancel", "取消"))).clicked();
            });
        });
        if third_click {
            return;
        }
        let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        if cancel || escape {
            self.resolve_tool_exit_prompt(false);
        } else if confirm || enter {
            self.resolve_tool_exit_prompt(true);
        }
    }
}
