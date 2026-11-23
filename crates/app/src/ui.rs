//! egui UI：菜单栏、工具栏、属性面板、状态栏、3D 视口。

use crate::app::{
    AgentIntegration, AppSettings, BoolOp, BooleanOperandSlot, Draw3DPlacementStage, GizmoMode,
    MirrorPlaneSource, S33DApp, UiColorTheme, UiFontChoice, UiFontSize, UiLanguage, UiUnit,
};
use crate::appearance::UiAppearance;
use crate::camera::ViewDir;
use crate::mode::{AlignMode, Mode};
use crate::render3d::ViewportCallback;
use crate::sketch_session::{world_axis_bindings_for_plane, SketchTool};
use crate::ui_icons::ToolIcon;
use eframe::egui;
use s33d_kernel::{DatumPlane, GeometryKernel, PlaneDef};
use s33d_model::{BodyKind, Feature, Primitive3DKind};

const ACCENT: egui::Color32 = egui::Color32::from_rgb(37, 118, 221);
const ACCENT_SOFT: egui::Color32 = egui::Color32::from_rgb(226, 238, 253);
const PANEL: egui::Color32 = egui::Color32::from_rgb(249, 250, 252);
const PANEL_RAISED: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
const VIEWPORT_BG: egui::Color32 = egui::Color32::from_rgb(38, 40, 45);
const BORDER: egui::Color32 = egui::Color32::from_rgb(211, 217, 225);
const TEXT: egui::Color32 = egui::Color32::from_rgb(36, 43, 52);
const MUTED: egui::Color32 = egui::Color32::from_rgb(105, 114, 126);
const USER_MANUAL_EN: &str = include_str!("../../../Docs/UserManual.en.md");
const USER_MANUAL_ZH_CN: &str = include_str!("../../../Docs/UserManual.zh-CN.md");

#[cfg(test)]
#[path = "appearance_qa.rs"]
mod appearance_qa;
#[cfg(test)]
#[path = "display_qa.rs"]
mod display_qa;
#[cfg(test)]
#[path = "tool_input_qa.rs"]
mod tool_input_qa;

fn active_appearance(ctx: &egui::Context) -> UiAppearance {
    ctx.data(|data| {
        data.get_temp(egui::Id::new("ui_appearance"))
            .unwrap_or_default()
    })
}

/// Explicitly colored captions/cards must follow the same palette as widgets.
/// Original returns the legacy color exactly, including its existing accents.
fn appearance_color(appearance: UiAppearance, original: egui::Color32) -> egui::Color32 {
    if appearance.is_original() {
        return original;
    }
    let p = appearance.palette();
    match original {
        TEXT => p.text,
        MUTED => p.muted,
        PANEL => p.panel,
        PANEL_RAISED => p.raised,
        VIEWPORT_BG => p.viewport,
        BORDER => p.border,
        ACCENT => p.accent,
        ACCENT_SOFT => p.accent_soft,
        color
            if color == egui::Color32::from_rgb(242, 246, 251)
                || color == egui::Color32::from_rgb(244, 246, 249) =>
        {
            p.button
        }
        _ => original,
    }
}

fn theme_color(ui: &egui::Ui, original: egui::Color32) -> egui::Color32 {
    appearance_color(active_appearance(ui.ctx()), original)
}

fn theme_error(ui: &egui::Ui, original: egui::Color32) -> egui::Color32 {
    if active_appearance(ui.ctx()).is_original() {
        original
    } else {
        ui.visuals().error_fg_color
    }
}

/// Keep History useful without allowing it to cover the action buttons in a
/// crowded beginner-mode Sketch panel. The history itself remains scrollable.
fn history_panel_height(available: f32, compact_for_sketch: bool) -> f32 {
    if compact_for_sketch {
        (available * 0.14).clamp(72.0, 96.0)
    } else {
        (available / 3.0).max(120.0)
    }
}

/// S33D 轻量桌面设计系统。只改变 egui 呈现，不改变应用架构。
fn apply_design_system(ctx: &egui::Context, settings: &AppSettings) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("ui_appearance"), settings.appearance));
    let mut style = (*ctx.style()).clone();
    let font_family = match settings.font {
        UiFontChoice::Proportional => egui::FontFamily::Proportional,
        UiFontChoice::Monospace => egui::FontFamily::Monospace,
    };
    let font_scale = settings.font_size.scale();
    let accent = settings.color_theme.accent();
    let accent_soft = settings.color_theme.accent_soft();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.interact_size = egui::vec2(42.0, 30.0);
    style.spacing.window_margin = egui::Margin::symmetric(12, 10);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 14.0;
    style.animation_time = 0.12;
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0 * font_scale, font_family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0 * font_scale, font_family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(13.0 * font_scale, font_family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(11.5 * font_scale, font_family),
    );

    let mut visuals = egui::Visuals::light();
    // Let every widget choose its own foreground color. A global override makes
    // selected/primary buttons ignore their white text and caused labels such
    // as "Done" to disappear against their background.
    visuals.override_text_color = None;
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL_RAISED;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, BORDER);
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.menu_corner_radius = egui::CornerRadius::same(9);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: egui::Color32::from_black_alpha(38),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 5],
        blur: 18,
        spread: 0,
        color: egui::Color32::from_black_alpha(32),
    };
    visuals.faint_bg_color = egui::Color32::from_rgb(244, 246, 249);
    visuals.extreme_bg_color = egui::Color32::WHITE;
    visuals.code_bg_color = egui::Color32::from_rgb(239, 242, 246);
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.hyperlink_color = accent;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.inactive.bg_fill = PANEL_RAISED;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(245, 247, 250);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.hovered.bg_fill = accent_soft;
    visuals.widgets.hovered.weak_bg_fill = accent_soft;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.2_f32, accent);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.active.weak_bg_fill = accent;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.2_f32, TEXT);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.open = visuals.widgets.active;
    visuals.striped = true;
    visuals.slider_trailing_fill = true;
    style.visuals = visuals;
    settings.appearance.configure_visuals(&mut style);
    ctx.set_style(style);
}

fn sidebar_frame(appearance: UiAppearance) -> egui::Frame {
    egui::Frame::new()
        .fill(appearance_color(appearance, PANEL))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .stroke(egui::Stroke::new(
            1.0_f32,
            appearance_color(appearance, BORDER),
        ))
}

fn morning_toolbar_style(settings: &AppSettings) -> egui::Style {
    let mut style = egui::Style::default();
    let family = match settings.font {
        UiFontChoice::Proportional => egui::FontFamily::Proportional,
        UiFontChoice::Monospace => egui::FontFamily::Monospace,
    };
    let scale = settings.font_size.scale();
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0 * scale, family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0 * scale, family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(13.0 * scale, family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(11.5 * scale, family),
    );
    style.visuals = egui::Visuals::light();
    let accent = settings.color_theme.accent();
    let accent_soft = settings.color_theme.accent_soft();
    style.visuals.selection.bg_fill = accent;
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    style.visuals.widgets.hovered.bg_fill = accent_soft;
    style.visuals.widgets.hovered.weak_bg_fill = accent_soft;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    style.visuals.widgets.active.bg_fill = accent;
    style.visuals.widgets.active.weak_bg_fill = accent;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    settings.appearance.configure_visuals(&mut style);
    style
}

fn section_title(ui: &mut egui::Ui, title: &str, subtitle: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(15.0)
                .strong()
                .color(theme_color(ui, TEXT)),
        );
        if let Some(subtitle) = subtitle {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(subtitle)
                        .small()
                        .color(theme_color(ui, MUTED)),
                );
            });
        }
    });
}

struct ToolTile {
    label: String,
    icon: ToolIcon,
    selected: bool,
    min_size: egui::Vec2,
}

impl ToolTile {
    fn min_size(mut self, size: egui::Vec2) -> Self {
        self.min_size = size;
        self
    }
}

impl egui::Widget for ToolTile {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        if active_appearance(ui.ctx()).icons_enabled() {
            let size = egui::vec2(ui.available_width().max(self.min_size.x), self.min_size.y);
            crate::ui_icons::icon_button(
                ui,
                &self.label,
                self.icon,
                self.selected,
                ui.is_enabled(),
                size,
            )
        } else {
            ui.add(
                egui::Button::new(self.label)
                    .selected(self.selected)
                    .min_size(self.min_size),
            )
        }
    }
}

fn tool_tile(label: &str, icon: ToolIcon, selected: bool) -> ToolTile {
    ToolTile {
        label: label.to_owned(),
        icon,
        selected,
        min_size: egui::vec2(74.0, 25.0),
    }
}

fn primary_button(label: &str, settings: &AppSettings) -> egui::Button<'static> {
    let accent = settings.accent();
    egui::Button::new(egui::RichText::new(label).strong().color(
        if settings.appearance.is_original() {
            egui::Color32::WHITE
        } else {
            settings.appearance.palette().on_accent
        },
    ))
    .fill(accent)
    .stroke(egui::Stroke::new(1.0_f32, accent))
}

/// Display and parse an internal millimeter value using the active UI unit.
/// Drag speed/range remain in mm so changing units never changes geometry.
struct ToolFocusScope(egui::Context);
impl Drop for ToolFocusScope {
    fn drop(&mut self) {
        self.0
            .data_mut(|d| d.insert_temp(egui::Id::new("numeric_tool_focus_scope"), false));
    }
}

fn arm_tool_focus(ctx: &egui::Context, key: String, active: bool) {
    ctx.data_mut(|data| {
        let id = egui::Id::new("numeric_tool_activation");
        let changed = data.get_temp::<String>(id).as_ref() != Some(&key);
        data.insert_temp(id, key);
        if changed || !active {
            data.insert_temp(egui::Id::new("numeric_tool_focus_pending"), active);
        }
    });
}

pub(crate) fn tool_numeric(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
    let response = ui.add(widget);
    if response.enabled()
        && ui
            .ctx()
            .data(|d| d.get_temp::<bool>(egui::Id::new("numeric_tool_focus_scope")))
            .unwrap_or(false)
        && ui
            .ctx()
            .data(|d| d.get_temp::<bool>(egui::Id::new("numeric_tool_focus_pending")))
            .unwrap_or(false)
    {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("numeric_tool_focus_pending"), false));
        ui.ctx().data_mut(|d| d.remove_temp::<String>(response.id));
        response.request_focus();
        let mut state = egui::TextEdit::load_state(ui.ctx(), response.id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::default(),
                egui::text::CCursor::new(usize::MAX),
            )));
        state.store(ui.ctx(), response.id);
    }
    response
}

pub(crate) fn length_drag_value<'a, Num: egui::emath::Numeric>(
    value_mm: &'a mut Num,
    unit: UiUnit,
) -> egui::DragValue<'a> {
    let decimals = unit.decimals();
    egui::DragValue::new(value_mm)
        .custom_formatter(move |millimeters, _| {
            format!("{:.*}", decimals, unit.from_mm(millimeters))
        })
        .custom_parser(move |text| {
            text.trim()
                .parse::<f64>()
                .ok()
                .map(|value| unit.to_mm(value))
        })
        .suffix(format!(" {}", unit.symbol()))
}

pub fn draw(app: &mut S33DApp, ctx: &egui::Context) {
    app.poll_fastener_job();
    if app.drawing.projection_pending() {
        crate::drawing2d::poll(&mut app.drawing, ctx);
    }
    if app.fastener_job.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
    apply_design_system(ctx, &app.settings);
    // Establish the modal input layer before drawing the controls underneath.
    app.step_import_modal(ctx);
    app.exact_face_modal(ctx);
    app.tool_exit_prompt_ui(ctx);
    app.model_progress_modal(ctx);
    app.export_progress_modal(ctx);
    app.fastener_progress_modal(ctx);
    app.offset_progress_modal(ctx);
    app.surface_progress_modal(ctx);
    app.drawing_progress_modal(ctx);
    // 保存待处理的 debug 截图
    let mut shot: Option<std::sync::Arc<egui::ColorImage>> = None;
    ctx.input(|i| {
        for ev in &i.events {
            if let egui::Event::Screenshot { image, .. } = ev {
                shot = Some(image.clone());
            }
        }
    });
    if let (Some(ts), Some(img)) = (app.pending_debug_ts.take(), shot) {
        let size = img.size;
        let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        if let Some(rgba) = image::RgbaImage::from_raw(size[0] as u32, size[1] as u32, raw) {
            let path = format!("debug/debug-{ts}.png");
            if rgba.save(&path).is_ok() {
                app.toast(format!("Screenshot saved to {path}"));
            } else {
                app.toast("Screenshot save failed");
            }
        }
    }

    // 顶部菜单恢复为原来的紧凑布局；工作区美学与菜单栏相互独立。
    let morning_menubar_style = morning_toolbar_style(&app.settings);
    egui::TopBottomPanel::top("menubar")
        .frame(egui::Frame::side_top_panel(&morning_menubar_style))
        .show(ctx, |ui| {
            ui.set_style(morning_menubar_style);
            if app.progress_busy() {
                ui.disable();
            }
            egui::menu::bar(ui, |ui| {
                file_menu(app, ui);
                edit_menu(app, ui);
                view_menu(app, ui);
                settings_menu(app, ui);
                plugins_menu(app, ui);
                help_menu(app, ui);
                ui.separator();
                ui.label(if app.project_name == "Untitled Project" {
                    app.settings.language.text("Untitled Project", "未命名项目")
                } else {
                    &app.project_name
                });
                if app.project_has_changes() {
                    ui.label("*");
                }
            });
        });

    plugin_toolbars(app, ctx);

    // 状态栏
    egui::TopBottomPanel::bottom("statusbar")
        .exact_height(34.0)
        .frame(
            egui::Frame::new()
                .fill(appearance_color(app.settings.appearance, PANEL_RAISED))
                .inner_margin(egui::Margin::symmetric(12, 5))
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    appearance_color(app.settings.appearance, BORDER),
                )),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("●")
                        .size(9.0)
                        .color(theme_color(ui, ACCENT)),
                );
                ui.label(
                    egui::RichText::new(localized_hint(app.settings.language, &app.hint))
                        .small()
                        .color(theme_color(ui, MUTED)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(
                        &mut app.snap_enabled,
                        app.settings.language.text("Snap", "捕捉"),
                    );
                    let unit = app.settings.unit;
                    let mut step = unit.from_mm(app.snap_step);
                    let r = tool_numeric(
                        ui,
                        egui::DragValue::new(&mut step)
                            .speed((unit.from_mm(app.snap_step).abs() * 0.1).max(0.0001))
                            .max_decimals(unit.decimals())
                            .prefix(app.settings.language.text("Step ", "步长 "))
                            .suffix(format!(" {}", unit.symbol())),
                    );
                    if r.changed() {
                        app.snap_step = unit.to_mm(step).max(0.000_001);
                        app.bump_scene();
                    }
                    ui.label(format!(
                        "{}: {}",
                        app.settings.language.text("Unit", "单位"),
                        unit.symbol()
                    ));
                });
            });
        });

    // 左侧工具栏
    let left_toolbar_style = morning_toolbar_style(&app.settings);
    egui::SidePanel::left("toolbar")
        .resizable(false)
        .exact_width(if app.settings.appearance.is_original() {
            94.0
        } else {
            134.0
        })
        .frame(egui::Frame::side_top_panel(&left_toolbar_style))
        .show(ctx, |ui| {
            ui.set_style(left_toolbar_style);
            if app.progress_busy() {
                ui.disable();
            }
            // Expanded creation/drawing menus must not hide the lower tools.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| toolbar(app, ui));
        });

    // 右侧属性面板：Camera 固定顶部 → 中间内容滚动 → History 固定底部 1/3
    let mut morning_sidebar_style = (*ctx.style()).clone();
    morning_sidebar_style.visuals = morning_toolbar_style(&app.settings).visuals;
    egui::SidePanel::right("properties")
        .resizable(true)
        .default_width(316.0)
        .width_range(286.0..=420.0)
        .frame(sidebar_frame(app.settings.appearance))
        .show(ctx, |ui| {
            // 保留右栏分区，只恢复原生浅色控件与选中高亮。
            ui.set_style(morning_sidebar_style);
            if app.progress_busy() {
                ui.disable();
            }
            // Camera 固定在右边栏最上
            if app.thread_ui.expanded || app.open_thread_panel {
                egui::CollapsingHeader::new(
                    app.settings.language.text("View controls", "视图控制"),
                )
                .id_salt("thread_view_controls")
                .show(ui, |ui| camera_properties(app, ui));
            } else {
                camera_properties(app, ui);
            }
            ui.separator();
            // Sketch + 新手提示通常拥有最长的属性单。此时把 History 压到
            // 底部的小型滚动区，优先让 Apply / Done / Exit Sketch 保持可见。
            let compact_history = (app.mode == Mode::Sketch && app.settings.beginner_mode)
                || app.drawing.open
                || app.thread_ui.expanded
                || app.open_thread_panel;
            let history_height = history_panel_height(ui.available_height(), compact_history);
            egui::TopBottomPanel::bottom("history_panel")
                .resizable(false)
                .exact_height(history_height)
                .show_inside(ui, |ui| history_panel(app, ui));
            // 中间：其余属性面板（可滚动）
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new("properties_height"), ui.available_height())
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| properties(app, ui));
        });

    // 中央 3D 视口
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(appearance_color(app.settings.appearance, VIEWPORT_BG)))
        .show(ctx, |ui| viewport(app, ui));

    // Objects 漂浮窗恢复原来的标题、尺寸和原生浅色样式。
    if app.objects_panel_open {
        let mut open = app.objects_panel_open;
        // 记忆位置；无记忆时放到屏幕下方四分之一处。
        let default_pos = app.objects_panel_pos.unwrap_or_else(|| {
            let sr = ctx.screen_rect();
            egui::pos2(sr.min.x + 105.0, sr.min.y + sr.height() * 0.75)
        });
        let current_style = (*ctx.style()).clone();
        ctx.set_style(morning_toolbar_style(&app.settings));
        let resp = egui::Window::new(
            egui::RichText::new(app.settings.language.text("Objects", "对象")).size(14.0),
        )
        .id(egui::Id::new("objects_floating_window"))
        .default_pos(default_pos)
        .default_width(160.0)
        .min_width(140.0)
        .resizable(true)
        .constrain(true)
        .open(&mut open)
        .show(ctx, |ui| {
            if app.progress_busy() {
                ui.disable();
            }
            objects_list(app, ui);
        });
        ctx.set_style(current_style);
        if !open {
            app.objects_panel_open = false;
        }
        // 记录当前窗口位置（拖动后即记住）
        if let Some(inner) = resp {
            let rect = inner.response.rect;
            if let Some(cur) = app.objects_panel_pos {
                if (cur - rect.min).length() > 0.5 {
                    app.objects_panel_pos = Some(rect.min);
                }
            } else {
                app.objects_panel_pos = Some(rect.min);
            }
        }
    }

    object_color_window(app, ctx);
    first_use_window(app, ctx);
    agent_control_help_window(app, ctx);
    agent_registration_notice_window(app, ctx);
    about_window(app, ctx);
    let gear_selection = app.selected_gear_for_plugin();
    let tire_selection = app.selected_tire_for_plugin();
    let wheel_rim_selection = app.selected_wheel_rim_for_plugin();
    let surface_texture_selection = app.selected_surface_texture_for_plugin();
    let surface_texture_target = app.surface_texture_target_for_plugin();
    app.plugins.sync_selected_gear(gear_selection);
    let fastener_selection = if app.selected.len() == 1 {
        app.body(app.selected[0]).and_then(|b| match b.feature {
            s33d_model::Feature::Fastener { params } => Some((b.id, params)),
            _ => None,
        })
    } else {
        None
    };
    app.plugins.sync_selected_fastener(fastener_selection);
    app.plugins.sync_selected_tire(tire_selection);
    app.plugins.sync_selected_wheel_rim(wheel_rim_selection);
    app.plugins
        .sync_selected_surface_texture(surface_texture_selection);
    app.plugins
        .sync_surface_texture_target(surface_texture_target);
    let plugin_commands = app
        .plugins
        .draw(ctx, app.settings.language, app.settings.unit);
    for command in plugin_commands {
        app.apply_plugin_command(command);
    }

    // Observe committed Sketch entity changes once per frame. This turns
    // drawing, parameter Apply, drag, Move, Rotate, Align and Delete into one
    // coherent local Undo/Redo timeline.
    app.sync_sketch_history();

    // Immutable ShapeIds make reference tracing cheap and deterministic.
    // Running after all UI commands preserves active previews while reclaiming
    // abandoned previews, boolean intermediates and history entries beyond the
    // bounded undo window.
    app.collect_kernel_garbage();

    // toast 通知
    toasts(app, ctx);
}

fn localized_hint(language: UiLanguage, hint: &str) -> String {
    if language == UiLanguage::English {
        return hint.to_owned();
    }
    if hint == SketchTool::Trim.hint() {
        return localized_sketch_tool_hint(language, SketchTool::Trim).to_owned();
    }
    match hint {
        "Select one object to cut" => "请选择一个对象进行剪切".to_owned(),
        "Copied to clipboard - use Paste" => "已复制到剪贴板，可使用粘贴".to_owned(),
        "Clipboard is empty - Copy a body first" => "剪贴板为空，请先复制一个对象".to_owned(),
        "Ready. Press S to start a sketch, double-click a sketch region to select it." => {
            "就绪。按 S 开始草图，双击草图区域进行选择。".to_owned()
        }
        "Sketch kept. Double-click a closed region, then press E for Extrude or P for Pocket." => {
            "草图已保留。双击封闭区域后，按 E 拉伸或按 P 切除。".to_owned()
        }
        "Select a sketch plane: click XY, YZ, ZX, a surface, or enable Linear equation" => {
            "选择草图平面：单击 XY、YZ、ZX 或一个表面，也可启用线性方程。".to_owned()
        }
        "Select an object, face, edge, or sketch" => "请选择对象、面、边或草图。".to_owned(),
        "Ready" => "就绪".to_owned(),
        "Project opened" => "工程已打开".to_owned(),
        "Draw 3D: choose Box, Cylinder, Sphere, Cone, or Torus" => {
            "三维绘制：请选择长方体、圆柱体、球体、圆锥体或圆环。".to_owned()
        }
        "Draw 3D: move the pointer to size the object, then click again" => {
            "三维绘制：移动指针调整大小，然后再次单击确认。".to_owned()
        }
        "Draw 3D: refine parameters in Properties, click Apply, then Done" => {
            "三维绘制：在属性中精调参数，依次单击应用和完成。".to_owned()
        }
        "Draw 3D: drag the preview to move it" => "三维绘制：拖动预览体以移动位置。".to_owned(),
        "Draw 3D: drag the red key point to resize" => "三维绘制：拖动红色关键点以调整大小。".to_owned(),
        "Draw 3D: drag again or refine Properties, then click Apply and Done" => "三维绘制：可以继续拖动或精调属性，然后依次单击应用和完成。".to_owned(),
        "Draw 3D parameters applied. Done keeps drawing open. Esc commits and exits." => {
            "三维绘制参数已应用。点完成后可继续绘制，按 Esc 提交并退出。".to_owned()
        }
        "Draw 3D: choose another object, reselect one to edit, or Exit Draw 3D" => {
            "三维绘制：请选择另一基础体、重新选择对象编辑，或退出三维绘制。".to_owned()
        }
        "Draw 3D complete" => "三维绘制完成".to_owned(),
        "Draw 3D cancelled" => "已取消三维绘制".to_owned(),
        "Choose a sketch plane in the panel or click one in the viewport: XY / YZ / ZX · Esc to cancel" => {
            "请在面板中选择草图平面，或在工作区单击 XY / YZ / ZX · Esc 取消。".to_owned()
        }
        "Choose a sketch plane in the panel or click XY / YZ / ZX" => {
            "请在面板中选择草图平面，或单击 XY / YZ / ZX。".to_owned()
        }
        "Sketch mode: pick entities or draw with L/A/C/R/G - press Done to finish" => {
            "草图模式：选择实体，或用 L/A/C/R/G 绘制；按完成结束。".to_owned()
        }
        "Editing sketch: draw or pick entities, Enter to rebuild" => {
            "正在编辑草图：绘制或选择实体，按 Enter 重建。".to_owned()
        }
        "Fix the sketch profile, then try Done again" => {
            "请修复草图轮廓，然后再次单击完成。".to_owned()
        }
        "Revolve: set angle and confirm (Enter)" => {
            "旋转：设置角度并确认（Enter）。".to_owned()
        }
        "Union complete" => "合并完成".to_owned(),
        "Mirror: click a planar surface to set the mirror plane" => {
            "镜像：单击平面表面设置镜像面。".to_owned()
        }
        "Mirror: choose a default plane or click a planar surface" => {
            "镜像：选择默认平面或单击一个平面表面。".to_owned()
        }
        "Mirror complete" => "镜像完成".to_owned(),
        "Mirror cancelled" => "已取消镜像".to_owned(),
        "Split: click a planar surface to set the cutting plane" => {
            "分割：单击平面表面设置切割面。".to_owned()
        }
        "Split: click a solid object to split" => "分割：单击要分割的实体。".to_owned(),
        "Split: choose a default plane or click a planar surface" => {
            "分割：选择默认平面或单击一个平面表面。".to_owned()
        }
        "Split: select an object, then choose the cutting plane" => {
            "分割：先选择对象，再选择切割面。".to_owned()
        }
        "Split preview failed; adjust the cutting plane" => {
            "分割预览失败；请调整切割面。".to_owned()
        }
        "Split complete" => "分割完成".to_owned(),
        "Split cancelled" => "已取消分割".to_owned(),
        "Section View: cut intersecting objects; keep others whole" => {
            "剖视图：相交对象剖切；其余对象保持完整。".to_owned()
        }
        "Section View: click a planar surface to set the section plane" => {
            "剖视图：单击平面表面设置剖切面。".to_owned()
        }
        "Section View: section plane set from the selected surface" => {
            "剖视图：已用所选表面设置剖切面。".to_owned()
        }
        "Section View: default section plane selected" => {
            "剖视图：已选择默认剖切面。".to_owned()
        }
        "Section View: equation plane applied" => "剖视图：已应用方程平面。".to_owned(),
        "Section View: choose a default plane or click a planar surface" => {
            "剖视图：选择默认平面或单击一个平面表面。".to_owned()
        }
        "Section View exited" => "已退出剖视图".to_owned(),
        "Rotation axis picked; drag or enter an angle, then Apply" => {
            "已选择旋转轴；拖动或输入角度，然后单击应用。".to_owned()
        }
        "Sketch element selected; edit its parameters in Properties" => {
            "已选择草图元素；可在属性中编辑参数。".to_owned()
        }
        "Sketch and dependent bodies updated" => "草图及其依赖实体已更新".to_owned(),
        "Sketch kept on the plane" => "草图已保留在当前平面".to_owned(),
        "Exited empty Sketch" => "已退出空草图".to_owned(),
        "Body created" => "实体已创建".to_owned(),
        "Body created by revolve" => "旋转实体已创建".to_owned(),
        "Cannot finish the current tool: check its parameters and required selections" => "无法完成当前工具：请检查参数及所需选择。".to_owned(),
        "Set a valid depth and target before exiting" => "退出前请设置有效深度并选择目标对象。".to_owned(),
        "Choose a rotation axis before exiting" => "退出前请先选择旋转轴。".to_owned(),
        "Set a valid rotation angle and axis before exiting" => "退出前请设置有效的旋转角度和轴线。".to_owned(),
        "Finish the current operation before switching tools" => "请先完成当前操作，再切换工具。".to_owned(),
        "Deleted" => "已删除".to_owned(),
        "Duplicated" => "已复制".to_owned(),
        "Undo" => "已撤销".to_owned(),
        "Redo" => "已重做".to_owned(),
        _ => hint.to_owned(),
    }
}

// ================= 菜单 =================

fn file_menu(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let title = language.text("File", "文件");
    ui.menu_button(title, |ui| {
        if ui
            .button(language.text("New Project", "新建项目"))
            .clicked()
        {
            app.request_project_action(crate::app::project_session::ProjectAction::New, ui.ctx());
            ui.close_menu();
        }
        if ui
            .button(language.text("Save Project…", "保存项目…"))
            .clicked()
        {
            save_project(app, false);
            ui.close_menu();
        }
        if ui
            .button(language.text("Save Project As…", "项目另存为…"))
            .clicked()
        {
            save_project(app, true);
            ui.close_menu();
        }
        if ui
            .button(language.text("Open Project…", "打开项目…"))
            .clicked()
        {
            open_project(app, ui.ctx());
            ui.close_menu();
        }
        ui.menu_button(language.text("Open Recent", "最近打开"), |ui| {
            let paths = app.settings.recent_projects.clone();
            if paths.is_empty() {
                ui.weak(language.text("No recent projects", "暂无最近工程"));
            }
            for path in paths {
                if ui
                    .button(crate::recent_projects::name(&path))
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    app.request_project_action(
                        crate::app::project_session::ProjectAction::Open(path),
                        ui.ctx(),
                    );
                    ui.close_menu();
                }
            }
        });
        if ui
            .button(language.text("Close Project", "关闭项目"))
            .clicked()
        {
            app.request_project_action(crate::app::project_session::ProjectAction::Close, ui.ctx());
            ui.close_menu();
        }
        ui.separator();
        if ui
            .button(language.text("Import Model…", "导入模型…"))
            .clicked()
        {
            import_model(app);
            ui.close_menu();
        }
        ui.separator();
        if ui.button(language.text("Export…", "导出…")).clicked() {
            export_dialog(app);
            ui.close_menu();
        }
        ui.separator();
        if ui.button(language.text("Exit", "退出")).clicked() {
            app.request_project_action(crate::app::project_session::ProjectAction::Exit, ui.ctx());
        }
    });
}

fn edit_menu(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.menu_button(language.text("Edit", "编辑"), |ui| {
        let undo = ui.add_enabled(
            if app.drawing.open {
                app.drawing.can_undo()
            } else {
                app.can_undo()
            },
            egui::Button::new(language.text("Undo  Ctrl+Z", "撤销  Ctrl+Z")),
        );
        if undo.clicked() {
            if app.drawing.open {
                app.drawing.undo();
            } else {
                app.undo();
            }
            ui.close_menu();
        }
        let redo = ui.add_enabled(
            if app.drawing.open {
                app.drawing.can_redo()
            } else {
                app.can_redo()
            },
            egui::Button::new(language.text("Redo  Ctrl+R", "重做  Ctrl+R")),
        );
        if redo.clicked() {
            if app.drawing.open {
                app.drawing.redo();
            } else {
                app.redo();
            }
            ui.close_menu();
        }
        ui.separator();
        let object = app.mode == Mode::Idle
            && app.selected.len() == 1
            && app.body(app.selected[0]).and_then(|b| b.shape).is_some();
        if ui
            .add_enabled(
                object,
                egui::Button::new(language.text("Copy  Ctrl+C", "复制  Ctrl+C")),
            )
            .clicked()
        {
            app.copy_selected();
            ui.close_menu();
        }
        if ui
            .add_enabled(
                object,
                egui::Button::new(language.text("Cut  Ctrl+X", "剪切  Ctrl+X")),
            )
            .clicked()
        {
            app.cut_selected();
            ui.close_menu();
        }
        if ui
            .add_enabled(
                app.mode == Mode::Idle && app.clipboard.is_some(),
                egui::Button::new(language.text("Paste  Ctrl+V", "粘贴  Ctrl+V")),
            )
            .clicked()
        {
            app.paste_clipboard();
            ui.close_menu();
        }
        ui.separator();
        let del = ui.add_enabled(
            !app.selected.is_empty() || app.mode == Mode::Sketch,
            egui::Button::new(language.text("Delete  Del", "删除  Del")),
        );
        if del.clicked() {
            app.delete_selected();
            ui.close_menu();
        }
        ui.separator();
        if ui
            .add_enabled(
                app.can_group_selection(),
                egui::Button::new(language.text("Group", "编组")),
            )
            .clicked()
        {
            app.group_selected();
            ui.close_menu();
        }
        if ui
            .add_enabled(
                app.can_ungroup_selection(),
                egui::Button::new(language.text("Ungroup", "取消编组")),
            )
            .clicked()
        {
            app.ungroup_selected();
            ui.close_menu();
        }
    });
}

fn view_menu(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let title = language.text("View", "视图");
    ui.menu_button(title, |ui| {
        if ui
            .button(language.text("Top  (7)", "顶视图  (7)"))
            .clicked()
        {
            app.set_view(ViewDir::Top);
            ui.close_menu();
        }
        if ui
            .button(language.text("Front  (1)", "前视图  (1)"))
            .clicked()
        {
            app.set_view(ViewDir::Front);
            ui.close_menu();
        }
        if ui
            .button(language.text("Right  (3)", "右视图  (3)"))
            .clicked()
        {
            app.set_view(ViewDir::Right);
            ui.close_menu();
        }
        if ui
            .button(language.text("Isometric  (0)", "等轴测  (0)"))
            .clicked()
        {
            app.set_view(ViewDir::Iso);
            ui.close_menu();
        }
        ui.separator();
        if ui
            .button(language.text("Toggle Perspective/Ortho  (5)", "切换透视/正交  (5)"))
            .clicked()
        {
            app.toggle_projection();
            ui.close_menu();
        }
        if ui
            .button(language.text("Fit View  (F)", "适合视图  (F)"))
            .clicked()
        {
            app.fit_view();
            ui.close_menu();
        }
        ui.separator();
        let mut show_grid = app.show_grid;
        let r = ui.checkbox(
            &mut show_grid,
            language.text("Show grid and RGB axes", "显示网格和 RGB 辅助轴"),
        );
        if r.changed() {
            app.show_grid = show_grid;
            app.bump_scene();
        }
    });
    if ui.input(|i| i.key_pressed(egui::Key::F)) {
        app.fit_view();
    }
}

fn help_menu(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.menu_button(language.text("Help", "帮助"), |ui| {
        ui.set_min_width(390.0);
        ui.label(
            egui::RichText::new(language.text("Search Help", "搜索帮助"))
                .small()
                .strong(),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.help_search)
                    .desired_width(300.0)
                    .hint_text(language.text("Tool, action, or shortcut…", "工具、操作或快捷键…")),
            );
            if ui
                .add_enabled(
                    !app.help_search.is_empty(),
                    egui::Button::new(language.text("Clear", "清除")),
                )
                .clicked()
            {
                app.help_search.clear();
            }
        });

        let query = app.help_search.trim();
        if !query.is_empty() {
            ui.separator();
            let matches = matching_help_sections(language, query);
            if matches.is_empty() {
                ui.label(
                    egui::RichText::new(
                        language.text("No matching help topic.", "没有匹配的帮助主题。"),
                    )
                    .weak(),
                );
            } else {
                ui.label(
                    egui::RichText::new(language.text("Results", "搜索结果"))
                        .small()
                        .weak(),
                );
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (index, section) in matches.iter().copied().take(12) {
                            help_topic_menu(ui, index, section);
                        }
                        if matches.len() > 12 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "+{} {}",
                                    matches.len() - 12,
                                    language.text("more results", "个更多结果")
                                ))
                                .small()
                                .weak(),
                            );
                        }
                    });
            }
        }

        ui.separator();
        ui.menu_button(language.text("User Guide", "使用指南"), |ui| {
            ui.set_min_width(360.0);
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, section) in manual_sections(language).skip(1) {
                        help_topic_menu(ui, index, section);
                    }
                });
        });
        ui.menu_button(language.text("Agent Control", "Agent 控制"), |ui| {
            ui.set_min_width(430.0);
            let agent_title = language.text("Agent Control", "Agent 控制");
            if let Some((index, section)) = manual_sections(language)
                .find(|(_, section)| help_section_title(*section).contains(agent_title))
            {
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| render_help_section(ui, index, section));
            }
        });
        ui.menu_button(
            language.text("Keyboard Shortcuts", "键盘快捷键"),
            |ui| {
                ui.set_min_width(390.0);
                let shortcut_title = language.text("Keyboard Shortcuts", "常用快捷键");
                if let Some((index, section)) = manual_sections(language)
                    .find(|(_, section)| help_section_title(*section).contains(shortcut_title))
                {
                    render_help_section(ui, index, section);
                }
            },
        );
        if ui
            .button(language.text("About S33D", "关于 S33D"))
            .clicked()
        {
            app.about_open = true;
            ui.close_menu();
        }
    });
}

fn help_manual(language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::English => USER_MANUAL_EN,
        UiLanguage::ChineseSimplified => USER_MANUAL_ZH_CN,
    }
}

fn manual_sections(language: UiLanguage) -> impl Iterator<Item = (usize, &'static str)> {
    help_manual(language).split("\n## ").enumerate()
}

fn help_section_title(section: &str) -> &str {
    section
        .lines()
        .next()
        .unwrap_or("Help")
        .trim()
        .trim_start_matches("# ")
}

fn matching_help_sections(language: UiLanguage, query: &str) -> Vec<(usize, &'static str)> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    manual_sections(language)
        .filter(|(_, section)| section.to_lowercase().contains(&query))
        .collect()
}

fn help_topic_menu(ui: &mut egui::Ui, index: usize, section: &'static str) {
    ui.menu_button(help_section_title(section), |ui| {
        ui.set_min_width(410.0);
        egui::ScrollArea::vertical()
            .max_height(330.0)
            .auto_shrink([false, true])
            .show(ui, |ui| render_help_section(ui, index, section));
    });
}

fn render_help_section(ui: &mut egui::Ui, index: usize, section: &str) {
    let mut in_code_block = false;
    for (line_index, line) in section.lines().enumerate() {
        let line = line.trim();
        if line.starts_with("```") {
            in_code_block = !in_code_block;
        } else if line.is_empty() {
            ui.add_space(3.0);
        } else if line_index == 0 {
            ui.label(egui::RichText::new(help_section_title(section)).strong());
        } else if in_code_block {
            ui.add(
                egui::Label::new(egui::RichText::new(line).monospace().small())
                    .wrap()
                    .selectable(true),
            );
        } else if let Some(heading) = line.strip_prefix("### ") {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(heading).strong());
        } else if let Some(item) = line.strip_prefix("- ") {
            ui.label(format!("• {item}"));
        } else if !(index == 0 && line.starts_with("# ")) {
            ui.label(line);
        }
    }
}

fn hidden_tips_settings(app: &mut S33DApp, ui: &mut egui::Ui) {
    if ui
        .button(
            app.settings
                .language
                .text("Re-enable all hidden tips", "重新启用所有隐藏提示"),
        )
        .clicked()
    {
        app.reenable_all_hidden_tips();
        ui.close_menu();
    }
}

fn settings_menu(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.menu_button(language.text("Settings", "设置"), |ui| {
        ui.menu_button(language.text("Appearance", "界面外观"), |ui| {
            for appearance in UiAppearance::ALL {
                let name = language.text(appearance.label_en(), appearance.label_zh());
                if ui.selectable_label(app.settings.appearance == appearance, name).clicked() {
                    app.set_ui_appearance(appearance);
                    app.save_ui_preferences();
                    ui.ctx().request_repaint();
                    ui.close_menu();
                }
            }
        });
        ui.menu_button(language.text("Language", "语言"), |ui| {
            if ui
                .selectable_value(&mut app.settings.language, UiLanguage::English, "English")
                .clicked()
            {
                app.save_ui_preferences();
                ui.close_menu();
            }
            if ui
                .selectable_value(
                    &mut app.settings.language,
                    UiLanguage::ChineseSimplified,
                    "简体中文",
                )
                .clicked()
            {
                app.save_ui_preferences();
                ui.close_menu();
            }
        });

        ui.menu_button(language.text("Font", "字体"), |ui| {
            if ui
                .selectable_value(
                    &mut app.settings.font,
                    UiFontChoice::Proportional,
                    language.text("UI font", "界面字体"),
                )
                .clicked()
            {
                app.save_ui_preferences();
                ui.close_menu();
            }
            if ui
                .selectable_value(
                    &mut app.settings.font,
                    UiFontChoice::Monospace,
                    language.text("Monospace", "等宽字体"),
                )
                .clicked()
            {
                app.save_ui_preferences();
                ui.close_menu();
            }
        });

        ui.menu_button(language.text("Font Size", "字号"), |ui| {
            for (value, english, chinese) in [
                (UiFontSize::Compact, "Compact", "紧凑"),
                (UiFontSize::Standard, "Standard", "标准"),
                (UiFontSize::Large, "Large", "大"),
            ] {
                if ui
                    .selectable_value(
                        &mut app.settings.font_size,
                        value,
                        language.text(english, chinese),
                    )
                    .clicked()
                {
                    app.save_ui_preferences();
                    ui.close_menu();
                }
            }
        });

        ui.menu_button(language.text("Accent Color", "强调色"), |ui| {
            if !app.settings.appearance.is_original() {
                ui.label(language.text("Appearance uses its own palette.", "当前外观使用预设配色。"));
                ui.label(language.text("The options below apply to Simple.", "以下选项仅用于简洁外观。"));
            }
            ui.add_enabled_ui(app.settings.appearance.is_original(), |ui| {
            for (value, english, chinese) in [
                (UiColorTheme::Blue, "Blue", "蓝色"),
                (UiColorTheme::Teal, "Teal", "青绿色"),
                (UiColorTheme::Orange, "Orange", "橙色"),
            ] {
                if ui
                    .selectable_value(
                        &mut app.settings.color_theme,
                        value,
                        language.text(english, chinese),
                    )
                    .clicked()
                {
                    app.save_ui_preferences();
                    ui.close_menu();
                }
            }
            });
        });

        ui.menu_button(language.text("Units", "单位"), |ui| {
            ui.label(
                egui::RichText::new(language.text("Metric", "公制"))
                    .small()
                    .weak(),
            );
            for unit in [UiUnit::Millimeter, UiUnit::Centimeter, UiUnit::Meter] {
                let label = format!("{} ({})", unit.name(language), unit.symbol());
                if ui
                    .selectable_label(app.settings.unit == unit, label)
                    .clicked()
                {
                    app.set_ui_unit(unit);
                    ui.close_menu();
                }
            }
            ui.separator();
            ui.label(
                egui::RichText::new(language.text("Imperial", "英制"))
                    .small()
                    .weak(),
            );
            for unit in [UiUnit::Inch, UiUnit::Foot] {
                let label = format!("{} ({})", unit.name(language), unit.symbol());
                if ui
                    .selectable_label(app.settings.unit == unit, label)
                    .clicked()
                {
                    app.set_ui_unit(unit);
                    ui.close_menu();
                }
            }
        });

        ui.separator();
        ui.menu_button(
            language.text("Getting Started", "首次使用提示"),
            |ui| {
                let mut show_at_launch = app.settings.show_first_use_tips;
                if ui
                    .checkbox(
                        &mut show_at_launch,
                        language.text("Show at launch", "启动时显示"),
                    )
                    .changed()
                {
                    app.settings.show_first_use_tips = show_at_launch;
                    app.save_ui_preferences();
                }
                if ui.button(language.text("Show now", "立即显示")).clicked() {
                    app.first_use_help_open = true;
                    ui.close_menu();
                }
            },
        );

        hidden_tips_settings(app, ui);

        ui.menu_button(language.text("Beginner Mode", "新手模式"), |ui| {
            if ui
                .selectable_value(
                    &mut app.settings.beginner_mode,
                    true,
                    language.text("Enabled", "开启"),
                )
                .clicked()
            {
                app.save_ui_preferences();
                ui.close_menu();
            }
            if ui
                .selectable_value(
                    &mut app.settings.beginner_mode,
                    false,
                    language.text("Disabled", "关闭"),
                )
                .clicked()
            {
                app.save_ui_preferences();
                ui.close_menu();
            }
        });

        ui.separator();
        ui.menu_button(
            language.text("Agent Connect", "Agent 连接"),
            |ui| {
                ui.set_min_width(510.0);
                let mut agent_control_enabled = app.settings.agent_control_enabled;
                if ui
                    .checkbox(
                        &mut agent_control_enabled,
                        language.text("Enable local control", "启用本机控制"),
                    )
                    .on_hover_text(language.text(
                        "Allow local Agents and scripts to control this S33D window.",
                        "允许本机 Agent 和脚本控制当前 S33D 窗口。",
                    ))
                    .changed()
                {
                    app.set_agent_control_enabled(agent_control_enabled);
                }
                ui.horizontal(|ui| {
                    ui.label(language.text("Port", "端口"));
                    tool_numeric(ui,
                        egui::DragValue::new(&mut app.agent_control_port_input)
                            .range(1..=u16::MAX)
                            .speed(1.0),
                    );
                    if ui.button(language.text("Apply Port", "应用端口")).clicked() {
                        app.apply_agent_control_port(app.agent_control_port_input);
                    }
                    if ui
                        .button(language.text("Test Connection", "测试连接"))
                        .clicked()
                    {
                        app.test_agent_control_connection();
                    }
                });
                if app.settings.agent_control_enabled {
                    if let Some(address) = app.agent_server.address() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} {address}",
                                language.text("Listening:", "监听地址：")
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }

                ui.separator();
                ui.label(
                    egui::RichText::new(language.text("Agent registration", "Agent 注册"))
                        .strong(),
                );
                ui.label(language.text("Connect the visual Agent you already use. Registration changes only the S33D entry; reconnect your chat only if your Agent requires it.","连接你正在使用的视觉 Agent。注册只更改 S33D 条目；仅在 Agent 要求时重连对话。"));
                for integration in [AgentIntegration::Codex, AgentIntegration::ClaudeCode] {
                    ui.horizontal(|ui| {
                        ui.set_min_width(490.0);
                        ui.label(integration.display_name());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .button(language.text("Unregister", "取消注册"))
                                    .on_hover_text(language.text(
                                        "Remove only S33D's MCP entry from this Agent configuration.",
                                        "只从该 Agent 配置中移除 S33D 的 MCP 条目。",
                                    ))
                                    .clicked()
                                {
                                    app.unregister_agent_integration(integration);
                                }
                                if ui
                                    .button(language.text("Register / Update", "注册 / 更新"))
                                    .clicked()
                                {
                                    app.register_agent_integration(integration);
                                }
                            },
                        );
                    });
                }
                if ui
                    .button(language.text(
                        "Other Agent Manual...",
                        "其他 Agent 手册...",
                    ))
                    .clicked()
                {
                    app.agent_control_help_open = true;
                    ui.close_menu();
                }
                ui.label(
                    egui::RichText::new(language.text(
                        "Registration follows the port saved by S33D automatically. Configuration access may require your permission; Unregister never deletes S33D.",
                        "注册会自动跟随 S33D 保存的端口。配置访问可能需要你授权；取消注册绝不会删除 S33D。",
                    ))
                    .small()
                    .weak(),
                );
                ui.separator();
                ui.label(egui::RichText::new(language.text("Photo to model", "照片建模")).strong());
                ui.label(language.text("Attach photos and your request in your Agent chat. The Agent understands the photos and uses S33D to model. Import here only if you want project reference storage or another way to share photos.","在 Agent 聊天中上传照片和建模要求，由 Agent 看照片并调用 S33D 建模。这里的导入仅用于随工程保存参考照片，或提供另一种照片传递方式，可选。"));
                if ui.button(language.text("Import reference photos…", "导入参考照片…")).clicked() { app.import_reference_photos_ui(); }
                for asset in &app.references.assets { ui.label(format!("{} ({} × {})",asset.name,asset.width,asset.height)); }
                if ui.button(language.text("Copy modeling prompt", "复制建模提示")).clicked() { ui.ctx().copy_text(app.photo_start_prompt()); }
                ui.small(language.text("Imported photos are embedded when you save the project. PNG/JPEG; appearance estimates are not engineering measurements.","保存工程时将嵌入参考照片。支持 PNG/JPEG；外观估算不等于工程测量。"));
                if let Some(status) = &app.agent_control_status {
                    ui.separator();
                    ui.label(egui::RichText::new(status).small());
                }
            },
        );

        ui.separator();
        ui.checkbox(
            &mut app.debug_enabled,
            language.text("Debug Logging", "调试日志"),
        );
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PluginMenuCopy {
    display_name: &'static str,
    description: &'static str,
    open_label: &'static str,
}

fn plugin_menu_copy(
    language: UiLanguage,
    id: &str,
    fallback_name: &'static str,
    fallback_description: &'static str,
) -> PluginMenuCopy {
    match id {
        "calculator" => PluginMenuCopy {
            display_name: language.text("Scientific Calculator", "科学计算器"),
            description: language.text(
                "Button-based scientific calculator with DEG/RAD, functions and Ans.",
                "带按钮、DEG/RAD、科学函数和 Ans 的科学计算器。",
            ),
            open_label: language.text("Open Calculator", "打开计算器"),
        },
        "gear-generator" => PluginMenuCopy {
            display_name: language.text("Gear Generator", "齿轮生成器"),
            description: language.text(
                "Parametric spur, helical, bevel and internal gears.",
                "按参数生成直齿、斜齿、锥齿和内齿圈。",
            ),
            open_label: language.text("Open Gear Generator", "打开齿轮生成器"),
        },
        "surface-texture" => PluginMenuCopy {
            display_name: language.text("Surface Texture", "表面纹理"),
            description: language.text(
                "Straight, diagonal and diamond knurling for planes and cylinders.",
                "为平面和圆柱生成直纹、斜纹与菱形滚花。",
            ),
            open_label: language.text("Open Surface Texture", "打开表面纹理"),
        },
        "tire-generator" => PluginMenuCopy {
            display_name: language.text("Tire Generator", "轮胎生成器"),
            description: language.text(
                "Dimensioned tires with common road and off-road tread patterns.",
                "按尺寸生成常见公路及越野胎纹轮胎。",
            ),
            open_label: language.text("Open Tire Generator", "打开轮胎生成器"),
        },
        "wheel-rim-generator" => PluginMenuCopy {
            display_name: language.text("Wheel Rim Generator", "轮毂生成器"),
            description: language.text(
                "Dimensioned wheel rims with common spoke and bolt patterns.",
                "按尺寸生成常见辐条及螺栓孔式轮毂。",
            ),
            open_label: language.text("Open Wheel Rim Generator", "打开轮毂生成器"),
        },
        "screw-generator" => PluginMenuCopy {
            display_name: language.text("Screw Generator", "螺钉生成器"),
            description: language.text(
                "Metric and custom screws with printable modeled threads.",
                "生成公制、自定义及适合打印的实体螺纹螺钉。",
            ),
            open_label: language.text("Open Screw Generator", "打开螺钉生成器"),
        },
        "nut-generator" => PluginMenuCopy {
            display_name: language.text("Nut Generator", "螺帽生成器"),
            description: language.text(
                "Hex, square, round and flange nuts with through threads.",
                "生成六角、方形、圆形及法兰螺帽，内螺纹贯通。",
            ),
            open_label: language.text("Open Nut Generator", "打开螺帽生成器"),
        },
        _ => PluginMenuCopy {
            display_name: fallback_name,
            description: fallback_description,
            open_label: language.text("Open", "打开"),
        },
    }
}

fn set_plugin_enabled_preference(settings: &mut AppSettings, id: &str, enabled: bool) -> bool {
    match id {
        "calculator" => settings.calculator_plugin_enabled = enabled,
        "gear-generator" => settings.gear_plugin_enabled = enabled,
        "surface-texture" => settings.surface_texture_plugin_enabled = enabled,
        "tire-generator" => settings.tire_plugin_enabled = enabled,
        "wheel-rim-generator" => settings.wheel_rim_plugin_enabled = enabled,
        "screw-generator" => settings.screw_plugin_enabled = enabled,
        "nut-generator" => settings.nut_plugin_enabled = enabled,
        _ => return false,
    }
    true
}

fn plugin_toolbars(app: &mut S33DApp, ctx: &egui::Context) {
    let enabled: Vec<_> = app
        .plugins
        .descriptors()
        .into_iter()
        .filter(|d| d.enabled)
        .collect();
    if enabled.is_empty() {
        return;
    }
    let l = app.settings.language;
    egui::TopBottomPanel::top("enabled_plugin_toolbars")
        .frame(sidebar_frame(app.settings.appearance).inner_margin(egui::Margin::symmetric(8, 3)))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for d in enabled {
                    let copy = plugin_menu_copy(l, d.id, d.name, d.description);
                    let title = format!("{} {}", if d.open { "▼" } else { "▶" }, copy.display_name);
                    if ui
                        .selectable_label(d.open, title)
                        .on_hover_text(l.text("Expand / collapse tool", "展开 / 收起工具"))
                        .clicked()
                    {
                        app.plugins.toggle(d.id);
                    }
                }
            });
        });
}

fn plugins_menu(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.menu_button(language.text("Plug-ins", "插件"), |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.label(
                egui::RichText::new(language.text("Installed plug-ins", "已安装插件"))
                    .small()
                    .weak(),
            );
            egui::Grid::new("plugin_menu_rows")
                .num_columns(3)
                .spacing(egui::vec2(18.0, 8.0))
                .min_row_height(28.0)
                .show(ui, |ui| {
                    for descriptor in app.plugins.descriptors() {
                        let mut enabled = descriptor.enabled;
                        let copy = plugin_menu_copy(
                            language,
                            descriptor.id,
                            descriptor.name,
                            descriptor.description,
                        );
                        ui.label(
                            egui::RichText::new(copy.display_name)
                                .strong()
                                .color(theme_color(ui, TEXT)),
                        )
                        .on_hover_text(copy.description);
                        let enabled_changed = ui
                            .horizontal(|ui| {
                                let label = ui.label(language.text("Enabled", "启用"));
                                plugin_enabled_switch(ui, &mut enabled)
                                    .labelled_by(label.id)
                                    .on_hover_text(
                                        language.text(
                                            "Enable or disable this plug-in",
                                            "启用或禁用此插件",
                                        ),
                                    )
                                    .changed()
                            })
                            .inner;
                        let open_clicked = ui
                            .small_button(language.text("Open", "打开"))
                            .on_hover_text(copy.open_label)
                            .clicked();
                        ui.end_row();
                        if enabled_changed {
                            app.plugins.set_enabled(descriptor.id, enabled);
                            if set_plugin_enabled_preference(
                                &mut app.settings,
                                descriptor.id,
                                enabled,
                            ) {
                                app.save_ui_preferences();
                            }
                        }
                        if open_clicked {
                            if app.plugins.open(descriptor.id) {
                                if set_plugin_enabled_preference(
                                    &mut app.settings,
                                    descriptor.id,
                                    true,
                                ) {
                                    app.save_ui_preferences();
                                }
                            }
                            ui.close_menu();
                        }
                    }
                });
            if !app.plugins.discovery_notes.is_empty() {
                ui.separator();
                ui.collapsing(
                    language.text("Plug-in discovery warnings", "插件扫描提示"),
                    |ui| {
                        for note in &app.plugins.discovery_notes {
                            ui.label(note);
                        }
                    },
                );
            }
            ui.label(
                egui::RichText::new(language.text(
                    "Scanned at startup · restart after changing Plug-ins",
                    "启动时扫描 · 修改 Plug-ins 后请重启",
                ))
                .small()
                .weak(),
            );
        });
    });
}

/// A semantic checkbox with an unmistakable left/off, right/on thumb.
fn plugin_enabled_switch(ui: &mut egui::Ui, enabled: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(36.0, 20.0), egui::Sense::click());
    if response.clicked() {
        *enabled = !*enabled;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *enabled, "")
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let fill = if *enabled {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().extreme_bg_color
        };
        ui.painter().rect(
            rect,
            10.0,
            fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let t = ui.ctx().animate_bool_responsive(response.id, *enabled);
        let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
        let thumb = if *enabled {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        };
        ui.painter()
            .circle_filled(egui::pos2(x, rect.center().y), 7.0, thumb);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                12.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Outside,
            );
        }
    }
    response
}

#[cfg(test)]
mod plugin_menu_tests {
    use super::{plugin_menu_copy, set_plugin_enabled_preference};
    use crate::app::{AppSettings, UiLanguage};

    const INSTALLED_PLUGIN_IDS: [&str; 7] = [
        "calculator",
        "gear-generator",
        "surface-texture",
        "tire-generator",
        "wheel-rim-generator",
        "screw-generator",
        "nut-generator",
    ];

    #[test]
    fn installed_plugin_rows_have_english_and_chinese_open_copy() {
        for id in INSTALLED_PLUGIN_IDS {
            let english = plugin_menu_copy(UiLanguage::English, id, "Fallback", "Fallback");
            let chinese =
                plugin_menu_copy(UiLanguage::ChineseSimplified, id, "Fallback", "Fallback");
            assert_ne!(english.display_name, "Fallback");
            assert_ne!(chinese.display_name, "Fallback");
            assert!(english.open_label.starts_with("Open "));
            assert!(chinese.open_label.starts_with("打开"));
        }
    }

    #[test]
    fn installed_plugin_toggles_map_to_persisted_preferences() {
        let mut settings = AppSettings::default();
        for id in INSTALLED_PLUGIN_IDS {
            assert!(set_plugin_enabled_preference(&mut settings, id, true));
        }
        assert!(settings.calculator_plugin_enabled);
        assert!(settings.gear_plugin_enabled);
        assert!(settings.surface_texture_plugin_enabled);
        assert!(settings.tire_plugin_enabled);
        assert!(settings.wheel_rim_plugin_enabled);
        assert!(settings.screw_plugin_enabled);
        assert!(settings.nut_plugin_enabled);
        assert!(!set_plugin_enabled_preference(
            &mut settings,
            "not-installed",
            true
        ));
    }
}

fn agent_control_help_window(app: &mut S33DApp, ctx: &egui::Context) {
    if !app.agent_control_help_open {
        return;
    }
    let language = app.settings.language;
    let mut open = app.agent_control_help_open;
    let escape_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    egui::Window::new(language.text("Agent Control Manual", "Agent 控制手册"))
        .id(egui::Id::new("agent_control_manual_window"))
        .default_width(620.0)
        .default_height(520.0)
        .min_width(430.0)
        .min_height(300.0)
        .resizable(true)
        .open(&mut open)
        .show(ctx, |ui| {
            let agent_title = language.text("Agent Control", "Agent 控制");
            if let Some((index, section)) = manual_sections(language)
                .find(|(_, section)| help_section_title(*section).contains(agent_title))
            {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| render_help_section(ui, index, section));
            } else {
                ui.label(language.text(
                    "The Agent Control chapter could not be loaded.",
                    "无法加载 Agent 控制章节。",
                ));
            }
        });
    app.agent_control_help_open = open && !escape_pressed;
}

fn agent_registration_notice_window(app: &mut S33DApp, ctx: &egui::Context) {
    let Some(message) = app.agent_registration_notice.clone() else {
        return;
    };
    let language = app.settings.language;
    let escape_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let mut open = true;
    let mut acknowledged = false;
    egui::Window::new(language.text("Registration Successful", "注册成功"))
        .id(egui::Id::new("agent_registration_success"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_width(420.0)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    language.text("S33D Agent Control is ready.", "S33D Agent 控制已就绪。"),
                )
                .strong()
                .color(app.settings.accent()),
            );
            ui.add_space(6.0);
            ui.label(message);
            ui.add_space(8.0);
            if ui.button(language.text("OK", "确定")).clicked() {
                acknowledged = true;
            }
        });
    if !open || acknowledged || escape_pressed {
        app.agent_registration_notice = None;
    }
}

fn first_use_window(app: &mut S33DApp, ctx: &egui::Context) {
    if !app.first_use_help_open {
        return;
    }
    // Resolve every dismissal path in one place. Reading Esc before the Window
    // is drawn avoids the viewport/tool shortcut handler consuming it first.
    let escape_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let language = app.settings.language;
    let mut open = app.first_use_help_open;
    let mut dont_show_again = !app.settings.show_first_use_tips;
    let mut close_clicked = false;
    let palette = app.settings.appearance.palette();
    let bold = |text: &str| {
        egui::RichText::new(text)
            .family(egui::FontFamily::Name("s33d-onboarding-bold".into()))
            .color(palette.text)
    };
    egui::Window::new(language.text("Getting Started", "首次使用指引"))
        .id(egui::Id::new("first_use_help"))
        // Neutral title bar, matching the selected appearance instead of a
        // large accent slab. The selection takes effect on the next repaint.
        .frame(egui::Frame::window(&ctx.style()).fill(palette.panel)
            .stroke(egui::Stroke::new(1.0_f32, palette.border)))
        .default_width(520.0)
        .default_height((ctx.screen_rect().height() - 48.0).max(0.0))
        .min_height(0.0)
        .max_height((ctx.screen_rect().height() - 48.0).max(0.0))
        .title_bar(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.visuals_mut().override_text_color = Some(palette.text);
            ui.visuals_mut().widgets.open.bg_fill = palette.raised;
            ui.visuals_mut().widgets.open.fg_stroke.color = palette.text;
            ui.horizontal(|ui| {
                ui.heading(bold(language.text("Getting Started", "首次使用指引")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text(language.text("Close", "关闭")).clicked() {
                        close_clicked = true;
                    }
                });
            });
            ui.separator();
            let footer_id = ui.make_persistent_id("first_use_footer_height");
            let measured_footer = ctx.data(|d| d.get_temp::<f32>(footer_id));
            let estimate = ui.spacing().interact_size.y * 2.0 + ui.spacing().item_spacing.y * 4.0 + 8.0;
            let body_height = (ui.available_height() - measured_footer.unwrap_or(estimate)).max(0.0);
            egui::ScrollArea::vertical().id_salt("first_use_body")
                .min_scrolled_height(0.0).max_height(body_height)
                .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(bold(language.text("Appearance", "界面外观")));
                for appearance in UiAppearance::ALL {
                    if ui.selectable_label(app.settings.appearance == appearance,
                        language.text(appearance.label_en(), appearance.label_zh())).clicked() {
                        app.set_ui_appearance(appearance);
                        app.save_ui_preferences();
                        ctx.request_repaint();
                    }
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(bold(language.text("Language", "语言")));
                if ui
                    .selectable_value(&mut app.settings.language, UiLanguage::English, "English")
                    .clicked()
                {
                    app.save_ui_preferences();
                }
                if ui
                    .selectable_value(
                        &mut app.settings.language,
                        UiLanguage::ChineseSimplified,
                        "简体中文",
                    )
                    .clicked()
                {
                    app.save_ui_preferences();
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(bold(language.text("Unit", "单位")));
                if ui
                    .selectable_label(app.settings.unit == UiUnit::Millimeter, "mm")
                    .clicked()
                {
                    app.set_ui_unit(UiUnit::Millimeter);
                }
                if ui
                    .selectable_label(app.settings.unit == UiUnit::Inch, "inch")
                    .clicked()
                {
                    app.set_ui_unit(UiUnit::Inch);
                }
                ui.label(
                    egui::RichText::new(language.text(
                        "More units are available in Settings.",
                        "更多单位可在设置中选择。",
                    ))
                    .small()
                    .color(palette.muted),
                );
            });
            let mut beginner_mode = app.settings.beginner_mode;
            if ui
                .checkbox(
                    &mut beginner_mode,
                    language.text(
                        "Beginner mode (show step-by-step guidance)",
                        "新手模式（显示分步操作指引）",
                    ),
                )
                .changed()
            {
                app.settings.beginner_mode = beginner_mode;
                app.save_ui_preferences();
            }
            ui.separator();
            ui.heading(bold(language.text("Tips", "使用提示")));
            ui.add_space(6.0);
            ui.label(bold(language.text("Select", "选择")));
            ui.label(bold(language.text("In the workspace:", "在工作区：")));
            let selection_tip = |ui: &mut egui::Ui, click: &str, action: &str, target: &str, end: &str| {
                let mut text = egui::text::LayoutJob::default();
                for (part, italics) in [(click, true), (action, false), (target, true), (end, false)] {
                    text.append(part, 0.0, egui::TextFormat {
                        font_id: egui::TextStyle::Body.resolve(ui.style()),
                        color: palette.text,
                        italics,
                        ..Default::default()
                    });
                }
                ui.label(text);
            };
            selection_tip(ui, language.text("Single-Click", "单击"), language.text(": select an ", "：选择"), language.text("edge", "边"), language.text(".", "。"));
            selection_tip(ui, language.text("Double-Click", "双击"), language.text(": select a ", "：选择"), language.text("surface", "面"), language.text(".", "。"));
            selection_tip(ui, language.text("Triple-Click", "三击"), language.text(": select the whole ", "：选择整个"), language.text("object", "物体"), language.text(".", "。"));
            ui.label(egui::RichText::new(language.text(
                "In Draw 3D, single-click a body to select and edit it.",
                "在 Draw 3D（绘制3D）工具内，单击实体即可选择并编辑。",
            )).color(palette.text));
            ui.label(egui::RichText::new(language.text(
                "Double/triple-click asks before applying and finishing the current tool, collapsing its menus and selecting a surface/object. Point construction keeps its clicks.",
                "双击/三击先提示确认，再应用并完成当前工具、收起菜单及选面/物体；逐点构造保留原操作。",
            )).small().color(palette.muted));
            let mut modifiers = egui::text::LayoutJob::default();
            for (part, italics) in [
                (language.text("Hold ", "按住 "), false),
                ("Ctrl", true),
                (language.text(" or ", " 或 "), false),
                ("Shift", true),
                (language.text(" to select more.", " 可多选。"), false),
            ] {
                modifiers.append(part, 0.0, egui::TextFormat {
                    font_id: egui::TextStyle::Body.resolve(ui.style()),
                    color: palette.text,
                    italics,
                    ..Default::default()
                });
            }
            ui.label(modifiers);
            ui.add_space(4.0);
            ui.label(bold(language.text("Or in Object List:", "或者在对象列表中：")));
            selection_tip(ui, language.text("Single-Click", "单击"), language.text(" to select an ", "选择一个"), language.text("object", "物体"), language.text(".", "。"));
            ui.add_space(10.0);
            ui.label(bold(language.text("Use a tool", "使用工具")));
            for (step, description) in [
                (language.text("Select", "选择"), language.text("a tool on the left, then adjust parameters on the right.", "左侧工具，再在右侧调整参数。")),
                (language.text("Apply", "应用"), language.text("to preview or apply the current parameters.", "预览或应用当前参数。")),
                (language.text("Done", "完成"), language.text("to commit the operation.", "提交本次操作。")),
                (language.text("Exit", "退出"), language.text("the current tool or Sketch.", "当前工具或草图。")),
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(bold(step));
                    let description = ui.label(description);
                    if step == language.text("Apply", "应用") {
                        description.on_hover_text(language.text(
                            "Long geometry calculations and exports show progress and elapsed time. Cancel or Esc preserves the original model and export destination.",
                            "耗时建模和导出会显示进度及已用时间。点击取消或按 Esc 保留原模型及原导出文件。",
                        ));
                    }
                });
            }
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(language.text(
                    "Shortcuts: Ctrl+Z Undo · Ctrl+R Redo · Ctrl+A Select All · Del Delete · Esc Cancel",
                    "快捷键：Ctrl+Z 撤销 · Ctrl+R 重做 · Ctrl+A 全选 · Del 删除 · Esc 取消",
                ))
                .small()
                .color(palette.muted),
            );
            });
            let footer_top = ui.cursor().top();
            ui.separator();
            ui.checkbox(
                &mut dont_show_again,
                language.text("Don't show this again", "不再显示"),
            );
            if ui
                .button(language.text("Got it", "知道了"))
                .clicked()
            {
                close_clicked = true;
            }
            let footer_height = ui.cursor().top() - footer_top;
            ctx.data_mut(|d| d.insert_temp(footer_id, footer_height));
            if measured_footer.is_none_or(|height| (height - footer_height).abs() > 0.5) {
                ctx.request_discard("Getting Started footer measured");
            }
        });
    app.first_use_help_open = help_window_remains_open(open, close_clicked, escape_pressed);
    let show_at_launch = !dont_show_again;
    if app.settings.show_first_use_tips != show_at_launch {
        app.settings.show_first_use_tips = show_at_launch;
        app.save_ui_preferences();
    }
}

fn help_window_remains_open(window_open: bool, got_it: bool, escape_pressed: bool) -> bool {
    window_open && !got_it && !escape_pressed
}

#[cfg(test)]
mod getting_started_tests {
    use super::{
        format_area, format_volume, help_manual, help_window_remains_open, history_panel_height,
        localized_hint, matching_help_sections, measured_face_area, mesh_surface_area_and_volume,
        triangle_indices_area,
    };
    use crate::app::{FaceHit, UiLanguage, UiUnit};
    use s33d_kernel::MeshData;

    #[test]
    fn every_documented_action_closes_getting_started() {
        assert!(!help_window_remains_open(false, false, false)); // title-bar X
        assert!(!help_window_remains_open(true, true, false)); // Got it
        assert!(!help_window_remains_open(true, false, true)); // Esc
        assert!(help_window_remains_open(true, false, false));
    }

    #[test]
    fn sketch_beginner_layout_keeps_history_below_action_buttons() {
        assert_eq!(history_panel_height(800.0, true), 96.0);
        assert_eq!(history_panel_height(500.0, true), 72.0);
        assert!(history_panel_height(800.0, false) > 250.0);
    }

    #[test]
    fn measurement_helpers_use_active_units() {
        let triangle = MeshData::new(
            vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            vec![0, 1, 2],
        );
        assert!((triangle_indices_area(&triangle, &triangle.indices) - 50.0).abs() < 1e-9);
        assert_eq!(format_area(UiUnit::Centimeter, 100.0), "1.0000 cm²");
        assert_eq!(format_volume(UiUnit::Centimeter, 1_000.0), "1.0000 cm³");

        let tetrahedron = MeshData::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
        );
        let (_, volume) = mesh_surface_area_and_volume(&tetrahedron);
        assert!((volume - 1.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn imported_face_area_never_reuses_display_indices_on_the_raw_mesh() {
        // Imported meshes use a compact raw vertex table. Hard-edge welding
        // splits it into a larger display table before face picking.
        let raw = s33d_kernel::MeshData::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
        );
        let display = raw.weld_hard(1e-5, 35.0);
        let triangle = display
            .indices
            .chunks_exact(3)
            .find(|triangle| {
                triangle
                    .iter()
                    .any(|index| *index as usize >= raw.positions.len())
            })
            .expect("hard-edge display mesh should use its own split vertex indices")
            .to_vec();
        let picked_area = triangle_indices_area(&display, &triangle);

        // Properties must use the area captured during picking and must not
        // dereference `triangle` against `raw` (which would panic).
        let face = FaceHit {
            body_id: 41,
            normal: [0.0, 1.0, 0.0],
            center: [0.0, 0.0, 0.0],
            triangles: triangle,
            area_mm2: picked_area,
        };
        assert_eq!(measured_face_area(&face), Some(picked_area));
    }

    #[test]
    fn help_menu_search_matches_tools_shortcuts_and_chinese() {
        let shortcuts = matching_help_sections(UiLanguage::English, "Ctrl+R");
        assert!(shortcuts
            .iter()
            .any(|(_, section)| section.contains("Shortcuts")));
        let boolean = matching_help_sections(UiLanguage::ChineseSimplified, "布尔");
        assert!(boolean
            .iter()
            .any(|(_, section)| section.contains("布尔运算")));
        assert!(matching_help_sections(UiLanguage::English, "布尔").is_empty());
        assert!(
            matching_help_sections(UiLanguage::ChineseSimplified, "Troubleshooting").is_empty()
        );
        assert!(matching_help_sections(UiLanguage::English, "not-a-real-s33d-topic").is_empty());
    }

    #[test]
    fn help_manual_content_is_separated_by_selected_language() {
        let contains_cjk = |text: &str| {
            text.chars()
                .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
        };
        assert!(!contains_cjk(help_manual(UiLanguage::English)));
        assert!(contains_cjk(help_manual(UiLanguage::ChineseSimplified)));
        assert!(help_manual(UiLanguage::English).contains("Keyboard Shortcuts"));
        assert!(help_manual(UiLanguage::ChineseSimplified).contains("常用快捷键"));
        assert!(help_manual(UiLanguage::English).contains("Agent Control"));
        assert!(help_manual(UiLanguage::ChineseSimplified).contains("Agent 控制"));
        assert!(!matching_help_sections(UiLanguage::English, "s33d_execute").is_empty());
        assert!(!matching_help_sections(UiLanguage::ChineseSimplified, "s33d_execute").is_empty());
    }

    #[test]
    fn core_runtime_hints_and_notifications_are_localized() {
        for english in [
            "Draw 3D: choose Box, Cylinder, Sphere, Cone, or Torus",
            "Sketch and dependent bodies updated",
            "Section View: cut intersecting objects; keep others whole",
            "Rotation axis picked; drag or enter an angle, then Apply",
        ] {
            assert_ne!(
                localized_hint(UiLanguage::ChineseSimplified, english),
                english
            );
        }
    }
}

fn about_window(app: &mut S33DApp, ctx: &egui::Context) {
    if !app.about_open {
        return;
    }
    let language = app.settings.language;
    let mut open = app.about_open;
    egui::Window::new(language.text("About S33D", "关于 S33D"))
        .id(egui::Id::new("about_s33d"))
        .default_width(390.0)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.heading(crate::identity::PRODUCT_NAME);
            ui.label(format!(
                "{} {}",
                language.text("Version", "版本"),
                crate::identity::VERSION
            ));
            ui.label(format!(
                "{}: {}",
                language.text("Author", "作者"),
                crate::identity::AUTHOR
            ));
            ui.label(format!(
                "{}: {}",
                language.text("Project license", "项目许可证"),
                crate::identity::LICENSE
            ));
            ui.separator();
            ui.label(language.text(
                "Built in Rust with OCCT, truck, Manifold and wgpu. Project geometry remains local to this computer.",
                "使用 Rust、OCCT、truck、Manifold 与 wgpu 构建；项目几何数据保留在本机。",
            ));
            ui.label(language.text(
                "Third-party components retain their own licenses; see the accompanying THIRD_PARTY_LICENSES.md.",
                "第三方组件遵循各自的许可证，详见随附的 THIRD_PARTY_LICENSES.md。",
            ));
            ui.label(language.text(
                "Alpha Preview: experimental software. Save backups of important projects; this is not a production release.",
                "Alpha Preview：实验性预览版本。请备份重要工程；此版本不作为正式生产版本。",
            ));
        });
    app.about_open = open;
}

#[cfg(test)]
mod about_tests {
    use super::*;

    fn collect_text(shape: &egui::Shape, text: &mut String) {
        match shape {
            egui::Shape::Text(shape) => {
                text.push_str(shape.galley.text());
                text.push('\n');
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, text);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn alpha_about_renders_identity_and_license_in_both_languages() {
        for (language, author_label, license_label, warning) in [
            (
                UiLanguage::English,
                "Author: YDZM",
                "Project license: MPL-2.0",
                "experimental software",
            ),
            (
                UiLanguage::ChineseSimplified,
                "作者: YDZM",
                "项目许可证: MPL-2.0",
                "实验性预览版本",
            ),
        ] {
            let ctx = egui::Context::default();
            let creation = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = S33DApp::new(&creation);
            app.about_open = true;
            app.settings.language = language;
            let mut text = String::new();
            // The first frame measures a newly opened egui window.
            for _ in 0..2 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| about_window(&mut app, ctx),
                );
                text.clear();
                for shape in output.shapes {
                    collect_text(&shape.shape, &mut text);
                }
            }
            for expected in [
                crate::identity::PRODUCT_NAME,
                crate::identity::VERSION,
                author_label,
                license_label,
                "THIRD_PARTY_LICENSES.md",
                warning,
            ] {
                assert!(
                    text.contains(expected),
                    "About is missing {expected:?}: {text}"
                );
            }
        }
    }
}

// ================= 工具栏 =================

fn toolbar(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let creating = matches!(
        app.mode,
        Mode::Sketch | Mode::PickPlane | Mode::Insert3D | Mode::EquationCurve
    );
    ui.set_min_width(74.0);
    // ---- 3D 变换操作（最顶部；不选任何操作时默认即 Select 选择模式）----
    let mv = ui
        .add_enabled(
            creating || (app.mode == Mode::Idle && !app.selected.is_empty()),
            tool_tile(
                language.text("Move", "移动"),
                ToolIcon::Move,
                app.gizmo == Some(GizmoMode::Translate),
            ),
        )
        .on_hover_text(language.text(
            "Move selected object(s) (M): drag with the left button",
            "移动选中对象（M）：按住左键拖动",
        ));
    if mv.clicked() {
        app.toggle_gizmo_tool(GizmoMode::Translate);
    }
    let rot = ui
        .add_enabled(
            creating || (app.mode == Mode::Idle && !app.selected.is_empty()),
            tool_tile(
                language.text("Rotate", "旋转"),
                ToolIcon::Rotate,
                app.gizmo == Some(GizmoMode::Rotate),
            ),
        )
        .on_hover_text(language.text(
            "Rotate selected object(s) (T): drag horizontally as a group",
            "旋转选中对象（T）：水平拖动整组对象",
        ));
    if rot.clicked() {
        app.toggle_gizmo_tool(GizmoMode::Rotate);
    }
    let scale = ui
        .add_enabled(
            creating || (app.mode == Mode::Idle && !app.selected.is_empty()),
            tool_tile(
                language.text("Scale", "缩放"),
                ToolIcon::Scale,
                app.gizmo == Some(GizmoMode::Scale),
            ),
        )
        .on_hover_text(language.text(
            "Scale selected object(s) around their shared center",
            "围绕公共中心缩放选中对象",
        ));
    if scale.clicked() {
        app.toggle_gizmo_tool(GizmoMode::Scale);
    }
    // Align（多对象对齐，参考 Tinkercad）
    let align = ui
        .add_enabled(
            creating || app.mode == Mode::Idle,
            tool_tile(
                language.text("Align", "对齐"),
                ToolIcon::Align,
                app.align_active,
            ),
        )
        .on_hover_text(language.text(
            "Open Align, then add two or more bodies before Apply",
            "打开对齐，然后加入至少两个实体再应用",
        ));
    if align.clicked() {
        app.toggle_align_tool();
    }

    ui.separator();
    if ui
        .add(tool_tile(
            language.text("Sketch", "草图"),
            ToolIcon::Sketch,
            app.sketch_tools_open,
        ))
        .on_hover_text(language.text(
            "Expand / collapse Sketch tools (S)",
            "展开 / 收起草图工具（S）",
        ))
        .clicked()
    {
        app.toggle_sketch_menu();
    }

    // ---- 草图子功能（缩进表示隶属 Sketch）----
    let in_sketch = app.mode == Mode::Sketch;
    if app.sketch_tools_open {
        ui.indent("sketch_tools", |ui| {
            let tools = [
                (
                    language.text("Pick", "选择"),
                    language.text(
                        "Pick entity: move, resize or delete (V)",
                        "选择图元：移动、调整大小或删除（V）",
                    ),
                    SketchTool::Pick,
                ),
                (
                    language.text("Move", "移动"),
                    language.text(
                        "Move selected sketch entities in the active plane (M)",
                        "在当前平面中移动选中草图图元（M）",
                    ),
                    SketchTool::Move,
                ),
                (
                    language.text("Rotate", "旋转"),
                    language.text(
                        "Rotate selected sketch entities around their group center (T)",
                        "围绕组中心旋转选中草图图元（T）",
                    ),
                    SketchTool::Rotate,
                ),
                (
                    language.text("Align", "对齐"),
                    language.text(
                        "Align sketch entities along the displayed world axes",
                        "沿显示的世界坐标轴对齐草图图元",
                    ),
                    SketchTool::Align,
                ),
                (
                    language.text("Trim", "修剪"),
                    language.text(
                        "Trim between vertices or intersections",
                        "删除顶点或交点之间的片段",
                    ),
                    SketchTool::Trim,
                ),
                (
                    language.text("Line", "直线"),
                    language.text("Line (L)", "直线（L）"),
                    SketchTool::Line,
                ),
                (
                    language.text("Arc", "圆弧"),
                    language.text("3-Point Arc (A)", "三点圆弧（A）"),
                    SketchTool::Arc,
                ),
                (
                    language.text("Circle", "圆"),
                    language.text("Circle (C)", "圆（C）"),
                    SketchTool::Circle,
                ),
                (
                    language.text("Rect", "矩形"),
                    language.text("Rectangle (R)", "矩形（R）"),
                    SketchTool::Rect,
                ),
                (
                    language.text("Poly", "多边形"),
                    language.text("Polygon (G)", "多边形（G）"),
                    SketchTool::Polygon,
                ),
                (
                    language.text("Pen", "手绘"),
                    language.text("Freehand: drag to draw a stroke", "按住拖动绘制笔画"),
                    SketchTool::Freehand,
                ),
                (
                    language.text("Text", "文字"),
                    language.text("Place editable text outlines", "放置可编辑的文字轮廓"),
                    SketchTool::Text,
                ),
            ];
            for (name, tip, tool) in tools {
                let active =
                    in_sketch && app.sketch.as_ref().map(|s| s.tool == tool).unwrap_or(false);
                let btn = tool_tile(name, sketch_tool_icon(tool), active)
                    .min_size(egui::vec2(56.0, 25.0));
                let r = ui.add_enabled(in_sketch, btn).on_hover_text(tip);
                if r.clicked() {
                    app.set_sketch_tool(tool);
                }
            }
            let done = ui
                .add_enabled(
                    in_sketch,
                    tool_tile(
                        language.text("Exit\nSketch", "退出\n草图"),
                        ToolIcon::Exit,
                        false,
                    )
                    .min_size(egui::vec2(56.0, 25.0)),
                )
                .on_hover_text(language.text(
                    "Exit sketch: keep the lines and faces on the plane",
                    "退出草图：保留平面上的线和面",
                ));
            if done.clicked() {
                app.exit_sketch();
            }
        });
    }
    if ui
        .add(tool_tile(
            language.text("Draw 3D", "绘制3D"),
            ToolIcon::Draw3D,
            app.draw3d_tools_open,
        ))
        .on_hover_text(language.text("Expand / collapse Draw 3D tools", "展开 / 收起绘制3D工具"))
        .clicked()
    {
        app.toggle_draw3d_menu();
    }
    let in_insert_3d = app.mode == Mode::Insert3D;
    if app.draw3d_tools_open {
        ui.indent("insert_3d_tools", |ui| {
            for kind in Primitive3DKind::ALL {
                let active = in_insert_3d
                    && app
                        .insert_3d_ctx
                        .as_ref()
                        .and_then(|ctx| ctx.params)
                        .is_some_and(|params| params.kind == kind);
                let kind_name = primitive_name(language, kind);
                let button = tool_tile(kind_name, primitive_icon(kind), active)
                    .min_size(egui::vec2(56.0, 25.0));
                if ui
                    .add_enabled(in_insert_3d, button)
                    .on_hover_text(if language == UiLanguage::English {
                        format!("Draw a parameterized {}", kind.name())
                    } else {
                        format!("绘制参数化{kind_name}")
                    })
                    .clicked()
                {
                    app.choose_insert_3d_primitive(kind);
                }
            }
            let exit = ui
                .add_enabled(
                    in_insert_3d,
                    tool_tile(
                        language.text("Exit\nDraw 3D", "退出\n绘制3D"),
                        ToolIcon::Exit,
                        false,
                    )
                    .min_size(egui::vec2(56.0, 25.0)),
                )
                .on_hover_text(language.text(
                    "Commit the current preview and exit Draw 3D",
                    "提交当前预览并退出绘制3D",
                ));
            if exit.clicked() {
                app.exit_insert_3d();
            }
        });
    }
    if ui
        .add(tool_tile(
            language.text("3-view Drawing", "三视图工程图"),
            ToolIcon::EngineeringDrawing,
            app.drawing.open,
        ))
        .clicked()
    {
        app.toggle_drawing_menu();
    }
    if app.drawing.open {
        ui.indent("drawing_tools", |ui| {
            if ui.button(language.text("Generate", "生成图纸")).clicked() {
                app.generate_drawing();
            }
            if ui
                .add_enabled(
                    app.drawing.views.len() == 3,
                    egui::Button::new(language.text("Auto dims", "自动标注")),
                )
                .clicked()
            {
                crate::drawing2d::auto_dimensions(&mut app.drawing);
            }
            if ui
                .selectable_label(app.drawing.manual, language.text("Manual dims", "手动标注"))
                .clicked()
            {
                app.drawing.manual = !app.drawing.manual;
                app.drawing.pending.clear();
            }
            crate::drawing2d::export_menu(app, ui, language.text("Export…", "导出…"));
            if ui
                .button(language.text("Exit Drawing", "退出工程图"))
                .clicked()
            {
                app.drawing.open = false;
                app.drawing.pending.clear();
            }
        });
    }
    tool_button(
        app,
        ui,
        language.text("Draw Equation", "方程绘图"),
        language.text("Draw parametric equations", "绘制参数方程"),
        Mode::EquationCurve,
        true,
        |app| app.start_equation_curve(None),
    );
    ui.separator();
    let bounded_surface_selected = app.mode == Mode::Idle
        && app
            .face_pick
            .clone()
            .is_some_and(|face| app.thread_target_for_face(&face).is_none());
    let selected_sketch = app.mode == Mode::Idle
        && app.selected.len() == 1
        && app
            .selected
            .first()
            .and_then(|&id| app.profile_sketch(id))
            .is_some();
    let equation_surface_selected = app.mode == Mode::Idle
        && app.selected.len() == 1
        && app.body(app.selected[0]).is_some_and(|b| {
            b.kind == BodyKind::Surface && matches!(b.feature, Feature::EquationSurface { .. })
        });
    let extrude_enabled = creating
        || (app.mode == Mode::Idle
            && app.selected.len() == 1
            && app
                .selected
                .first()
                .and_then(|&id| app.profile_sketch(id))
                .is_some())
        || bounded_surface_selected;
    let extrude = ui
        .add_enabled(
            extrude_enabled || equation_surface_selected,
            tool_tile(language.text("Extrude", "拉伸"), ToolIcon::Extrude, false),
        )
        .on_hover_text(language.text(
            "Extrude the selected closed Sketch region or bounded surface (E)",
            "拉伸选中的封闭草图区域或有界表面（E）",
        ));
    if extrude.clicked() {
        app.start_extrude();
    }
    // Pocket（拉伸切除）放在 Extrude 下面
    let pocket = ui
        .add_enabled(
            extrude_enabled,
            tool_tile(language.text("Pocket", "切除"), ToolIcon::Pocket, false),
        )
        .on_hover_text(language.text(
            "Pocket: cut from the selected closed Sketch region or bounded surface (P)",
            "切除：从选中的封闭草图区域或有界表面切除（P）",
        ));
    if pocket.clicked() {
        app.start_pocket();
    }
    let revolve_enabled = creating || selected_sketch || bounded_surface_selected;
    let revolve = ui
        .add_enabled(
            revolve_enabled,
            tool_tile(language.text("Revolve", "旋转体"), ToolIcon::Revolve, false),
        )
        .on_hover_text(language.text(
            "Revolve the selected closed Sketch region or bounded surface (O)",
            "旋转选中的封闭草图区域或有界表面（O）",
        ));
    if revolve.clicked() {
        app.start_revolve();
    }

    ui.separator();
    let booleans = [
        (
            language.text("Union", "并集"),
            language.text(
                "Union: merge two or more bodies",
                "并集：合并两个或更多实体",
            ),
            BoolOp::Union,
        ),
        (
            language.text("Subtract", "差集"),
            language.text(
                "Subtract: target body minus tool body",
                "差集：从原物体中减去工具物体",
            ),
            BoolOp::Subtract,
        ),
        (
            language.text("Intersect", "交集"),
            language.text(
                "Intersect: keep the common volume of two bodies",
                "交集：保留两个实体的公共体积",
            ),
            BoolOp::Intersect,
        ),
    ];
    for (name, tip, op) in booleans {
        let active = app.mode == Mode::Boolean
            && app
                .boolean_ctx
                .as_ref()
                .map(|c| c.op == op)
                .unwrap_or(false);
        let icon = match op {
            BoolOp::Union => ToolIcon::Union,
            BoolOp::Subtract => ToolIcon::Subtract,
            BoolOp::Intersect => ToolIcon::Intersect,
        };
        let btn = tool_tile(name, icon, active);
        if ui
            .add_enabled(
                creating || matches!(app.mode, Mode::Idle | Mode::Boolean),
                btn,
            )
            .on_hover_text(tip)
            .clicked()
        {
            app.start_boolean(op);
        }
    }
    // Split：沿切割平面把 object 切成两个或多个独立 objects。
    let split = ui
        .add_enabled(
            creating || matches!(app.mode, Mode::Idle | Mode::Split),
            tool_tile(
                language.text("Split", "分割"),
                ToolIcon::Split,
                app.mode == Mode::Split,
            ),
        )
        .on_hover_text(language.text(
            "Split a solid object with a cutting plane (surface, datum or equation)",
            "使用切割平面（表面、基准面或方程）分割实体",
        ));
    if split.clicked() {
        app.start_split();
    }
    // Section View：所有 visible objects 沿剖面平面剖切显示。
    let section_view = ui
        .add_enabled(
            creating || matches!(app.mode, Mode::Idle | Mode::SectionView),
            tool_tile(
                language.text("Section\nView", "剖切\n视图"),
                ToolIcon::Section,
                app.mode == Mode::SectionView,
            ),
        )
        .on_hover_text(language.text(
            "Section view: cut intersecting objects; keep others whole",
            "剖切视图：相交对象正常剖切，其余保持完整",
        ));
    if section_view.clicked() {
        app.start_section_view();
    }
    let mirror = ui
        .add_enabled(
            (creating || (app.mode == Mode::Idle && !app.selected.is_empty()))
                || app.mode == Mode::Mirror,
            tool_tile(
                language.text("Mirror", "镜像"),
                ToolIcon::Mirror,
                app.mode == Mode::Mirror,
            ),
        )
        .on_hover_text(language.text(
            "Mirror selected objects across a surface",
            "跨平面镜像选中对象",
        ));
    if mirror.clicked() {
        app.start_mirror();
    }

    // Chamfer/Fillet（棱线识别 + CSG 倒角/圆角）
    let chamfer_btn = ui
        .add_enabled(
            creating || app.mode == Mode::Idle,
            tool_tile(
                language.text("Chamfer/\nFillet", "倒角/\n圆角"),
                if app.chamfer_fillet {
                    ToolIcon::Fillet
                } else {
                    ToolIcon::Chamfer
                },
                app.chamfer_active,
            ),
        )
        .on_hover_text(language.text(
            "Chamfer/Fillet: click an edge, set distance/radius, Apply",
            "倒角/圆角：单击一条边，设置距离/半径后应用",
        ));
    if chamfer_btn.clicked() {
        app.toggle_chamfer_tool();
    }
    if ui
        .add_enabled(
            creating || app.mode == Mode::Idle,
            tool_tile(
                language.text("Offset/\nShell", "偏移/\n抽壳"),
                ToolIcon::OffsetShell,
                false,
            )
            .min_size(egui::Vec2::ZERO),
        )
        .clicked()
    {
        app.start_offset_shell();
    }

    ui.separator();
    let dup = ui
        .add_enabled(
            creating || (app.mode == Mode::Idle && !app.selected.is_empty()),
            tool_tile(language.text("Copy", "复制"), ToolIcon::Copy, false),
        )
        .on_hover_text(language.text(
            "Copy one selected body to the clipboard (Ctrl+C)",
            "将选中实体复制到剪贴板（Ctrl+C）",
        ));
    if dup.clicked() && app.finish_creation_for_tool_switch() {
        app.copy_selected();
    }

    let paste = ui
        .add_enabled(
            app.mode == Mode::Idle && app.clipboard.is_some(),
            tool_tile(language.text("Paste", "粘贴"), ToolIcon::Paste, false),
        )
        .on_hover_text(language.text(
            "Paste the clipboard as a new body (Ctrl+V)",
            "将剪贴板内容粘贴为新实体（Ctrl+V）",
        ));
    if paste.clicked() {
        app.paste_clipboard();
    }

    ui.separator();
    let del_enabled = !app.selected.is_empty() || app.mode == Mode::Sketch;
    let del = ui
        .add_enabled(
            del_enabled,
            tool_tile(language.text("Delete", "删除"), ToolIcon::Delete, false),
        )
        .on_hover_text(language.text("Delete selection (Del)", "删除所选内容（Del）"));
    if del.clicked() {
        app.delete_selected();
    }

    // Array（线性/网格阵列复制；多轴多选）
    let array_btn = ui
        .add_enabled(
            creating || (app.mode == Mode::Idle && !app.selected.is_empty()),
            tool_tile(
                language.text("Array", "阵列"),
                ToolIcon::Array,
                app.array_active,
            ),
        )
        .on_hover_text(language.text(
            "Array one selected body along X/Y/Z axes",
            "沿 X/Y/Z 轴阵列选中实体",
        ));
    if array_btn.clicked() {
        app.toggle_array_tool();
    }
    ui.separator();
    if ui
        .add_enabled(
            app.can_group_selection(),
            tool_tile(language.text("Group", "编组"), ToolIcon::Group, false),
        )
        .on_hover_text(language.text("Group selected objects", "将选中对象编为一组"))
        .clicked()
    {
        app.group_selected();
    }
    if ui
        .add_enabled(
            app.can_ungroup_selection(),
            tool_tile(language.text("Ungroup", "解组"), ToolIcon::Ungroup, false),
        )
        .on_hover_text(language.text("Ungroup selected objects", "将选中对象解组"))
        .clicked()
    {
        app.ungroup_selected();
    }
    // ---- Objects 漂浮窗开关（按钮；窗口显示在工作区，默认打开）----
    ui.separator();
    let objs = ui
        .add(tool_tile(
            language.text("Objects", "对象"),
            ToolIcon::Objects,
            app.objects_panel_open,
        ))
        .on_hover_text(language.text(
            "Show/hide the Objects floating window",
            "显示/隐藏对象浮动窗口",
        ));
    if objs.clicked() {
        app.objects_panel_open = !app.objects_panel_open;
    }
}

fn primitive_name(language: UiLanguage, kind: Primitive3DKind) -> &'static str {
    match kind {
        Primitive3DKind::Box => language.text("Box", "长方体"),
        Primitive3DKind::Cylinder => language.text("Cylinder", "圆柱体"),
        Primitive3DKind::Sphere => language.text("Sphere", "球体"),
        Primitive3DKind::Cone => language.text("Cone", "圆锥体"),
        Primitive3DKind::Torus => language.text("Torus", "圆环体"),
    }
}

fn primitive_icon(kind: Primitive3DKind) -> ToolIcon {
    match kind {
        Primitive3DKind::Box => ToolIcon::Box,
        Primitive3DKind::Cylinder => ToolIcon::Cylinder,
        Primitive3DKind::Sphere => ToolIcon::Sphere,
        Primitive3DKind::Cone => ToolIcon::Cone,
        Primitive3DKind::Torus => ToolIcon::Torus,
    }
}

fn sketch_tool_icon(tool: SketchTool) -> ToolIcon {
    match tool {
        SketchTool::Pick => ToolIcon::Pick,
        SketchTool::Move => ToolIcon::Move,
        SketchTool::Rotate => ToolIcon::Rotate,
        SketchTool::Align => ToolIcon::Align,
        SketchTool::Trim => ToolIcon::Trim,
        SketchTool::Line => ToolIcon::Line,
        SketchTool::Arc => ToolIcon::Arc,
        SketchTool::Circle => ToolIcon::Circle,
        SketchTool::Rect => ToolIcon::Rect,
        SketchTool::Polygon => ToolIcon::Poly,
        SketchTool::Freehand => ToolIcon::Pen,
        SketchTool::Text => ToolIcon::Text,
    }
}

fn draw_3d_size_summary(
    language: UiLanguage,
    unit: UiUnit,
    params: &s33d_model::Primitive3DParams,
) -> String {
    match params.kind {
        Primitive3DKind::Box => format!(
            "{} {}   {} {}   {} {}",
            language.text("W", "宽"),
            unit.format_mm(params.width),
            language.text("D", "深"),
            unit.format_mm(params.depth),
            language.text("H", "高"),
            unit.format_mm(params.height)
        ),
        Primitive3DKind::Cylinder => format!(
            "R {}   {} {}",
            unit.format_mm(params.radius),
            language.text("H", "高"),
            unit.format_mm(params.height)
        ),
        Primitive3DKind::Sphere => format!("R {}", unit.format_mm(params.radius)),
        Primitive3DKind::Cone => format!(
            "R {}   {} {}   {} {}",
            unit.format_mm(params.radius),
            language.text("Top", "顶"),
            unit.format_mm(params.top_radius),
            language.text("H", "高"),
            unit.format_mm(params.height)
        ),
        Primitive3DKind::Torus => format!(
            "{} {}   {} {}",
            language.text("Major R", "大半径"),
            unit.format_mm(params.major_radius),
            language.text("Minor R", "小半径"),
            unit.format_mm(params.minor_radius)
        ),
    }
}

/// Objects 漂浮窗内容（点击选中，双击重新编辑）。
fn objects_list(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let active_sketch_id = if app.mode == Mode::Sketch && app.selected.len() == 1 {
        app.doc
            .body(app.selected[0])
            .filter(|body| body.kind == BodyKind::Sketch)
            .map(|body| body.id)
    } else {
        None
    };
    let active_elements: Vec<(usize, String, bool)> = app
        .sketch
        .as_ref()
        .map(|sketch| {
            sketch
                .entities
                .iter()
                .enumerate()
                .map(|(index, entity)| {
                    let kind = match entity.kind() {
                        "line" => language.text("Line", "直线"),
                        "arc" => language.text("Arc", "圆弧"),
                        "circle" => language.text("Circle", "圆"),
                        "rect" => language.text("Rectangle", "矩形"),
                        "polygon" => language.text("Polygon", "多边形"),
                        "freehand" => language.text("Freehand", "手绘"),
                        "text" => language.text("Text", "文字"),
                        other => other,
                    };
                    (
                        index,
                        format!("{kind} {}", index + 1),
                        match sketch.tool {
                            SketchTool::Align => sketch.align_selection.contains(&index),
                            SketchTool::Move | SketchTool::Rotate => {
                                sketch.transform_selection.contains(&index)
                            }
                            _ => sketch.is_entity_selected(index),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let draft_sketch_active = app.mode == Mode::Sketch && active_sketch_id.is_none();
    let mut bodies: Vec<(
        u64,
        String,
        BodyKind,
        bool,
        bool,
        [f32; 4],
        Vec<(usize, String, bool)>,
    )> = app
        .doc
        .bodies
        .iter()
        .map(|b| {
            let sketch_elements = if active_sketch_id == Some(b.id) {
                active_elements.clone()
            } else {
                b.sketch()
                    .map(|sketch| {
                        sketch
                            .entities
                            .iter()
                            .enumerate()
                            .map(|(index, entity)| {
                                let kind = match entity.kind() {
                                    "line" => language.text("Line", "直线"),
                                    "arc" => language.text("Arc", "圆弧"),
                                    "circle" => language.text("Circle", "圆"),
                                    "rect" => language.text("Rectangle", "矩形"),
                                    "polygon" => language.text("Polygon", "多边形"),
                                    "freehand" => language.text("Freehand", "手绘"),
                                    "text" => language.text("Text", "文字"),
                                    other => other,
                                };
                                (index, format!("{kind} {}", index + 1), false)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            (
                b.id,
                b.name.clone(),
                b.kind.clone(),
                app.selected.contains(&b.id),
                b.visible,
                b.material.color,
                sketch_elements,
            )
        })
        .collect();
    if bodies.is_empty() && !draft_sketch_active {
        ui.label(egui::RichText::new(language.text("No objects yet", "暂无对象")).weak());
    } else {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(language.text("Visible", "可见"))
                    .small()
                    .color(theme_color(ui, MUTED)),
            );
            ui.label(
                egui::RichText::new(language.text("Object", "对象"))
                    .small()
                    .color(theme_color(ui, MUTED)),
            );
        });
    }
    let mut select_id = None;
    let mut edit_id = None;
    let mut visibility_change = None;
    let mut delete_id = None;
    let mut select_element: Option<(Option<u64>, usize, bool, bool)> = None;
    let groups = app.doc.groups.clone();
    // Keep members contiguous in the view; Shift-range order below follows
    // this same view ordering rather than accidentally crossing other groups.
    bodies.sort_by_key(|b| {
        groups
            .iter()
            .position(|g| g.members.contains(&b.0))
            .unwrap_or(groups.len())
    });
    let mut drawn_groups = std::collections::HashSet::new();
    let mut collapsed_groups = std::collections::HashSet::new();
    for (id, name, kind, selected, visible, body_color, sketch_elements) in &bodies {
        if let Some(group) = groups.iter().find(|g| g.members.contains(id)) {
            if drawn_groups.insert(group.id) {
                let key = egui::Id::new(("object_group_open", group.id));
                let mut expanded = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(true);
                let selected = group.members.iter().all(|id| app.selected.contains(id));
                ui.horizontal(|ui| {
                    if ui.small_button(if expanded { "▼" } else { "▶" }).clicked() {
                        expanded = !expanded;
                    }
                    let response = ui.selectable_label(
                        selected,
                        format!("{} ({})", group.name, group.members.len()),
                    );
                    if response.clicked() {
                        let additive = ui.input(|i| {
                            i.modifiers.ctrl || i.modifiers.shift || i.modifiers.command
                        }) || app.align_active;
                        app.select_group(group.id, additive);
                    }
                    response.context_menu(|ui| {
                        if ui.button(language.text("Ungroup", "取消编组")).clicked() {
                            app.select_group(group.id, false);
                            app.ungroup_selected();
                            ui.close_menu();
                        }
                        if ui.button(language.text("Show all", "全部显示")).clicked() {
                            visibility_change = Some((group.members.clone(), true));
                            ui.close_menu();
                        }
                        if ui.button(language.text("Hide all", "全部隐藏")).clicked() {
                            visibility_change = Some((group.members.clone(), false));
                            ui.close_menu();
                        }
                        if ui.button(language.text("Color…", "颜色…")).clicked() {
                            app.object_color_editor = Some((group.members.clone(), *body_color));
                            ui.close_menu();
                        }
                        if ui
                            .button(language.text("Delete group and objects", "删除组及对象"))
                            .clicked()
                        {
                            delete_id = Some(group.members.clone());
                            ui.close_menu();
                        }
                    });
                });
                ui.data_mut(|d| d.insert_temp(key, expanded));
                if !expanded {
                    collapsed_groups.insert(group.id);
                }
            }
            if collapsed_groups.contains(&group.id) {
                continue;
            }
        }
        let kind_str = if *kind == BodyKind::Curve {
            language.text("curve", "曲线")
        } else if *kind == BodyKind::Surface {
            language.text("surface", "曲面")
        } else if *kind == BodyKind::Sketch {
            language.text("sketch", "草图")
        } else {
            language.text("solid", "实体")
        };
        let mut shown = *visible;
        let mut resp = None;
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut shown, "")
                .on_hover_text(if *visible {
                    language.text("Hide object", "隐藏对象")
                } else {
                    language.text("Show object", "显示对象")
                })
                .changed()
            {
                visibility_change = Some((vec![*id], shown));
            }
            let prefix = if groups.iter().any(|g| g.members.contains(id)) { "    " } else { "" };
            let label = if *visible {
                egui::RichText::new(format!("{prefix}{name} ({kind_str})"))
            } else {
                egui::RichText::new(format!("{prefix}{name} ({kind_str})")).weak()
            };
            resp = Some(
                ui.selectable_label(*selected, label)
                    .on_hover_text(if *visible {
                        if language == UiLanguage::English {
                            format!(
                                "Object #{id} · Shift-click selects a range · Ctrl-click toggles · double-click to edit"
                            )
                        } else {
                            format!("对象 #{id} · Shift 单击连续选择 · Ctrl 单击追加/取消 · 双击编辑")
                        }
                    } else {
                        if language == UiLanguage::English {
                            format!("Object #{id} is hidden · Shift-click selects a range · Ctrl-click toggles")
                        } else {
                            format!("对象 #{id} 已隐藏 · Shift 单击连续选择 · Ctrl 单击追加/取消")
                        }
                    }),
            );
        });
        let Some(resp) = resp else { continue };
        let targets_key = resp.id.with("objects_context_targets");
        if resp.secondary_clicked() {
            let targets = app.objects_context_targets(*id);
            ui.data_mut(|data| data.insert_temp(targets_key, targets));
        }
        resp.context_menu(|ui| {
            let targets = ui
                .data(|data| data.get_temp::<Vec<u64>>(targets_key))
                .unwrap_or_else(|| app.objects_context_targets(*id));
            let multiple = targets.len() > 1;
            if multiple {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        targets.len(),
                        language.text("objects selected", "个已选对象")
                    ))
                    .strong(),
                );
                if ui
                    .add_enabled(
                        app.can_group_selection(),
                        egui::Button::new(language.text("Group", "编组")),
                    )
                    .clicked()
                {
                    app.group_selected();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        app.can_ungroup_selection(),
                        egui::Button::new(language.text("Ungroup", "取消编组")),
                    )
                    .clicked()
                {
                    app.ungroup_selected();
                    ui.close_menu();
                }
            } else {
                let opening = app.rename_body_id != Some(*id);
                if opening {
                    app.rename_body_id = Some(*id);
                    app.rename_body_text = name.clone();
                }
                ui.label(
                    egui::RichText::new(language.text("Rename object", "重命名对象")).strong(),
                );
                let editor = ui.add(
                    egui::TextEdit::singleline(&mut app.rename_body_text)
                        .desired_width(180.0)
                        .hint_text(language.text("Object name", "对象名称")),
                );
                if opening {
                    editor.request_focus();
                }
                let submit = ui.input(|input| input.key_pressed(egui::Key::Enter));
                let apply = ui.button(language.text("Rename", "重命名")).clicked();
                if submit || apply {
                    let next_name = app.rename_body_text.clone();
                    if app.rename_body(*id, &next_name) {
                        app.rename_body_id = None;
                        ui.close_menu();
                    }
                }
            }
            ui.separator();
            let all_visible = targets
                .iter()
                .all(|&id| app.body(id).is_some_and(|b| b.visible));
            let any_visible = targets
                .iter()
                .any(|&id| app.body(id).is_some_and(|b| b.visible));
            let mut context_visible = all_visible;
            if ui
                .add(
                    egui::Checkbox::new(&mut context_visible, language.text("Visible", "可见"))
                        .indeterminate(any_visible && !all_visible),
                )
                .changed()
            {
                visibility_change = Some((targets.clone(), context_visible));
                ui.close_menu();
            }
            ui.horizontal(|ui| {
                ui.label(language.text("Color", "颜色"));
                let body_color = targets
                    .first()
                    .and_then(|&id| app.body(id))
                    .map(|b| b.material.color)
                    .unwrap_or(*body_color);
                let color = egui::Color32::from_rgba_unmultiplied(
                    (body_color[0] * 255.0) as u8,
                    (body_color[1] * 255.0) as u8,
                    (body_color[2] * 255.0) as u8,
                    (body_color[3] * 255.0) as u8,
                );
                if ui
                    .add(
                        egui::Button::new(language.text("Choose…", "选择…"))
                            .fill(color)
                            .stroke(egui::Stroke::new(1.0_f32, theme_color(ui, BORDER))),
                    )
                    .clicked()
                {
                    app.object_color_editor = Some((targets.clone(), body_color));
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui
                .add_enabled(
                    !multiple || matches!(app.mode, Mode::Idle | Mode::Boolean),
                    egui::Button::new(
                        egui::RichText::new(language.text("Delete", "删除"))
                            .color(theme_error(ui, egui::Color32::from_rgb(190, 48, 48))),
                    ),
                )
                .clicked()
            {
                delete_id = Some(targets);
                ui.close_menu();
            }
        });
        if resp.clicked() {
            let (shift, explicit_additive) = ui.input(|input| {
                (
                    input.modifiers.shift,
                    input.modifiers.ctrl || input.modifiers.command,
                )
            });
            // Align 激活时 Objects 列表本身就是待提交选单：
            // 普通点击即追加/移除，不需要一直按 Ctrl/Shift。
            let additive = explicit_additive || app.align_active;
            select_id = Some((*id, additive, shift));
        }
        if resp.double_clicked() {
            edit_id = Some(*id);
        }
        // Sketch 的元素树持续可见；正在编辑时使用 session 中的实时内容，
        // Done 后实体仍留在对应 Sketch 下。
        if *kind == BodyKind::Sketch {
            ui.indent(("sketch_elements", id), |ui| {
                if sketch_elements.is_empty() {
                    ui.label(
                        egui::RichText::new(language.text("No sketch elements", "暂无草图图元"))
                            .small()
                            .weak(),
                    );
                } else {
                    for (index, label, element_selected) in sketch_elements {
                        let response = ui
                            .selectable_label(*element_selected, egui::RichText::new(label).small())
                            .on_hover_text(if active_sketch_id == Some(*id) {
                                language.text(
                                    "Select this sketch element and edit it in Properties",
                                    "选择此草图图元并在属性中编辑",
                                )
                            } else {
                                language.text(
                                    "Click to reopen this Sketch and edit the element",
                                    "单击以重新打开此草图并编辑图元",
                                )
                            });
                        if response.clicked() {
                            let (additive, shift) = ui.input(|input| {
                                (
                                    input.modifiers.ctrl || input.modifiers.command,
                                    input.modifiers.shift,
                                )
                            });
                            select_element = Some((Some(*id), *index, additive, shift));
                        }
                    }
                }
            });
        }
    }
    // 新 Sketch 在正式 Exit Sketch 前还没有 Document Body；用临时父节点
    // 保持 Objects 树层级完整，避免已 Done 的实体在列表里无处显示。
    if draft_sketch_active {
        ui.horizontal(|ui| {
            let mut visible = true;
            ui.add_enabled(false, egui::Checkbox::new(&mut visible, ""));
            ui.label(
                egui::RichText::new(
                    language.text("Current Sketch (editing)", "当前草图（编辑中）"),
                )
                .strong(),
            );
        });
        ui.indent("draft_sketch_elements", |ui| {
            if active_elements.is_empty() {
                ui.label(
                    egui::RichText::new(language.text("No sketch elements", "暂无草图图元"))
                        .small()
                        .weak(),
                );
            } else {
                for (index, label, element_selected) in &active_elements {
                    let response = ui
                        .selectable_label(*element_selected, egui::RichText::new(label).small())
                        .on_hover_text(language.text(
                            "Select this sketch element and edit it in Properties",
                            "选择此草图图元并在属性中编辑",
                        ));
                    if response.clicked() {
                        let (additive, shift) = ui.input(|input| {
                            (
                                input.modifiers.ctrl || input.modifiers.command,
                                input.modifiers.shift,
                            )
                        });
                        select_element = Some((None, *index, additive, shift));
                    }
                }
            }
        });
    }
    if let Some((ids, visible)) = visibility_change {
        app.set_bodies_visible(&ids, visible);
    }
    if let Some(ids) = delete_id {
        app.delete_bodies_from_objects(&ids);
    }
    if let Some((id, additive, shift)) = select_id {
        if !shift {
            app.object_list_anchor = Some(id);
        }
        if shift
            && (matches!(app.mode, Mode::Idle | Mode::SectionView)
                || (app.mode == Mode::Boolean
                    && app
                        .boolean_ctx
                        .as_ref()
                        .is_some_and(|c| c.op == BoolOp::Union)))
        {
            app.select_object_range(id, additive);
        } else if shift && app.mode != Mode::Sketch {
            app.toast(language.text(
                "Finish the current tool before selecting an object range",
                "请先结束当前工具，再选择对象区间",
            ));
        } else if app.mode == Mode::Boolean {
            if app
                .body(id)
                .is_some_and(|body| body.kind == BodyKind::Solid)
            {
                app.boolean_click(id, additive);
            } else {
                app.toast(language.text(
                    "Boolean operands must be solid objects",
                    "布尔运算对象必须是实体",
                ));
            }
        } else if app.mode == Mode::Split {
            app.split_click(id, additive);
        } else if app.mode == Mode::Mirror {
            app.toast(language.text(
                "Mirror source objects are locked; Cancel to change them",
                "镜像源对象已锁定；取消后可更改",
            ));
        } else if app.mode == Mode::Insert3D {
            app.edit_draw_3d_body(id);
        } else {
            app.select_from_objects(id, additive);
        }
    }
    if let Some((owner, index, additive, shift)) = select_element {
        if owner.is_none() || owner == active_sketch_id {
            app.select_sketch_list_item(index, additive, shift);
        } else if let Some(owner) = owner {
            if app.mode == Mode::Idle {
                app.edit_sketch(owner);
                app.select_sketch_list_item(index, false, false);
            } else {
                app.toast(language.text(
                    "Exit the current operation before editing another Sketch",
                    "请先退出当前操作，再编辑其他草图",
                ));
            }
        }
    }
    if let Some(id) = edit_id {
        if app.mode == Mode::Boolean
            || app.mode == Mode::Split
            || app.mode == Mode::SectionView
            || app.mode == Mode::Mirror
            || app.mode == Mode::Insert3D
            || app.align_active
        {
            // In Boolean mode a double-click is still just an operand pick;
            // Align 中双击也不应跳出工具去编辑 object。
        } else if app
            .body(id)
            .is_some_and(|b| matches!(b.feature, Feature::EquationSurface { .. }))
        {
            app.start_equation_surface(Some(id));
        } else if app.body(id).is_some_and(|b| b.kind == BodyKind::Curve) {
            app.start_equation_curve(Some(id));
        } else if app.body(id).map(|b| b.is_sketch()).unwrap_or(false) {
            app.edit_sketch(id);
        } else {
            app.select(id, false);
            app.toast(language.text(
                "Solid selected - use Extrude/Revolve/Boolean from the toolbar",
                "已选择实体——请从工具栏使用拉伸、旋转体或布尔运算",
            ));
        }
    }
}

fn object_color_window(app: &mut S33DApp, ctx: &egui::Context) {
    let Some((body_ids, mut draft)) = app.object_color_editor.clone() else {
        return;
    };
    let language = app.settings.language;
    let body_name = if body_ids.len() > 1 {
        format!("{} {}", body_ids.len(), language.text("objects", "个对象"))
    } else {
        app.body(*body_ids.first().unwrap_or(&0))
            .map(|body| body.name.clone())
            .unwrap_or_else(|| {
                language
                    .text("Object no longer exists", "对象已不存在")
                    .into()
            })
    };
    let names = body_ids
        .iter()
        .filter_map(|&id| app.body(id).map(|b| b.name.clone()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut open = true;
    let mut apply = false;
    let mut cancel = false;
    egui::Window::new(language.text("Object Color", "对象颜色"))
        .id(egui::Id::new("object_color_editor"))
        .default_width(340.0)
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(body_name).strong());
            if body_ids.len() > 1 {
                ui.label(language.text(
                    "Applies to the captured selection",
                    "应用于打开时选定的对象组",
                ))
                .on_hover_text(&names);
            }
            ui.add_space(4.0);
            let mut color = egui::Color32::from_rgba_unmultiplied(
                (draft[0] * 255.0).round() as u8,
                (draft[1] * 255.0).round() as u8,
                (draft[2] * 255.0).round() as u8,
                (draft[3] * 255.0).round() as u8,
            );
            ui.horizontal(|ui| {
                ui.label(language.text("Custom", "自定义"));
                if ui.color_edit_button_srgba(&mut color).changed() {
                    draft = [
                        color.r() as f32 / 255.0,
                        color.g() as f32 / 255.0,
                        color.b() as f32 / 255.0,
                        color.a() as f32 / 255.0,
                    ];
                }
            });
            ui.label(
                egui::RichText::new(language.text("Quick colors", "快捷颜色"))
                    .small()
                    .weak(),
            );
            ui.horizontal_wrapped(|ui| {
                for preset in [
                    egui::Color32::from_rgb(132, 161, 196),
                    egui::Color32::from_rgb(94, 145, 214),
                    egui::Color32::from_rgb(56, 158, 124),
                    egui::Color32::from_rgb(232, 164, 66),
                    egui::Color32::from_rgb(211, 91, 91),
                    egui::Color32::from_rgb(156, 105, 194),
                    egui::Color32::from_rgb(84, 84, 88),
                    egui::Color32::from_rgb(222, 224, 228),
                ] {
                    if ui
                        .add(
                            egui::Button::new("")
                                .min_size(egui::vec2(28.0, 24.0))
                                .fill(preset),
                        )
                        .clicked()
                    {
                        draft = [
                            preset.r() as f32 / 255.0,
                            preset.g() as f32 / 255.0,
                            preset.b() as f32 / 255.0,
                            1.0,
                        ];
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(primary_button(
                        language.text("Apply Color", "应用颜色"),
                        &app.settings,
                    ))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button(language.text("Cancel", "取消")).clicked() {
                    cancel = true;
                }
            });
        });

    if apply {
        app.object_color_editor = None;
        app.set_bodies_color(&body_ids, draft);
    } else if cancel || !open || !body_ids.iter().any(|&id| app.body(id).is_some()) {
        app.object_color_editor = None;
    } else {
        app.object_color_editor = Some((body_ids, draft));
    }
}

/// 工具栏选项卡按钮：仅显示名称，悬停弹出快捷键提示；
/// `developed=false` 的选项卡文字为灰色（待开发）。
fn tool_button(
    app: &mut S33DApp,
    ui: &mut egui::Ui,
    name: &str,
    tip: &str,
    mode: Mode,
    developed: bool,
    f: impl FnOnce(&mut S33DApp),
) {
    let active = app.mode == mode;
    let icon = match mode {
        Mode::PickPlane => ToolIcon::Sketch,
        Mode::Insert3D => ToolIcon::Draw3D,
        Mode::EquationCurve => ToolIcon::Equation,
        _ => ToolIcon::Pick,
    };
    let btn = tool_tile(name, icon, active).min_size(egui::vec2(
        74.0,
        if developed && mode == Mode::EquationCurve {
            30.0
        } else {
            25.0
        },
    ));
    if ui.add(btn).on_hover_text(tip).clicked() {
        f(app);
    }
}

// ================= 属性面板 =================

/// 草图平面选择（PickPlane 模式）：用清晰的平面卡片与视口中的三维平面对应。
fn sketch_plane_picker(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    section_title(
        ui,
        language.text("Choose Sketch Plane", "选择草图平面"),
        Some(language.text("Sketch", "草图")),
    );
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(language.text(
            "Select a datum plane here or directly in the workspace.",
            "在此处或直接在工作区中选择基准面。",
        ))
        .size(12.0)
        .color(theme_color(ui, MUTED)),
    );
    ui.add_space(8.0);

    let planes = [
        (
            DatumPlane::Front,
            "XY",
            language.text("Front plane", "前基准面"),
            egui::Color32::from_rgb(97, 160, 240),
            egui::Color32::from_rgb(231, 241, 253),
        ),
        (
            DatumPlane::Right,
            "YZ",
            language.text("Right plane", "右基准面"),
            egui::Color32::from_rgb(224, 108, 117),
            egui::Color32::from_rgb(253, 235, 238),
        ),
        (
            DatumPlane::Top,
            "ZX",
            language.text("Top plane", "顶基准面"),
            egui::Color32::from_rgb(123, 201, 111),
            egui::Color32::from_rgb(235, 248, 233),
        ),
    ];
    let mut picked = None;
    for (i, (plane, axes, title, accent, tint)) in planes.iter().enumerate() {
        let (card, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), egui::Sense::click());
        let hovered = response.hovered() || app.plane_pick_hover == Some(i);
        let fill = if hovered {
            if active_appearance(ui.ctx()).is_dark() {
                ui.visuals().widgets.hovered.weak_bg_fill
            } else {
                *tint
            }
        } else {
            theme_color(ui, PANEL_RAISED)
        };
        let stroke = if hovered {
            egui::Stroke::new(1.25_f32, *accent)
        } else {
            egui::Stroke::new(1.0_f32, theme_color(ui, BORDER))
        };
        ui.painter().rect(
            card,
            egui::CornerRadius::same(8),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );

        let chip = egui::Rect::from_min_size(
            card.left_center() + egui::vec2(12.0, -18.0),
            egui::vec2(42.0, 36.0),
        );
        ui.painter()
            .rect_filled(chip, egui::CornerRadius::same(7), *tint);
        ui.painter().text(
            chip.center(),
            egui::Align2::CENTER_CENTER,
            *axes,
            egui::FontId::proportional(16.0),
            *accent,
        );
        ui.painter().text(
            egui::pos2(chip.right() + 12.0, card.center().y),
            egui::Align2::LEFT_CENTER,
            *title,
            egui::FontId::proportional(13.5),
            theme_color(ui, TEXT),
        );

        if response
            .on_hover_text(if language == UiLanguage::English {
                format!("Sketch on the {axes} plane")
            } else {
                format!("在 {axes} 平面上创建草图")
            })
            .clicked()
        {
            picked = Some(*plane);
        }
        ui.add_space(5.0);
    }

    ui.add_space(2.0);
    ui.separator();
    ui.add_space(5.0);
    ui.label(
        egui::RichText::new(language.text("Custom plane", "自定义平面"))
            .strong()
            .color(theme_color(ui, TEXT)),
    );
    let mut equation_enabled = app.sketch_plane_equation_enabled;
    let mut equation = app.sketch_plane_equation;
    let unit = app.settings.unit;
    if ui
        .checkbox(
            &mut equation_enabled,
            language.text("Linear equation", "线性方程"),
        )
        .on_hover_text(language.text(
            "Define the sketch plane as ax + by + cz = d",
            "将草图平面定义为 ax + by + cz = d",
        ))
        .changed()
    {
        app.set_sketch_plane_equation(equation_enabled, equation);
    }

    let mut equation_changed = false;
    ui.add_enabled_ui(equation_enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            equation_changed |= ui
                .add_sized(
                    [48.0, 28.0],
                    egui::DragValue::new(&mut equation[0]).speed(0.1),
                )
                .changed();
            ui.label("x +");
            equation_changed |= ui
                .add_sized(
                    [48.0, 28.0],
                    egui::DragValue::new(&mut equation[1]).speed(0.1),
                )
                .changed();
            ui.label("y +");
            equation_changed |= ui
                .add_sized(
                    [48.0, 28.0],
                    egui::DragValue::new(&mut equation[2]).speed(0.1),
                )
                .changed();
            ui.label("z =");
            equation_changed |= ui
                .add_sized(
                    [82.0, 28.0],
                    length_drag_value(&mut equation[3], unit).speed(unit.to_mm(0.1)),
                )
                .changed();
        });
    });
    if equation_changed {
        app.set_sketch_plane_equation(equation_enabled, equation);
    }

    if equation_enabled {
        let ready = app.sketch_plane_equation_is_valid();
        ui.label(
            egui::RichText::new(if ready {
                language.text("Custom plane is ready", "自定义平面已就绪")
            } else {
                language.text(
                    "At least one x / y / z coefficient must be non-zero",
                    "x、y、z 系数中至少有一个必须非零",
                )
            })
            .small()
            .color(if ready {
                theme_color(ui, MUTED)
            } else {
                theme_error(ui, egui::Color32::from_rgb(196, 82, 57))
            }),
        );
        if ui
            .add_enabled_ui(ready, |ui| {
                ui.add_sized(
                    [ui.available_width(), 32.0],
                    primary_button(language.text("Start Sketch", "开始草图"), &app.settings),
                )
            })
            .inner
            .clicked()
        {
            app.start_sketch_on_equation();
        }
    }

    ui.add_space(3.0);
    ui.separator();
    ui.add_space(3.0);
    if ui
        .add_sized(
            [ui.available_width(), 30.0],
            egui::Button::new(language.text("Cancel  ·  Esc", "取消  ·  Esc")),
        )
        .clicked()
    {
        app.mode = Mode::Idle;
        app.bump_scene();
    }
    if let Some(plane) = picked {
        app.start_sketch_on(plane);
    }
}
/// Camera 区块：可编辑的 eye/target，实时诊断视线方向与投影模式。
/// Move/Rotate/Scale 的参数化输入（指定轴 + 数值，Apply 应用）。
fn transform_params(app: &mut S33DApp, ui: &mut egui::Ui, gizmo: GizmoMode) {
    let language = app.settings.language;
    let unit = app.settings.unit;
    match gizmo {
        GizmoMode::Translate => {
            // 第一行：六个方向快捷位移（X+ X- Y+ Y- Z+ Z-）+ Step 输入。
            ui.horizontal_wrapped(|ui| {
                for (label, axis, sign) in [
                    ("X+", 0usize, 1.0f32),
                    ("X-", 0, -1.0),
                    ("Y+", 1, 1.0),
                    ("Y-", 1, -1.0),
                    ("Z+", 2, 1.0),
                    ("Z-", 2, -1.0),
                ] {
                    let tip = format!("Move {label} by Step");
                    if ui
                        .add_sized([34.0, 24.0], egui::Button::new(label))
                        .on_hover_text(tip)
                        .clicked()
                    {
                        app.move_nudge(axis, sign);
                    }
                }
                ui.separator();
                ui.label(language.text("Step", "步长"));
                tool_numeric(
                    ui,
                    length_drag_value(&mut app.move_step, unit)
                        .speed(0.1)
                        .range(0.001..=100_000.0),
                )
                .on_hover_text(if language == UiLanguage::English {
                    format!(
                        "Distance moved by each direction button ({})",
                        unit.symbol()
                    )
                } else {
                    format!("每次单击方向按钮移动的距离（{}）", unit.symbol())
                });
            });
            // 输入状态持久化在 app（局部变量每帧会重置，导致输入丢失）
            ui.label(language.text("Move by (world axes):", "按世界轴移动："));
            ui.horizontal(|ui| {
                tool_numeric(
                    ui,
                    length_drag_value(&mut app.move_input[0], unit)
                        .speed(1.0)
                        .prefix("X "),
                );
                tool_numeric(
                    ui,
                    length_drag_value(&mut app.move_input[1], unit)
                        .speed(1.0)
                        .prefix("Y "),
                );
                tool_numeric(
                    ui,
                    length_drag_value(&mut app.move_input[2], unit)
                        .speed(1.0)
                        .prefix("Z "),
                );
            });
            if ui.button(language.text("Apply Move", "应用移动")).clicked() {
                let (dx, dy, dz) = (app.move_input[0], app.move_input[1], app.move_input[2]);
                let mat = glam::Mat4::from_translation(glam::Vec3::new(dx, dy, dz));
                app.apply_transform(&mat, &format!("Move by ({dx}, {dy}, {dz})"));
                app.move_input = [0.0; 3];
            }
            ui.separator();
            // Move To：整组选中 objects 的共同中心移到目标绝对坐标。
            ui.label(language.text("Move selection center to:", "将选区中心移动到："));
            ui.horizontal(|ui| {
                tool_numeric(
                    ui,
                    length_drag_value(&mut app.move_to_input[0], unit)
                        .speed(1.0)
                        .prefix("X "),
                );
                tool_numeric(
                    ui,
                    length_drag_value(&mut app.move_to_input[1], unit)
                        .speed(1.0)
                        .prefix("Y "),
                );
                tool_numeric(
                    ui,
                    length_drag_value(&mut app.move_to_input[2], unit)
                        .speed(1.0)
                        .prefix("Z "),
                );
            });
            if ui
                .button(language.text("Apply Move To", "应用移动到"))
                .clicked()
            {
                app.move_to_selected(app.move_to_input);
            }
            if let Some(center) = app.selected_center() {
                ui.label(
                    egui::RichText::new(if language == UiLanguage::English {
                        format!(
                            "Selection center: ({}, {}, {})",
                            unit.format_mm(center.x as f64),
                            unit.format_mm(center.y as f64),
                            unit.format_mm(center.z as f64)
                        )
                    } else {
                        format!(
                            "选区中心：({}，{}，{})",
                            unit.format_mm(center.x as f64),
                            unit.format_mm(center.y as f64),
                            unit.format_mm(center.z as f64)
                        )
                    })
                    .small()
                    .weak(),
                );
            }
        }
        GizmoMode::Rotate => {
            ui.label(language.text("Rotate around axis:", "绕轴旋转："));
            ui.horizontal(|ui| {
                for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                    let active = app.rotate_custom_axis.is_none() && app.rotate_input.0 == axis;
                    if ui.selectable_label(active, *name).clicked() {
                        app.use_world_rotation_axis(axis);
                    }
                }
            });
            let mut use_selected_axis =
                app.rotate_axis_pick_active || app.rotate_custom_axis.is_some();
            if ui
                .checkbox(
                    &mut use_selected_axis,
                    language.text("Rotate around selected axis", "绕选定轴旋转"),
                )
                .changed()
            {
                app.set_selected_rotation_axis_enabled(use_selected_axis);
            }
            if app.rotate_axis_pick_active {
                ui.label(
                    egui::RichText::new(language.text(
                        "Click a straight edge or sketch line in the viewport…",
                        "在工作区中单击一条直边或草图直线…",
                    ))
                    .small()
                    .color(theme_error(ui, egui::Color32::from_rgb(224, 72, 76))),
                );
            }
            if let Some(axis) = app.rotate_custom_axis {
                ui.label(
                    egui::RichText::new(if language == UiLanguage::English {
                        format!(
                            "Picked axis  p=({:.1}, {:.1}, {:.1})  d=({:.2}, {:.2}, {:.2})",
                            axis.point[0],
                            axis.point[1],
                            axis.point[2],
                            axis.direction[0],
                            axis.direction[1],
                            axis.direction[2]
                        )
                    } else {
                        format!(
                            "选定轴  点=({:.1}, {:.1}, {:.1})  方向=({:.2}, {:.2}, {:.2})",
                            axis.point[0],
                            axis.point[1],
                            axis.point[2],
                            axis.direction[0],
                            axis.direction[1],
                            axis.direction[2]
                        )
                    })
                    .small()
                    .color(theme_color(ui, MUTED)),
                );
                if ui
                    .small_button(language.text("Pick another axis", "选择另一条轴"))
                    .clicked()
                {
                    app.set_selected_rotation_axis_enabled(true);
                }
            } else if !app.rotate_axis_pick_active {
                ui.label(
                    egui::RichText::new(language.text(
                        "World axis passes through the selection center",
                        "世界轴经过选区中心",
                    ))
                    .small()
                    .color(theme_color(ui, MUTED)),
                );
            }
            ui.horizontal(|ui| {
                ui.label(language.text("Angle", "角度"));
                tool_numeric(
                    ui,
                    egui::DragValue::new(&mut app.rotate_input.1)
                        .speed(1.0)
                        .suffix(" deg"),
                );
            });
            ui.horizontal(|ui| {
                ui.label(language.text("Quick:", "快捷："));
                if ui.button("+90°").clicked() {
                    app.rotate_selected(90.0);
                }
                if ui.button("−90°").clicked() {
                    app.rotate_selected(-90.0);
                }
            });
            if ui
                .button(language.text("Apply Rotate", "应用旋转"))
                .clicked()
            {
                let angle = app.rotate_input.1;
                app.rotate_selected(angle);
                app.rotate_input.1 = 0.0;
            }
        }
        GizmoMode::Scale => {
            ui.label(
                egui::RichText::new(language.text("Scale by percent", "按百分比缩放")).strong(),
            );
            if ui
                .checkbox(
                    &mut app.scale_uniform,
                    language.text("Uniform Scale", "等比缩放"),
                )
                .on_hover_text(language.text(
                    "Keep X, Y, and Z percentages equal",
                    "保持 X、Y、Z 的缩放百分比一致",
                ))
                .changed()
                && app.scale_uniform
            {
                app.scale_input = crate::app::synchronized_scale_percent(app.scale_input, 0, true);
            }
            let mut changed_axis = None;
            ui.horizontal(|ui| {
                if tool_numeric(
                    ui,
                    egui::DragValue::new(&mut app.scale_input[0])
                        .speed(1.0)
                        .range(0.01..=1_000_000.0)
                        .prefix("X ")
                        .suffix("%"),
                )
                .changed()
                {
                    changed_axis = Some(0);
                }
                if tool_numeric(
                    ui,
                    egui::DragValue::new(&mut app.scale_input[1])
                        .speed(1.0)
                        .range(0.01..=1_000_000.0)
                        .prefix("Y ")
                        .suffix("%"),
                )
                .changed()
                {
                    changed_axis = Some(1);
                }
                if tool_numeric(
                    ui,
                    egui::DragValue::new(&mut app.scale_input[2])
                        .speed(1.0)
                        .range(0.01..=1_000_000.0)
                        .prefix("Z ")
                        .suffix("%"),
                )
                .changed()
                {
                    changed_axis = Some(2);
                }
            });
            if app.scale_uniform {
                if let Some(axis) = changed_axis {
                    app.scale_input = crate::app::synchronized_scale_percent(
                        app.scale_input,
                        axis,
                        app.scale_uniform,
                    );
                }
            }
            if ui
                .button(language.text("Apply Percent Scale", "应用百分比缩放"))
                .clicked()
            {
                app.scale_selected_by_percent(app.scale_input);
            }

            ui.add_space(5.0);
            ui.separator();
            ui.label(
                egui::RichText::new(language.text("Scale to size", "缩放到指定尺寸")).strong(),
            );
            let current_size = app
                .selected_bounds()
                .map(|(min, max)| (max - min).to_array())
                .unwrap_or(app.scale_size_input);
            if ui
                .checkbox(
                    &mut app.scale_size_uniform,
                    language.text("Uniform Scale", "等比缩放"),
                )
                .on_hover_text(language.text(
                    "Keep the current X, Y, and Z size proportions",
                    "保持当前 X、Y、Z 尺寸比例",
                ))
                .changed()
                && app.scale_size_uniform
            {
                app.scale_size_input = crate::app::synchronized_scale_size(
                    app.scale_size_input,
                    current_size,
                    0,
                    true,
                );
            }
            ui.label(
                egui::RichText::new(language.text("Target selection bounds", "目标选区边界"))
                    .small()
                    .color(theme_color(ui, MUTED)),
            );
            let mut changed_axis = None;
            ui.horizontal(|ui| {
                for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                    if tool_numeric(
                        ui,
                        length_drag_value(&mut app.scale_size_input[axis], unit)
                            .speed(0.5)
                            .range(0.001..=1_000_000.0)
                            .prefix(format!("{label} ")),
                    )
                    .changed()
                    {
                        changed_axis = Some(axis);
                    }
                }
            });
            if app.scale_size_uniform {
                if let Some(axis) = changed_axis {
                    app.scale_size_input = crate::app::synchronized_scale_size(
                        app.scale_size_input,
                        current_size,
                        axis,
                        true,
                    );
                }
            }
            if ui
                .button(language.text("Apply Scale to Size", "应用尺寸缩放"))
                .clicked()
            {
                app.scale_selected_to_size(app.scale_size_input);
            }
        }
    }
}
/// History 栏：最近 200 次操作（最新的在底部，滚动跟随）。
fn history_panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.add_space(8.0);
    let subtitle = if language == UiLanguage::English {
        format!("{} steps", app.history.len())
    } else {
        format!("{} 步", app.history.len())
    };
    section_title(ui, language.text("History", "历史"), Some(&subtitle));
    egui::ScrollArea::vertical()
        .id_salt("history_scroll")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.history.is_empty() {
                ui.label(
                    egui::RichText::new(language.text("No operations yet", "暂无操作"))
                        .color(theme_color(ui, MUTED)),
                );
            } else {
                for (index, entry) in app.history.iter().rev().take(200).enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:02}", index + 1))
                                .monospace()
                                .small()
                                .color(theme_color(ui, ACCENT)),
                        );
                        ui.label(
                            egui::RichText::new(entry)
                                .small()
                                .color(theme_color(ui, MUTED)),
                        );
                    });
                }
            }
        });
}
fn camera_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let unit = app.settings.unit;
    let mut cam = app.camera.clone();
    let mut changed = false;
    section_title(
        ui,
        language.text("View", "视图"),
        Some(if cam.ortho {
            language.text("Orthographic", "正交")
        } else {
            language.text("Perspective", "透视")
        }),
    );
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let width = ((ui.available_width() - ui.spacing().item_spacing.x * 3.0) / 4.0).max(0.0);
        for (label, view) in [
            (language.text("Iso", "等轴测"), ViewDir::Iso),
            (language.text("Top", "顶视"), ViewDir::Top),
            (language.text("Front", "前视"), ViewDir::Front),
            (language.text("Right", "右视"), ViewDir::Right),
        ] {
            if ui
                .add_sized([width, 30.0], egui::Button::new(label))
                .clicked()
            {
                cam.set_view(view);
                changed = true;
            }
        }
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(language.text("Orbit", "环绕"))
                .small()
                .color(theme_color(ui, MUTED)),
        );
        for (direction, dx, dy, tip) in [
            (
                egui::vec2(-1.0, 0.0),
                -18.0,
                0.0,
                language.text("Orbit view left", "向左环绕视图"),
            ),
            (
                egui::vec2(1.0, 0.0),
                18.0,
                0.0,
                language.text("Orbit view right", "向右环绕视图"),
            ),
            (
                egui::vec2(0.0, -1.0),
                0.0,
                -18.0,
                language.text("Orbit view up", "向上环绕视图"),
            ),
            (
                egui::vec2(0.0, 1.0),
                0.0,
                18.0,
                language.text("Orbit view down", "向下环绕视图"),
            ),
        ] {
            if solid_triangle_button(ui, direction, tip).clicked() {
                cam.orbit(dx, dy, 0.008);
                changed = true;
            }
        }
        ui.separator();
        if ui
            .add_sized([28.0, 24.0], egui::Button::new("+"))
            .on_hover_text(language.text("Zoom in", "放大"))
            .clicked()
        {
            cam.zoom(1.0);
            changed = true;
        }
        if ui
            .add_sized([28.0, 24.0], egui::Button::new("−"))
            .on_hover_text(language.text("Zoom out", "缩小"))
            .clicked()
        {
            cam.zoom(-1.0);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(language.text("Pan", "平移视图"))
                .small()
                .color(theme_color(ui, MUTED)),
        );
        for (direction, dx, dy, tip) in [
            (
                egui::vec2(-1.0, 0.0),
                22.0,
                0.0,
                language.text("Pan view left", "向左平移视图"),
            ),
            (
                egui::vec2(1.0, 0.0),
                -22.0,
                0.0,
                language.text("Pan view right", "向右平移视图"),
            ),
            (
                egui::vec2(0.0, -1.0),
                0.0,
                -22.0,
                language.text("Pan view up", "向上平移视图"),
            ),
            (
                egui::vec2(0.0, 1.0),
                0.0,
                22.0,
                language.text("Pan view down", "向下平移视图"),
            ),
        ] {
            if solid_triangle_button(ui, direction, tip).clicked() {
                cam.pan(dx, dy);
                changed = true;
            }
        }
    });
    let mut ortho = cam.ortho;
    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(!ortho, language.text("Perspective", "透视"))
            .on_hover_text(language.text("Perspective projection", "透视投影"))
            .clicked()
        {
            ortho = false;
        }
        if ui
            .selectable_label(ortho, language.text("Orthographic", "正交"))
            .on_hover_text(language.text("Orthographic projection", "正交投影"))
            .clicked()
        {
            ortho = true;
        }
        if ui
            .button(language.text("Fit", "适合"))
            .on_hover_text(language.text("Fit all geometry", "显示全部几何体"))
            .clicked()
        {
            app.fit_view();
            cam = app.camera.clone();
            changed = true;
        }
        if ui.button(language.text("Recenter", "重新居中"))
            .on_hover_text(language.text(
                "Center visible objects in perspective and fit their size; keep the viewing direction",
                "切换到透视，将可见对象居中并适配大小，保留当前观察方向",
            )).clicked() {
            app.recenter_view();
            cam = app.camera.clone();
            ortho = false;
            changed = true;
        }
    });
    if ortho != cam.ortho {
        cam.ortho = ortho;
        changed = true;
    }
    if ortho {
        changed |= tool_numeric(
            ui,
            length_drag_value(&mut cam.ortho_half, unit)
                .speed(1.0)
                .prefix(language.text("View scale ", "视图比例 ")),
        )
        .changed();
    }
    egui::CollapsingHeader::new(language.text("Camera coordinates", "相机坐标"))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(language.text("Eye", "相机位置"))
                    .small()
                    .color(theme_color(ui, MUTED)),
            );
            ui.horizontal(|ui| {
                changed |= tool_numeric(
                    ui,
                    length_drag_value(&mut cam.eye.x, unit)
                        .speed(1.0)
                        .prefix("X "),
                )
                .changed();
                changed |= tool_numeric(
                    ui,
                    length_drag_value(&mut cam.eye.y, unit)
                        .speed(1.0)
                        .prefix("Y "),
                )
                .changed();
                changed |= tool_numeric(
                    ui,
                    length_drag_value(&mut cam.eye.z, unit)
                        .speed(1.0)
                        .prefix("Z "),
                )
                .changed();
            });
            ui.label(
                egui::RichText::new(language.text("Target", "目标点"))
                    .small()
                    .color(theme_color(ui, MUTED)),
            );
            ui.horizontal(|ui| {
                changed |= tool_numeric(
                    ui,
                    length_drag_value(&mut cam.target.x, unit)
                        .speed(1.0)
                        .prefix("X "),
                )
                .changed();
                changed |= tool_numeric(
                    ui,
                    length_drag_value(&mut cam.target.y, unit)
                        .speed(1.0)
                        .prefix("Y "),
                )
                .changed();
                changed |= tool_numeric(
                    ui,
                    length_drag_value(&mut cam.target.z, unit)
                        .speed(1.0)
                        .prefix("Z "),
                )
                .changed();
            });
            let fwd = cam.forward();
            let camera_summary = if language == UiLanguage::English {
                format!(
                    "Direction {:.2}, {:.2}, {:.2}  ·  Distance {}",
                    fwd.x,
                    fwd.y,
                    fwd.z,
                    unit.format_mm(cam.distance() as f64)
                )
            } else {
                format!(
                    "方向 {:.2}, {:.2}, {:.2}  ·  距离 {}",
                    fwd.x,
                    fwd.y,
                    fwd.z,
                    unit.format_mm(cam.distance() as f64)
                )
            };
            ui.label(
                egui::RichText::new(camera_summary)
                    .small()
                    .color(theme_color(ui, MUTED)),
            );
        });

    if changed {
        app.camera = cam;
        app.bump_scene();
    }
}

/// 用几何绘制的实心三角代替字体 glyph，避免系统字体只显示左右、
/// 却缺失上下三角的问题。四个方向的外观与 ◀ ▶ ▲ ▼ 一致。
fn solid_triangle_button(
    ui: &mut egui::Ui,
    direction: egui::Vec2,
    tooltip: &str,
) -> egui::Response {
    let response = ui
        .add_sized([28.0, 24.0], egui::Button::new(""))
        .on_hover_text(tooltip);
    let direction = direction.normalized();
    let perpendicular = egui::vec2(-direction.y, direction.x);
    let center = response.rect.center();
    let points = vec![
        center + direction * 5.2,
        center - direction * 3.4 + perpendicular * 4.4,
        center - direction * 3.4 - perpendicular * 4.4,
    ];
    let color = if response.hovered() || response.has_focus() {
        theme_color(ui, ACCENT)
    } else {
        theme_color(ui, TEXT)
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
    response
}

fn insert_3d_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    // Numeric edit buffers belong to this object/draft, not the previous ball.
    let key = app.insert_3d_ctx.as_ref().map(|ctx| {
        (
            ctx.editing_body,
            ctx.params.map(|p| p.kind as u8),
            app.doc.bodies.len(),
        )
    });
    ui.push_id(("draw3d_draft_properties", key), |ui| {
        insert_3d_properties_inner(app, ui)
    });
}

fn insert_3d_properties_inner(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let unit = app.settings.unit;
    let Some(ctx) = app.insert_3d_ctx.clone() else {
        return;
    };
    section_title(
        ui,
        language.text("Draw 3D", "绘制3D"),
        Some(language.text("Primitive", "基本体")),
    );
    let Some(mut params) = ctx.params else {
        ui.label(
            egui::RichText::new(language.text(
                "Choose a primitive, or select a Draw 3D object in Objects / the workspace",
                "请选择基本体，或在对象列表/工作区中选择一个绘制3D对象",
            ))
            .color(theme_color(ui, MUTED)),
        );
        ui.label(language.text(
            "Box · Cylinder · Sphere · Cone · Torus",
            "长方体 · 圆柱体 · 球体 · 圆锥体 · 圆环体",
        ));
        ui.add_space(6.0);
        if ui
            .button(language.text("Exit Draw 3D", "退出绘制3D"))
            .clicked()
        {
            app.exit_insert_3d();
        }
        return;
    };

    if ctx.stage == Draw3DPlacementStage::AwaitBase {
        ui.label(
            egui::RichText::new(primitive_name(language, params.kind))
                .size(15.0)
                .strong()
                .color(theme_color(ui, TEXT)),
        );
        ui.add_space(4.0);
        ui.label(if params.kind == Primitive3DKind::Sphere {
            language.text(
                "Click in the workspace to set the sphere center.",
                "在工作区单击以确定球心。",
            )
        } else {
            language.text(
                "Click in the workspace to set the base center.",
                "在工作区单击以确定底部中心点。",
            )
        });
        ui.add_space(8.0);
        if ui
            .button(language.text("Exit Draw 3D", "退出绘制3D"))
            .clicked()
        {
            app.cancel_insert_3d();
        }
        return;
    }

    if let Some(id) = ctx.editing_body {
        let name = app
            .body(id)
            .map(|body| body.name.as_str())
            .unwrap_or(language.text("Object", "对象"));
        ui.label(
            egui::RichText::new(if language == UiLanguage::English {
                format!("Editing: {name}")
            } else {
                format!("正在编辑：{name}")
            })
            .small()
            .color(theme_color(ui, ACCENT)),
        );
    }
    ui.label(
        egui::RichText::new(primitive_name(language, params.kind))
            .size(15.0)
            .strong()
            .color(theme_color(ui, TEXT)),
    );
    ui.add_space(3.0);
    ui.label(
        egui::RichText::new(if params.kind == Primitive3DKind::Sphere {
            language.text("Center Position", "球心位置")
        } else {
            language.text("Base Position", "基准位置")
        })
        .strong(),
    );
    let mut reference = crate::app::draw_3d_reference_point(&params);
    let mut changed = false;
    egui::Grid::new("insert_3d_position")
        .num_columns(2)
        .spacing([8.0, 5.0])
        .show(ui, |ui| {
            for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.label(label);
                changed |=
                    tool_numeric(ui, length_drag_value(&mut reference[axis], unit).speed(0.5))
                        .changed();
                ui.end_row();
            }
        });

    ui.add_space(4.0);
    ui.label(egui::RichText::new(language.text("Dimensions", "尺寸")).strong());
    egui::Grid::new("insert_3d_dimensions")
        .num_columns(2)
        .spacing([8.0, 5.0])
        .show(ui, |ui| match params.kind {
            Primitive3DKind::Box => {
                for (label, value) in [
                    (language.text("Width", "宽度"), &mut params.width),
                    (language.text("Depth", "深度"), &mut params.depth),
                    (language.text("Height", "高度"), &mut params.height),
                ] {
                    ui.label(label);
                    changed |= tool_numeric(
                        ui,
                        length_drag_value(value, unit)
                            .speed(0.5)
                            .range(0.1..=1_000_000.0),
                    )
                    .changed();
                    ui.end_row();
                }
            }
            Primitive3DKind::Cylinder => {
                for (label, value) in [
                    (language.text("Radius", "半径"), &mut params.radius),
                    (language.text("Height", "高度"), &mut params.height),
                ] {
                    ui.label(label);
                    changed |= tool_numeric(
                        ui,
                        length_drag_value(value, unit)
                            .speed(0.5)
                            .range(0.1..=1_000_000.0),
                    )
                    .changed();
                    ui.end_row();
                }
            }
            Primitive3DKind::Sphere => {
                ui.label(language.text("Radius", "半径"));
                changed |= tool_numeric(
                    ui,
                    length_drag_value(&mut params.radius, unit)
                        .speed(0.5)
                        .range(0.1..=1_000_000.0),
                )
                .changed();
                ui.end_row();
            }
            Primitive3DKind::Cone => {
                for (label, value, minimum) in [
                    (
                        language.text("Bottom Radius", "底部半径"),
                        &mut params.radius,
                        0.1,
                    ),
                    (
                        language.text("Top Radius", "顶部半径"),
                        &mut params.top_radius,
                        0.0,
                    ),
                    (language.text("Height", "高度"), &mut params.height, 0.1),
                ] {
                    ui.label(label);
                    changed |= tool_numeric(
                        ui,
                        length_drag_value(value, unit)
                            .speed(0.5)
                            .range(minimum..=1_000_000.0),
                    )
                    .changed();
                    ui.end_row();
                }
            }
            Primitive3DKind::Torus => {
                for (label, value) in [
                    (
                        language.text("Major Radius", "大半径"),
                        &mut params.major_radius,
                    ),
                    (
                        language.text("Minor Radius", "小半径"),
                        &mut params.minor_radius,
                    ),
                ] {
                    ui.label(label);
                    changed |= tool_numeric(
                        ui,
                        length_drag_value(value, unit)
                            .speed(0.5)
                            .range(0.1..=1_000_000.0),
                    )
                    .changed();
                    ui.end_row();
                }
            }
        });
    if changed {
        crate::app::set_draw_3d_reference_point(&mut params, reference);
        app.update_insert_3d_params(params);
    }

    if ctx.stage == Draw3DPlacementStage::Precision {
        ui.add_space(5.0);
        ui.label(
            egui::RichText::new(language.text(
                "Drag the preview to move it · drag red key points to resize",
                "拖动预览可移动 · 拖动红色关键点可改尺寸",
            ))
            .small()
            .color(theme_color(ui, MUTED)),
        );
    }

    ui.add_space(7.0);
    let ready = app.insert_3d_ctx.as_ref().is_some_and(|context| {
        context.stage == Draw3DPlacementStage::Precision && context.preview_shape.is_some()
    });
    let applied = app
        .insert_3d_ctx
        .as_ref()
        .is_some_and(|context| context.parameters_applied);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                ready,
                primary_button(language.text("Apply", "应用"), &app.settings),
            )
            .clicked()
        {
            app.apply_insert_3d_params();
        }
        if ui
            .add_enabled(
                ready && applied,
                egui::Button::new(language.text("Done", "完成")),
            )
            .clicked()
        {
            app.done_insert_3d();
        }
        if ui
            .button(language.text("Exit Draw 3D", "退出绘制3D"))
            .clicked()
        {
            app.exit_insert_3d();
        }
    });
    ui.label(
        egui::RichText::new(language.text(
            "Apply accepts the precise parameters · Done commits the object",
            "应用会确认精确参数 · 完成会提交对象",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );
    ui.label(
        egui::RichText::new(language.text(
            "Esc cancels the uncommitted preview",
            "Esc 取消尚未提交的预览",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );
}

/// 平面选择面板的交互动作（Mirror / Split / Section View 共用）。
enum PlanePickerAction {
    Datum(MirrorPlaneSource),
    BeginSurfacePick,
    EnableEquation,
    DisableEquation,
    Equation([f64; 4]),
}

/// Mirror / Split / Section View 共用的平面选择 UI：
/// XY/YZ/ZX 基准面 + 表面拾取按钮 + 可勾选的 ax + by + cz = d 方程。
fn plane_picker_panel(
    ui: &mut egui::Ui,
    language: UiLanguage,
    unit: UiUnit,
    plane_source: Option<MirrorPlaneSource>,
    plane_face_label: Option<String>,
    equation: [f64; 4],
    equation_enabled: bool,
) -> Option<PlanePickerAction> {
    let mut action = None;
    let mut datum_choice = plane_source;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut datum_choice, Some(MirrorPlaneSource::XY), "XY");
        ui.selectable_value(&mut datum_choice, Some(MirrorPlaneSource::YZ), "YZ");
        ui.selectable_value(&mut datum_choice, Some(MirrorPlaneSource::ZX), "ZX");
    });
    if datum_choice != plane_source {
        if let Some(
            source @ (MirrorPlaneSource::XY | MirrorPlaneSource::YZ | MirrorPlaneSource::ZX),
        ) = datum_choice
        {
            action = Some(PlanePickerAction::Datum(source));
        }
    }

    let surface_active = plane_source == Some(MirrorPlaneSource::Surface);
    let plane_button = egui::Button::new(
        egui::RichText::new(plane_face_label.unwrap_or_else(|| {
            language
                .text("Select a surface…", "选择一个表面…")
                .to_string()
        }))
        .color(if surface_active {
            egui::Color32::from_rgb(205, 92, 15)
        } else {
            theme_color(ui, MUTED)
        }),
    )
    .fill(if surface_active {
        egui::Color32::from_rgb(255, 240, 225)
    } else {
        theme_color(ui, PANEL_RAISED)
    })
    .stroke(egui::Stroke::new(
        if surface_active { 2.0_f32 } else { 1.0_f32 },
        egui::Color32::from_rgb(239, 126, 34),
    ));
    if ui
        .add_sized([ui.available_width(), 34.0], plane_button)
        .on_hover_text(language.text(
            "Click, then pick a planar surface in the viewport",
            "单击后在工作区中选择一个平面表面",
        ))
        .clicked()
    {
        action = Some(PlanePickerAction::BeginSurfacePick);
    }

    ui.add_space(5.0);
    let mut enabled = equation_enabled;
    if ui
        .checkbox(&mut enabled, language.text("Linear equation", "线性方程"))
        .on_hover_text(language.text(
            "Define the plane as ax + by + cz = d",
            "将平面定义为 ax + by + cz = d",
        ))
        .changed()
    {
        action = Some(if enabled {
            PlanePickerAction::EnableEquation
        } else {
            PlanePickerAction::DisableEquation
        });
    }
    let mut values = equation;
    let mut equation_changed = false;
    ui.add_enabled_ui(equation_enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            equation_changed |= ui
                .add_sized(
                    [48.0, 28.0],
                    egui::DragValue::new(&mut values[0]).speed(0.1),
                )
                .changed();
            ui.label("x +");
            equation_changed |= ui
                .add_sized(
                    [48.0, 28.0],
                    egui::DragValue::new(&mut values[1]).speed(0.1),
                )
                .changed();
            ui.label("y +");
            equation_changed |= ui
                .add_sized(
                    [48.0, 28.0],
                    egui::DragValue::new(&mut values[2]).speed(0.1),
                )
                .changed();
            ui.label("z =");
            equation_changed |= ui
                .add_sized(
                    [82.0, 28.0],
                    length_drag_value(&mut values[3], unit).speed(unit.to_mm(0.1)),
                )
                .changed();
        });
    });
    if equation_enabled && equation_changed {
        action = Some(PlanePickerAction::Equation(values));
    }
    action
}

fn mirror_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let Some(ctx) = app.mirror_ctx.clone() else {
        return;
    };
    section_title(
        ui,
        language.text("Mirror", "镜像"),
        Some(language.text("Transform", "变换")),
    );

    ui.label(
        egui::RichText::new(language.text("Objects", "对象"))
            .strong()
            .color(theme_color(ui, TEXT)),
    );
    egui::Frame::new()
        .fill(theme_color(ui, egui::Color32::from_rgb(242, 246, 251)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            for source in &ctx.sources {
                let name = app
                    .body(*source)
                    .map(|body| body.name.as_str())
                    .unwrap_or(language.text("Unavailable object", "不可用对象"));
                ui.label(format!("●  {name}"));
            }
        });
    ui.label(
        egui::RichText::new(language.text(
            "Source objects are locked for this operation",
            "本次操作的源对象已锁定",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(language.text("Mirror Plane", "镜像平面"))
            .strong()
            .color(theme_color(ui, TEXT)),
    );

    let plane_name = if ctx.plane_source == Some(MirrorPlaneSource::Surface) {
        ctx.plane.as_ref().map(|face| {
            let owner = app
                .body(face.body_id)
                .map(|body| body.name.as_str())
                .unwrap_or(language.text("Object", "对象"));
            if language == UiLanguage::English {
                format!("●  {owner} · surface")
            } else {
                format!("●  {owner} · 表面")
            }
        })
    } else {
        None
    };
    if let Some(action) = plane_picker_panel(
        ui,
        language,
        app.settings.unit,
        ctx.plane_source,
        plane_name,
        ctx.equation,
        ctx.equation_enabled,
    ) {
        match action {
            PlanePickerAction::Datum(source) => app.set_mirror_datum_plane(source),
            PlanePickerAction::BeginSurfacePick => app.begin_mirror_plane_pick(),
            PlanePickerAction::EnableEquation => app.set_mirror_equation(ctx.equation),
            PlanePickerAction::DisableEquation => app.disable_mirror_equation(),
            PlanePickerAction::Equation(values) => app.set_mirror_equation(values),
        }
    }

    let has_plane = ctx.plane_def.is_some();
    let ready = has_plane && ctx.previews.len() == ctx.sources.len() && !ctx.previews.is_empty();
    let readiness = if let Some(plane) = &ctx.plane_def {
        let normal = plane.normal();
        if language == UiLanguage::English {
            format!(
                "{} preview ready · normal ({:.2}, {:.2}, {:.2})",
                ctx.plane_source
                    .map(MirrorPlaneSource::label)
                    .unwrap_or("Mirror"),
                normal.x,
                normal.y,
                normal.z
            )
        } else {
            format!(
                "镜像预览已就绪 · 法向 ({:.2}, {:.2}, {:.2})",
                normal.x, normal.y, normal.z
            )
        }
    } else if ctx.equation_enabled {
        language
            .text(
                "Equation needs at least one non-zero coefficient",
                "方程至少需要一个非零系数",
            )
            .to_string()
    } else {
        language
            .text(
                "Choose XY / YZ / ZX, or click a planar surface",
                "请选择 XY / YZ / ZX，或单击一个平面表面",
            )
            .to_string()
    };
    ui.label(
        egui::RichText::new(readiness)
            .small()
            .color(theme_color(ui, MUTED)),
    );
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                ready,
                primary_button(language.text("Apply", "应用"), &app.settings),
            )
            .clicked()
        {
            app.apply_mirror();
        }
        if ui
            .button(language.text("Cancel (Esc)", "取消（Esc）"))
            .clicked()
        {
            app.cancel_mirror();
        }
    });
}

/// Split 属性面板：object 字段 + 切割平面（XY/YZ/ZX/表面/方程）+ Keep original + Apply。
fn split_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let Some(ctx) = app.split_ctx.clone() else {
        return;
    };
    section_title(
        ui,
        language.text("Split", "分割"),
        Some(language.text("Boolean", "布尔运算")),
    );

    ui.label(
        egui::RichText::new(language.text("Object", "对象"))
            .strong()
            .color(theme_color(ui, TEXT)),
    );
    let target_name = ctx
        .target
        .and_then(|id| app.body(id).map(|body| format!("●  {}", body.name.clone())));
    let target_button = egui::Button::new(
        egui::RichText::new(target_name.unwrap_or_else(|| {
            language
                .text("Select an object…", "选择一个对象…")
                .to_string()
        }))
        .color(if ctx.target.is_some() {
            egui::Color32::from_rgb(9, 139, 134)
        } else {
            theme_color(ui, MUTED)
        }),
    )
    .fill(if ctx.target.is_some() {
        egui::Color32::from_rgb(229, 248, 246)
    } else if ctx.picking {
        theme_color(ui, ACCENT_SOFT)
    } else {
        theme_color(ui, PANEL_RAISED)
    })
    .stroke(egui::Stroke::new(
        if ctx.picking { 2.0_f32 } else { 1.0_f32 },
        egui::Color32::from_rgb(23, 176, 168),
    ));
    if ui
        .add_sized([ui.available_width(), 34.0], target_button)
        .on_hover_text(language.text(
            "Click, then pick the solid in the viewport or Objects list",
            "单击后在工作区或对象列表中选择实体",
        ))
        .clicked()
    {
        app.begin_split_object_pick();
    }
    ui.label(
        egui::RichText::new(language.text(
            "The object is cut into two or more independent objects",
            "对象将被切分为两个或更多独立对象",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(language.text("Cutting Plane", "切割平面"))
            .strong()
            .color(theme_color(ui, TEXT)),
    );
    let plane_name = if ctx.plane_source == Some(MirrorPlaneSource::Surface) {
        ctx.plane.as_ref().map(|face| {
            let owner = app
                .body(face.body_id)
                .map(|body| body.name.as_str())
                .unwrap_or(language.text("Object", "对象"));
            if language == UiLanguage::English {
                format!("●  {owner} · surface")
            } else {
                format!("●  {owner} · 表面")
            }
        })
    } else {
        None
    };
    if let Some(action) = plane_picker_panel(
        ui,
        language,
        app.settings.unit,
        ctx.plane_source,
        plane_name,
        ctx.equation,
        ctx.equation_enabled,
    ) {
        match action {
            PlanePickerAction::Datum(source) => app.set_split_datum_plane(source),
            PlanePickerAction::BeginSurfacePick => app.begin_split_plane_pick(),
            PlanePickerAction::EnableEquation => app.set_split_equation(ctx.equation),
            PlanePickerAction::DisableEquation => app.disable_split_equation(),
            PlanePickerAction::Equation(values) => app.set_split_equation(values),
        }
    }

    let has_plane = ctx.plane_def.is_some();
    let ready = has_plane && ctx.previews.len() >= 2;
    let readiness = if let Some(plane) = &ctx.plane_def {
        let normal = plane.normal();
        let label = ctx
            .plane_source
            .map(MirrorPlaneSource::label)
            .unwrap_or("Split");
        if language == UiLanguage::ChineseSimplified && ctx.previews.len() >= 2 {
            format!(
                "分割预览已就绪 · {} 个部分 · 法向 ({:.2}, {:.2}, {:.2})",
                ctx.previews.len(),
                normal.x,
                normal.y,
                normal.z
            )
        } else if language == UiLanguage::ChineseSimplified {
            format!(
                "该平面未切割对象 · 法向 ({:.2}, {:.2}, {:.2})",
                normal.x, normal.y, normal.z
            )
        } else if ctx.previews.len() >= 2 {
            format!(
                "{} preview ready · {} pieces · normal ({:.2}, {:.2}, {:.2})",
                label,
                ctx.previews.len(),
                normal.x,
                normal.y,
                normal.z
            )
        } else {
            format!(
                "{} plane does not cut the object · normal ({:.2}, {:.2}, {:.2})",
                label, normal.x, normal.y, normal.z
            )
        }
    } else if ctx.equation_enabled {
        language
            .text(
                "Equation needs at least one non-zero coefficient",
                "方程至少需要一个非零系数",
            )
            .to_string()
    } else {
        language
            .text(
                "Choose XY / YZ / ZX, or click a planar surface",
                "请选择 XY / YZ / ZX，或单击一个平面表面",
            )
            .to_string()
    };
    ui.label(
        egui::RichText::new(readiness)
            .small()
            .color(theme_color(ui, MUTED)),
    );

    let mut keep_originals = ctx.keep_originals;
    if ui
        .checkbox(
            &mut keep_originals,
            language.text("Keep original object", "保留原对象"),
        )
        .changed()
    {
        if let Some(current) = app.split_ctx.as_mut() {
            current.keep_originals = keep_originals;
        }
    }
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                ready,
                primary_button(language.text("Apply", "应用"), &app.settings),
            )
            .clicked()
        {
            app.apply_split();
        }
        if ui
            .button(language.text("Cancel (Esc)", "取消（Esc）"))
            .clicked()
        {
            app.cancel_split();
        }
    });
}

/// Section View 属性面板：剖面平面（XY/YZ/ZX/表面/方程）+ Flip + 退出。
fn section_view_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let Some(ctx) = app.section_view_ctx.clone() else {
        return;
    };
    section_title(
        ui,
        language.text("Section View", "剖切视图"),
        Some(language.text("View", "视图")),
    );
    ui.label(
        egui::RichText::new(language.text(
            "Intersecting objects are cut; others stay whole. Drag the arrow to move the plane.",
            "相交对象正常剖切，其余保持完整；拖动箭头移动剖面。",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(language.text("Section Plane", "剖切平面"))
            .strong()
            .color(theme_color(ui, TEXT)),
    );
    let plane_name = if ctx.plane_source == Some(MirrorPlaneSource::Surface) {
        ctx.plane.as_ref().map(|face| {
            let owner = app
                .body(face.body_id)
                .map(|body| body.name.as_str())
                .unwrap_or(language.text("Object", "对象"));
            if language == UiLanguage::English {
                format!("●  {owner} · surface")
            } else {
                format!("●  {owner} · 表面")
            }
        })
    } else {
        None
    };
    if let Some(action) = plane_picker_panel(
        ui,
        language,
        app.settings.unit,
        ctx.plane_source,
        plane_name,
        ctx.equation,
        ctx.equation_enabled,
    ) {
        match action {
            PlanePickerAction::Datum(source) => app.set_section_view_datum_plane(source),
            PlanePickerAction::BeginSurfacePick => app.begin_section_plane_pick(),
            PlanePickerAction::EnableEquation => app.set_section_view_equation(ctx.equation),
            PlanePickerAction::DisableEquation => app.disable_section_view_equation(),
            PlanePickerAction::Equation(values) => app.set_section_view_equation(values),
        }
    }

    if let Some(plane) = ctx.plane_def {
        let [a, b, c, d] = crate::app::section_interaction::equation(&plane);
        ui.monospace(format!("{a:.3}x + {b:.3}y + {c:.3}z = {d:.3} mm"));
        ui.small(language.text(
            "Drag the arrow in the viewport along the normal",
            "在视口拖动箭头沿法线移动剖面",
        ));
    }
    let mut flip = ctx.flip;
    if ui
        .checkbox(
            &mut flip,
            language.text("Flip side (show opposite half)", "翻转一侧（显示另一半）"),
        )
        .on_hover_text(language.text(
            "Show the opposite half of intersecting objects; keep others whole",
            "显示相交实体的另一半；不相交对象保持完整",
        ))
        .changed()
    {
        app.set_section_view_flip(flip);
    }

    let readiness = if let Some(plane) = &ctx.plane_def {
        let normal = plane.normal();
        if language == UiLanguage::English {
            format!(
                "Sectioning active · normal ({:.2}, {:.2}, {:.2})",
                normal.x, normal.y, normal.z
            )
        } else {
            format!(
                "剖切已激活 · 法向 ({:.2}, {:.2}, {:.2})",
                normal.x, normal.y, normal.z
            )
        }
    } else if ctx.equation_enabled {
        language
            .text(
                "Equation needs at least one non-zero coefficient",
                "方程至少需要一个非零系数",
            )
            .to_string()
    } else {
        language
            .text(
                "Choose XY / YZ / ZX, or click a planar surface",
                "请选择 XY / YZ / ZX，或单击一个平面表面",
            )
            .to_string()
    };
    ui.label(
        egui::RichText::new(readiness)
            .small()
            .color(theme_color(ui, MUTED)),
    );
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        if ui
            .button(language.text("Exit Section View (Esc)", "退出剖切视图（Esc）"))
            .clicked()
        {
            app.cancel_section_view();
        }
    });
}

fn axis_align_controls(app: &mut S33DApp, ui: &mut egui::Ui) {
    let l = app.settings.language;
    let unit = app.settings.unit;
    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(
                app.axis_align.picking,
                l.text("Pick target axis", "拾取目标轴线"),
            )
            .clicked()
        {
            app.axis_align.picking = !app.axis_align.picking;
            app.bump_scene();
        }
        if ui
            .add_enabled(
                app.align_reference.is_some(),
                egui::Button::new(l.text("Use locked object's axis", "使用锁定对象轴线")),
            )
            .clicked()
        {
            app.align_axis_from_reference();
        }
    });
    ui.label(l.text(
        "Axis equation: P(t) = (X, Y, Z) + t (a, b, c)",
        "轴线方程：P(t) = (X, Y, Z) + t (a, b, c)",
    ));
    let mut changed = false;
    ui.horizontal(|ui| {
        for (i, label) in ["X ", "Y ", "Z "].iter().enumerate() {
            changed |= tool_numeric(
                ui,
                length_drag_value(&mut app.axis_align.point[i], unit)
                    .speed(0.1)
                    .prefix(*label),
            )
            .changed();
        }
    });
    ui.horizontal(|ui| {
        for (i, label) in ["a ", "b ", "c "].iter().enumerate() {
            changed |= tool_numeric(
                ui,
                egui::DragValue::new(&mut app.axis_align.direction[i])
                    .speed(0.01)
                    .prefix(*label),
            )
            .changed();
        }
    });
    if changed {
        app.axis_align.fixed_body = None;
        app.axis_align.picking = false;
        app.bump_scene();
    }
    ui.checkbox(
        &mut app.axis_align.cylinder_axes,
        l.text("Use cylinder axes automatically", "自动使用圆柱自身轴线"),
    );
    ui.horizontal(|ui| {
        ui.label(l.text("Other objects' axis:", "其它对象自身轴："));
        for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
            ui.selectable_value(&mut app.axis_align.source_axis, i, *label);
        }
    });
    let valid = app
        .axis_align
        .point
        .iter()
        .chain(app.axis_align.direction.iter())
        .all(|v| v.is_finite())
        && glam::DVec3::from_array(app.axis_align.direction).length_squared() > 1e-12;
    let movable = app.selected.iter().any(|id| {
        Some(*id) != app.axis_align.fixed_body
            && Some(*id) != app.align_reference
            && app
                .body(*id)
                .is_some_and(|b| b.kind == BodyKind::Solid && b.shape.is_some())
    });
    if ui
        .add_enabled(
            valid && movable && !app.axis_align.picking,
            primary_button(l.text("Apply", "应用"), &app.settings),
        )
        .clicked()
    {
        app.align_selected_to_axis();
    }
    ui.weak(if app.axis_align.picking {
        l.text(
            "Click a cylinder wall or straight edge",
            "请单击圆柱面或直边",
        )
    } else {
        l.text(
            "Rotate axes onto the line; keep position along it. A picked cylinder stays fixed.",
            "将自身轴线旋转至目标轴；保留沿轴位置。拾取的圆柱保持不动。",
        )
    });
}

fn properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let focus_key = format!(
        "{:?}:{:?}:{}:{}:{:?}",
        app.mode,
        app.gizmo,
        app.array_active,
        app.chamfer_active,
        app.insert_3d_ctx.as_ref().map(|c| c.stage)
    );
    let active = matches!(
        app.mode,
        Mode::Extrude
            | Mode::Revolve
            | Mode::Insert3D
            | Mode::OffsetShell
            | Mode::Split
            | Mode::Mirror
            | Mode::SectionView
    ) || (app.mode == Mode::Idle
        && (app.gizmo.is_some() || app.array_active || app.chamfer_active));
    arm_tool_focus(
        ui.ctx(),
        format!("{}:{focus_key}", app.numeric_focus_generation),
        active,
    );
    let _focus_scope = ToolFocusScope(ui.ctx().clone());
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("numeric_tool_focus_scope"), true));

    let language = app.settings.language;
    let unit = app.settings.unit;
    section_title(ui, language.text("Properties", "属性"), None);
    ui.add_space(4.0);

    if app.drawing.open {
        crate::drawing2d::panel(app, ui);
        return;
    }

    if !app.thread_ui.expanded && !app.open_thread_panel {
        beginner_guidance(app, ui);
    }
    if app.mode == Mode::Idle {
        crate::threads::panel(app, ui);
        if app.thread_ui.expanded {
            return;
        }
    }
    if app.mode == Mode::OffsetShell {
        crate::app::exact_tools::panel(app, ui);
        return;
    }
    if app.mode == Mode::EquationCurve {
        ui.heading(language.text("Draw Equation", "方程绘图"));
        ui.horizontal(|ui| {
            let surface = app.surface_ctx.is_some();
            if ui
                .selectable_label(!surface, language.text("Line", "线"))
                .on_hover_text(language.text(
                    "Change type; replace the current draft",
                    "切换类型并替换当前草稿",
                ))
                .clicked()
                && surface
            {
                app.cancel_equation_curve();
                app.start_equation_curve(None);
            }
            if ui
                .selectable_label(surface, language.text("Surface", "曲面"))
                .on_hover_text(language.text(
                    "Change type; replace the current draft",
                    "切换类型并替换当前草稿",
                ))
                .clicked()
                && !surface
            {
                app.start_equation_surface(None);
            }
        });
        if app.surface_ctx.is_some() {
            crate::app::equation_surface::panel(app, ui);
        } else {
            crate::app::equation_curve::panel(app, ui);
        }
        return;
    }

    // PickPlane 模式：直接选择草图平面（XY / YZ / ZX）
    if app.mode == Mode::PickPlane {
        sketch_plane_picker(app, ui);
        return;
    }

    if app.mode == Mode::RevolveAxis {
        revolve_axis_properties(app, ui);
        return;
    }

    if app.mode == Mode::Insert3D {
        insert_3d_properties(app, ui);
        return;
    }

    if app.mode == Mode::Mirror {
        mirror_properties(app, ui);
        return;
    }

    if app.mode == Mode::Split {
        split_properties(app, ui);
        return;
    }

    if app.mode == Mode::SectionView {
        section_view_properties(app, ui);
        return;
    }

    if let Some(edge) = app.selected_edge.as_ref() {
        let length_mm: f64 = edge
            .segments
            .iter()
            .map(|segment| {
                (glam::Vec3::from_array(segment.b) - glam::Vec3::from_array(segment.a)).length()
                    as f64
            })
            .sum();
        section_title(
            ui,
            language.text("Measure", "测量"),
            Some(language.text("Edge", "边")),
        );
        ui.label(format!(
            "{}: {}",
            language.text("Length", "长度"),
            unit.format_mm(length_mm)
        ));
        ui.label(format!(
            "{}: {}",
            language.text("Segments", "分段"),
            edge.segments.len()
        ));
        ui.separator();
    }

    // 选中的 3D 表面（可作为 sketch 平面）
    if let Some(face) = app.face_pick.clone() {
        let unit = app.settings.unit;
        let cylinder = app.thread_target_for_face(&face);
        ui.label(egui::RichText::new(language.text("Face", "面")).strong());
        if let Some(target) = &cylinder {
            ui.label(if target.internal {
                language.text("Complete bore wall", "完整内孔壁")
            } else {
                language.text("Complete cylinder side", "完整圆柱外表面")
            });
            ui.label(format!(
                "Ø{} · {} {}",
                unit.format_mm(target.radius * 2.),
                language.text("Length", "长度"),
                unit.format_mm(target.length)
            ));
            let key = egui::Id::new((
                "cylinder_diameter",
                target.body_id,
                app.doc.body(target.body_id).and_then(|b| b.shape),
            ));
            let mut diameter = ui
                .ctx()
                .data(|data| data.get_temp::<f64>(key))
                .unwrap_or(target.radius * 2.);
            let exact = app.cylinder_diameter_editable(target);
            ui.add_enabled_ui(exact, |ui| {
                ui.horizontal(|ui| {
                    ui.label(language.text("Diameter", "直径"));
                    tool_numeric(ui, length_drag_value(&mut diameter, unit).speed(0.01));
                    if ui.button(language.text("Apply", "应用")).clicked() {
                        if let Err(error) = app.edit_cylinder_diameter(target.clone(), diameter) {
                            app.toast(error);
                        }
                    }
                });
            });
            ui.ctx().data_mut(|data| data.insert_temp(key, diameter));
            ui.small(if exact {
                language.text(
                    "Direct solid edit; one Undo step.",
                    "实体直接编辑；一步撤销。",
                )
            } else {
                language.text("Mesh wall: diameter is read-only.", "网格柱面：直径只读。")
            });
        }
        ui.add_enabled_ui(cylinder.is_none(), |ui| {
            ui.label(if language == UiLanguage::English {
                format!(
                    "Normal: ({:.2}, {:.2}, {:.2})",
                    face.normal[0], face.normal[1], face.normal[2]
                )
            } else {
                format!(
                    "法向：({:.2}, {:.2}, {:.2})",
                    face.normal[0], face.normal[1], face.normal[2]
                )
            });
            ui.label(if language == UiLanguage::English {
                format!(
                    "Center: ({}, {}, {})",
                    unit.format_mm(face.center[0] as f64),
                    unit.format_mm(face.center[1] as f64),
                    unit.format_mm(face.center[2] as f64)
                )
            } else {
                format!(
                    "中心：({}, {}, {})",
                    unit.format_mm(face.center[0] as f64),
                    unit.format_mm(face.center[1] as f64),
                    unit.format_mm(face.center[2] as f64)
                )
            });
            if let Some(area_mm2) = measured_face_area(&face) {
                ui.label(format!(
                    "{}: {}",
                    language.text("Area", "面积"),
                    format_area(unit, area_mm2)
                ));
            }
            if ui
                .add(primary_button(
                    language.text("Sketch on this surface", "在此表面创建草图"),
                    &app.settings,
                ))
                .clicked()
            {
                app.start_sketch_on_face(&face);
            }
            ui.horizontal(|ui| {
                if ui.button(language.text("Extrude", "拉伸")).clicked() {
                    app.start_extrude();
                }
                if ui.button(language.text("Pocket", "切除")).clicked() {
                    app.start_pocket();
                }
                if ui.button(language.text("Revolve", "旋转体")).clicked() {
                    app.start_revolve();
                }
            });
        });
        if ui
            .button(language.text("Deselect face (Esc)", "取消选择面（Esc）"))
            .clicked()
        {
            app.deselect_surface();
        }
        ui.separator();
    }

    // Align 面板（与其它操作面板一致：位于 Camera 之下；工具栏 Align 打开）
    if app.align_active {
        let objects: Vec<(u64, String, bool)> = app
            .selected
            .iter()
            .filter_map(|id| {
                app.body(*id).map(|body| {
                    (
                        *id,
                        body.name.clone(),
                        body.kind == BodyKind::Solid && body.shape.is_some(),
                    )
                })
            })
            .collect();
        if app.align_reference.is_some_and(|reference| {
            !objects
                .iter()
                .any(|(id, _, solid)| *id == reference && *solid)
        }) {
            app.align_reference = None;
        }

        let align_subtitle = if language == UiLanguage::English {
            format!("{} objects", objects.len())
        } else {
            format!("{} 个对象", objects.len())
        };
        section_title(ui, language.text("Align", "对齐"), Some(&align_subtitle));
        if ui
            .checkbox(
                &mut app.axis_align.enabled,
                language.text("Center-axis alignment", "中心轴线对齐"),
            )
            .changed()
        {
            app.axis_align.picking = false;
            app.bump_scene();
        }
        if app.axis_align.enabled {
            axis_align_controls(app, ui);
        } else {
            // 操作控件始终放在可卷动 object 选单之前；选单再长也不会
            // 把 Axis / Mode / Apply 挤出当前面板。
            ui.horizontal(|ui| {
                ui.label(language.text("Axis:", "轴："));
                ui.selectable_value(&mut app.align_axis, 0, "X");
                ui.selectable_value(&mut app.align_axis, 1, "Y");
                ui.selectable_value(&mut app.align_axis, 2, "Z");
            });
            ui.horizontal(|ui| {
                ui.label(language.text("Mode:", "模式："));
                ui.selectable_value(
                    &mut app.align_mode,
                    AlignMode::Min,
                    language.text("Min", "最小"),
                );
                ui.selectable_value(
                    &mut app.align_mode,
                    AlignMode::Center,
                    language.text("Center", "居中"),
                );
                ui.selectable_value(
                    &mut app.align_mode,
                    AlignMode::Max,
                    language.text("Max", "最大"),
                );
            });
            let solid_count = objects.iter().filter(|(_, _, solid)| *solid).count();
            let can_apply = solid_count >= 2
                && app.align_reference.is_some_and(|reference| {
                    objects
                        .iter()
                        .any(|(id, _, solid)| *id == reference && *solid)
                });
            if ui
                .add_enabled(
                    can_apply,
                    primary_button(language.text("Apply", "应用"), &app.settings),
                )
                .clicked()
            {
                app.align_selected(app.align_axis, app.align_mode);
            }
            let readiness = if solid_count < 2 {
                language.text(
                    "Add at least two solid objects before Apply",
                    "请先加入至少两个实体对象",
                )
            } else if app.align_reference.is_none() {
                language.text(
                    "Lock one object as the fixed alignment reference",
                    "锁定一个对象作为固定对齐基准",
                )
            } else {
                language.text(
                    "Ready · Min/Max aligns edges · Center aligns centers",
                    "就绪 · 最小/最大对齐边界 · 居中对齐中心",
                )
            };
            ui.label(
                egui::RichText::new(readiness)
                    .small()
                    .color(theme_color(ui, MUTED)),
            );
        }
        ui.add_space(5.0);

        ui.label(
            egui::RichText::new(language.text(
                "Selected objects · click to add/remove · Ctrl+A selects all visible solids",
                "已选对象 · 单击添加/移除 · Ctrl+A 选择全部可见实体",
            ))
            .small()
            .color(theme_color(ui, MUTED)),
        );
        egui::Frame::new()
            .fill(theme_color(ui, egui::Color32::from_rgb(242, 246, 251)))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(9, 6))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("align_selected_objects_scroll")
                    .max_height(180.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (id, name, solid) in &objects {
                            ui.horizontal(|ui| {
                                ui.label(if *solid {
                                    name.clone()
                                } else {
                                    format!("{name} (not solid)")
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let mut locked = app.align_reference == Some(*id);
                                        if ui
                                            .add_enabled(
                                                *solid,
                                                egui::Checkbox::new(
                                                    &mut locked,
                                                    language.text("Lock", "锁定"),
                                                ),
                                            )
                                            .changed()
                                        {
                                            app.align_reference = locked.then_some(*id);
                                            if app.axis_align.enabled {
                                                app.axis_align.fixed_body = app.align_reference;
                                            }
                                        }
                                    },
                                );
                            });
                        }
                        if objects.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    language.text("No objects selected yet", "尚未选择对象"),
                                )
                                .weak(),
                            );
                        }
                    });
            });
        ui.label(
            egui::RichText::new(language.text(
                "The locked object stays in place; all others move to it.",
                "锁定对象保持不动，其他对象向它对齐。",
            ))
            .small()
            .color(theme_color(ui, MUTED)),
        );
        ui.separator();
    }

    // Array 面板（与其它操作面板一致；工具栏 Array 打开）
    if app.array_active {
        ui.label(egui::RichText::new(language.text("Array", "阵列")).strong());
        ui.separator();
        ui.label(language.text("Axes (multi-select):", "轴（可多选）："));
        egui::Grid::new("array_grid")
            .num_columns(4)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new(language.text("Axis", "轴")).weak());
                ui.label(egui::RichText::new(language.text("Dir", "方向")).weak());
                ui.label(egui::RichText::new(language.text("Copies", "数量")).weak())
                    .on_hover_text(language.text(
                        "Total count along this axis, including the original object",
                        "该轴上的总数量，包含原对象",
                    ));
                ui.label(egui::RichText::new(language.text("Spacing", "间距")).weak());
                ui.end_row();
                for a in 0..3usize {
                    ui.checkbox(&mut app.array_axes[a], ["X", "Y", "Z"][a]);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut app.array_positive[a], true, "+");
                        ui.selectable_value(&mut app.array_positive[a], false, "-");
                    });
                    tool_numeric(
                        ui,
                        egui::DragValue::new(&mut app.array_copies[a])
                            .speed(0.1)
                            .range(1..=200),
                    );
                    tool_numeric(
                        ui,
                        length_drag_value(&mut app.array_spacing[a], unit)
                            .speed(0.5)
                            .range(0.1..=100000.0),
                    );
                    ui.end_row();
                }
            });
        if ui
            .add(primary_button(
                language.text("Apply", "应用"),
                &app.settings,
            ))
            .clicked()
        {
            app.array_selected();
        }
        ui.label(
            egui::RichText::new(language.text(
                "Copies includes the original · Multi-axis counts form a cartesian grid",
                "数量包含原对象 · 多轴数量组成笛卡尔网格",
            ))
            .small()
            .weak(),
        );
        ui.separator();
    }

    // Chamfer/Fillet 面板
    if app.chamfer_active {
        section_title(
            ui,
            language.text("Edge treatment", "边处理"),
            Some(language.text("Feature", "特征")),
        );
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut app.chamfer_fillet,
                false,
                language.text("Chamfer", "倒角"),
            );
            ui.selectable_value(
                &mut app.chamfer_fillet,
                true,
                language.text("Fillet", "圆角"),
            );
        });
        ui.horizontal(|ui| {
            ui.label(if app.chamfer_fillet {
                language.text("Radius", "半径")
            } else {
                language.text("Distance", "距离")
            });
            tool_numeric(
                ui,
                length_drag_value(&mut app.chamfer_param, unit)
                    .speed(0.1)
                    .range(0.05..=1000.0),
            );
        });
        let edge_info = match &app.selected_edge {
            Some(e) if language == UiLanguage::English => format!(
                "Edge selected on body {} ({} segments)",
                e.body_id,
                e.segments.len()
            ),
            Some(e) => format!(
                "已选择实体 {} 上的边（{} 个线段）",
                e.body_id,
                e.segments.len()
            ),
            None => language
                .text(
                    "No edge selected - click an edge in the viewport",
                    "尚未选择边——请在工作区中单击一条边",
                )
                .to_string(),
        };
        egui::Frame::new()
            .fill(theme_color(ui, egui::Color32::from_rgb(242, 246, 251)))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 7))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(edge_info)
                        .small()
                        .color(theme_color(ui, MUTED)),
                );
            });
        ui.horizontal(|ui| {
            let apply = ui.add_enabled(
                app.selected_edge.is_some(),
                primary_button(language.text("Apply", "应用"), &app.settings),
            );
            if apply.clicked() {
                app.apply_chamfer();
            }
            let done = ui.add_enabled(
                app.selected_edge.is_some(),
                egui::Button::new(language.text("Done", "完成")),
            );
            if done.clicked() {
                app.chamfer_done();
            }
        });
        if ui
            .button(language.text("Exit Chamfer/Fillet", "退出倒角/圆角"))
            .clicked()
        {
            app.exit_chamfer_tool();
        }
        ui.label(
            egui::RichText::new(language.text(
                "Apply updates this edge. Exit applies the latest parameters and finishes. Double/triple-click also collapses menus and selects a face/object.",
                "应用会更新当前边；退出会应用最新参数并完成。双击/三击还会收起菜单，再选面/对象。",
            ))
                .small()
                .weak(),
        );
        ui.separator();
    }

    if app.mode == Mode::Sketch {
        sketch_properties(app, ui);
        return;
    }
    if app.mode == Mode::Extrude {
        extrude_properties(app, ui);
        return;
    }

    if app.mode == Mode::Revolve {
        revolve_properties(app, ui);
        return;
    }
    if app.mode == Mode::Boolean {
        let ctx = app.boolean_ctx.clone();
        let op_name = ctx
            .as_ref()
            .map(|ctx| match ctx.op {
                BoolOp::Union => language.text("Union", "并集"),
                BoolOp::Subtract => language.text("Subtract", "差集"),
                BoolOp::Intersect => language.text("Intersect", "交集"),
            })
            .unwrap_or(language.text("Boolean", "布尔运算"));
        section_title(ui, op_name, Some(language.text("Boolean", "布尔运算")));
        if ctx.as_ref().is_some_and(|ctx| ctx.op == BoolOp::Union) {
            let operands: Vec<(u64, String)> = app
                .selected
                .iter()
                .filter_map(|id| {
                    app.body(*id).and_then(|body| {
                        (body.visible && body.kind == BodyKind::Solid && body.shape.is_some())
                            .then(|| (*id, body.name.clone()))
                    })
                })
                .collect();
            ui.label(
                egui::RichText::new(if language == UiLanguage::English {
                    format!("{} objects selected", operands.len())
                } else {
                    format!("已选择 {} 个对象", operands.len())
                })
                .strong(),
            );
            egui::Frame::new()
                .fill(theme_color(ui, egui::Color32::from_rgb(242, 246, 251)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    if operands.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                language.text("No solid objects selected", "尚未选择实体对象"),
                            )
                            .weak(),
                        );
                    } else {
                        for (_, name) in &operands {
                            ui.label(name);
                        }
                    }
                });
            ui.label(
                egui::RichText::new(language.text(
                    "Select in Objects or click the viewport · Ctrl/Shift-click for multi-select · Ctrl+A selects all",
                    "在对象列表或工作区中选择 · Ctrl/Shift 单击多选 · Ctrl+A 全选",
                ))
                .small()
                .color(theme_color(ui, MUTED)),
            );
            let mut keep_originals = ctx.as_ref().is_some_and(|ctx| ctx.keep_originals);
            if ui
                .checkbox(
                    &mut keep_originals,
                    language.text("Keep original objects", "保留原对象"),
                )
                .changed()
            {
                if let Some(ctx) = app.boolean_ctx.as_mut() {
                    ctx.keep_originals = keep_originals;
                }
            }
            if ui
                .add_enabled(
                    operands.len() >= 2,
                    primary_button(language.text("Apply", "应用"), &app.settings),
                )
                .clicked()
            {
                app.apply_union_selected();
            }
            if ui
                .button(language.text("Cancel (Esc)", "取消（Esc）"))
                .clicked()
            {
                app.cancel_boolean();
            }
            return;
        }
        let Some(ctx) = ctx else {
            return;
        };
        let tool_label = if ctx.op == BoolOp::Subtract {
            language.text("Subtracting Object (B)", "减去对象（B）")
        } else {
            language.text("Intersecting Object", "求交对象")
        };
        let target_name = ctx
            .target
            .and_then(|id| app.body(id).map(|body| format!("●  {}", body.name.clone())));
        let tool_name = ctx
            .tool
            .and_then(|id| app.body(id).map(|body| format!("●  {}", body.name.clone())));

        ui.label(
            egui::RichText::new(language.text("Original Object (A)", "原对象（A）"))
                .strong()
                .color(theme_color(ui, TEXT)),
        );
        let target_active = ctx.picking == Some(BooleanOperandSlot::Target);
        let target_button = egui::Button::new(
            egui::RichText::new(target_name.unwrap_or_else(|| {
                language
                    .text("Select an object…", "选择一个对象…")
                    .to_string()
            }))
            .color(if ctx.target.is_some() {
                egui::Color32::from_rgb(9, 139, 134)
            } else {
                theme_color(ui, MUTED)
            }),
        )
        .fill(if ctx.target.is_some() {
            egui::Color32::from_rgb(229, 248, 246)
        } else if target_active {
            theme_color(ui, ACCENT_SOFT)
        } else {
            theme_color(ui, PANEL_RAISED)
        })
        .stroke(egui::Stroke::new(
            if target_active { 2.0_f32 } else { 1.0_f32 },
            egui::Color32::from_rgb(23, 176, 168),
        ));
        let target_response = ui.add_sized([ui.available_width(), 34.0], target_button);
        boolean_operand_menu(app, ui, &target_response, BooleanOperandSlot::Target);

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(tool_label)
                .strong()
                .color(theme_color(ui, TEXT)),
        );
        let tool_active = ctx.picking == Some(BooleanOperandSlot::Tool);
        let tool_color = if ctx.op == BoolOp::Subtract {
            egui::Color32::from_rgb(224, 70, 51)
        } else {
            egui::Color32::from_rgb(157, 82, 219)
        };
        let tool_button = egui::Button::new(
            egui::RichText::new(tool_name.unwrap_or_else(|| {
                language
                    .text("Select an object…", "选择一个对象…")
                    .to_string()
            }))
            .color(if ctx.tool.is_some() {
                tool_color
            } else {
                theme_color(ui, MUTED)
            }),
        )
        .fill(if ctx.tool.is_some() {
            if ctx.op == BoolOp::Subtract {
                egui::Color32::from_rgb(253, 235, 232)
            } else {
                egui::Color32::from_rgb(246, 235, 253)
            }
        } else if tool_active {
            theme_color(ui, ACCENT_SOFT)
        } else {
            theme_color(ui, PANEL_RAISED)
        })
        .stroke(egui::Stroke::new(
            if tool_active { 2.0_f32 } else { 1.0_f32 },
            tool_color,
        ));
        let tool_response = ui.add_sized([ui.available_width(), 34.0], tool_button);
        boolean_operand_menu(app, ui, &tool_response, BooleanOperandSlot::Tool);

        if ctx.op == BoolOp::Subtract {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        ctx.target.is_some() || ctx.tool.is_some(),
                        egui::Button::new(language.text("Swap objects", "交换对象")),
                    )
                    .clicked()
                {
                    app.swap_boolean_operands();
                }
                ui.weak(language.text("Result: A − B", "结果：A − B"));
            });
        }

        ui.label(
            egui::RichText::new(if ctx.picking.is_some() {
                language.text(
                    "Click a solid in the viewport or Objects list to fill the highlighted field.",
                    "在工作区或对象列表中单击实体以填入高亮字段。",
                )
            } else {
                language.text(
                    "Operands ready. Review the preview colors, then Apply.",
                    "运算对象已就绪。确认预览颜色后点击应用。",
                )
            })
            .small()
            .color(theme_color(ui, MUTED)),
        );
        let mut keep_originals = ctx.keep_originals;
        if ui
            .checkbox(
                &mut keep_originals,
                language.text("Keep original objects", "保留原对象"),
            )
            .changed()
        {
            if let Some(ctx) = app.boolean_ctx.as_mut() {
                ctx.keep_originals = keep_originals;
            }
        }
        if ui
            .add_enabled(
                ctx.target.is_some() && ctx.tool.is_some(),
                primary_button(language.text("Apply", "应用"), &app.settings),
            )
            .clicked()
        {
            app.apply_boolean_pair();
        }
        if ui
            .button(language.text("Cancel (Esc)", "取消（Esc）"))
            .clicked()
        {
            app.cancel_boolean();
        }
        return;
    }

    if app.selected.is_empty() {
        egui::Frame::new()
            .fill(theme_color(ui, egui::Color32::from_rgb(244, 246, 249)))
            .corner_radius(egui::CornerRadius::same(9))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(language.text("Nothing selected", "未选择任何内容"))
                        .strong()
                        .color(theme_color(ui, TEXT)),
                );
                ui.label(
                    egui::RichText::new(language.text(
                        "Select a body, face, edge, or sketch to inspect it.",
                        "选择实体、面、边或草图以查看属性。",
                    ))
                    .small()
                    .color(theme_color(ui, MUTED)),
                );
            });
        return;
    }
    if app.selected.len() > 1 {
        let count = app.selected.len();
        let selection_title = if language == UiLanguage::English {
            format!("{count} Objects selected")
        } else {
            format!("已选择 {count} 个对象")
        };
        section_title(ui, &selection_title, Some(language.text("Group", "组")));
        let names = app
            .selected
            .iter()
            .filter_map(|id| app.body(*id).map(|body| body.name.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        ui.label(
            egui::RichText::new(names)
                .small()
                .color(theme_color(ui, MUTED)),
        );
        if let Some((min, max)) = app.selected_bounds() {
            let size = max - min;
            let center = (min + max) * 0.5;
            let decimals = unit.decimals();
            ui.label(if language == UiLanguage::English {
                format!(
                    "Group size: {:.*} × {:.*} × {:.*} {}",
                    decimals,
                    unit.from_mm(size.x as f64),
                    decimals,
                    unit.from_mm(size.y as f64),
                    decimals,
                    unit.from_mm(size.z as f64),
                    unit.symbol()
                )
            } else {
                format!(
                    "组尺寸：{:.*} × {:.*} × {:.*} {}",
                    decimals,
                    unit.from_mm(size.x as f64),
                    decimals,
                    unit.from_mm(size.y as f64),
                    decimals,
                    unit.from_mm(size.z as f64),
                    unit.symbol()
                )
            });
            ui.label(if language == UiLanguage::English {
                format!(
                    "Group center: ({}, {}, {})",
                    unit.format_mm(center.x as f64),
                    unit.format_mm(center.y as f64),
                    unit.format_mm(center.z as f64)
                )
            } else {
                format!(
                    "组中心：({}，{}，{})",
                    unit.format_mm(center.x as f64),
                    unit.format_mm(center.y as f64),
                    unit.format_mm(center.z as f64)
                )
            });
        }
        if let Some(gizmo) = app.gizmo {
            ui.separator();
            ui.label(
                egui::RichText::new(match gizmo {
                    GizmoMode::Translate => {
                        language.text("Move all selected objects together", "一起移动所有选中对象")
                    }
                    GizmoMode::Rotate => language.text(
                        "Rotate all selected objects around the shared center",
                        "围绕公共中心旋转所有选中对象",
                    ),
                    GizmoMode::Scale => language.text(
                        "Scale all selected objects around the shared center",
                        "围绕公共中心缩放所有选中对象",
                    ),
                })
                .italics(),
            );
            transform_params(app, ui, gizmo);
        }
        ui.separator();
        if ui
            .button(language.text("Duplicate selected", "复制所选对象"))
            .clicked()
        {
            app.duplicate_selected();
        }
        if ui
            .button(language.text("Delete selected", "删除所选对象"))
            .clicked()
        {
            app.delete_selected();
        }
        return;
    }
    let id = app.selected[0];
    let Some(body) = app.body(id).cloned() else {
        return;
    };

    section_title(
        ui,
        &body.name,
        Some(if body.kind == BodyKind::Curve {
            language.text("Curve", "曲线")
        } else if body.kind == BodyKind::Surface {
            language.text("Surface", "曲面")
        } else if body.is_sketch() {
            language.text("Sketch", "草图")
        } else {
            language.text("Solid", "实体")
        }),
    );

    // 名称
    let mut name = body.name.clone();
    let r = ui.text_edit_singleline(&mut name);
    if r.changed() && !name.is_empty() {
        app.rename_body(id, &name);
    }

    // 颜色
    let color = [
        (body.material.color[0] * 255.0) as u8,
        (body.material.color[1] * 255.0) as u8,
        (body.material.color[2] * 255.0) as u8,
    ];
    let mut color_egui = egui::Color32::from_rgb(color[0], color[1], color[2]);
    let r = ui.color_edit_button_srgba(&mut color_egui);
    if r.changed() {
        app.set_body_color(
            id,
            [
                color_egui.r() as f32 / 255.0,
                color_egui.g() as f32 / 255.0,
                color_egui.b() as f32 / 255.0,
                1.0,
            ],
        );
    }
    let _ = color;

    // 体尺寸与变换提示
    if let Some(shape) = body.shape {
        if let Some((min, max, surface_area, volume)) = app.shape_measurements(shape) {
            let size = max - min;
            let center = (min + max) / 2.0;
            let decimals = unit.decimals();
            ui.label(if language == UiLanguage::English {
                format!(
                    "Size: {:.*} × {:.*} × {:.*} {}",
                    decimals,
                    unit.from_mm(size.x as f64),
                    decimals,
                    unit.from_mm(size.y as f64),
                    decimals,
                    unit.from_mm(size.z as f64),
                    unit.symbol()
                )
            } else {
                format!(
                    "尺寸：{:.*} × {:.*} × {:.*} {}",
                    decimals,
                    unit.from_mm(size.x as f64),
                    decimals,
                    unit.from_mm(size.y as f64),
                    decimals,
                    unit.from_mm(size.z as f64),
                    unit.symbol()
                )
            });
            ui.label(if language == UiLanguage::English {
                format!(
                    "Center: ({}, {}, {})",
                    unit.format_mm(center.x as f64),
                    unit.format_mm(center.y as f64),
                    unit.format_mm(center.z as f64)
                )
            } else {
                format!(
                    "中心：({}，{}，{})",
                    unit.format_mm(center.x as f64),
                    unit.format_mm(center.y as f64),
                    unit.format_mm(center.z as f64)
                )
            });
            ui.label(format!(
                "{}: {}",
                language.text("Surface area", "表面积"),
                format_area(unit, surface_area)
            ));
            ui.label(format!(
                "{}: {}",
                language.text("Volume", "体积"),
                format_volume(unit, volume)
            ));
        }
    }
    if let Some(g) = app.gizmo {
        ui.label(
            egui::RichText::new(match g {
                GizmoMode::Translate => language.text(
                    "Move mode: drag in the viewport, or enter a direction and distance below",
                    "移动模式：在工作区中拖动，或在下方输入方向和距离",
                ),
                GizmoMode::Rotate => language.text(
                    "Rotate mode: drag in the viewport, or pick an axis and angle below",
                    "旋转模式：在工作区中拖动，或在下方选择轴并输入角度",
                ),
                GizmoMode::Scale => language.text(
                    "Scale mode: drag in the viewport, or enter percentages below",
                    "缩放模式：在工作区中拖动，或在下方输入百分比",
                ),
            })
            .italics(),
        );
        ui.separator();
        transform_params(app, ui, g);
    }

    if matches!(body.feature, Feature::EquationSurface { .. }) {
        if ui
            .button(language.text("Edit equation", "编辑方程"))
            .clicked()
        {
            app.start_equation_surface(Some(id));
        }
        if body.kind == BodyKind::Surface
            && ui
                .button(language.text("Extrude surface", "曲面拉伸"))
                .clicked()
        {
            app.start_extrude();
        }
    }
    if body.kind == BodyKind::Curve
        && ui
            .button(language.text("Edit equation", "编辑方程"))
            .clicked()
    {
        app.start_equation_curve(Some(id));
    }
    if body.is_sketch() || (body.kind == BodyKind::Curve && app.profile_sketch(id).is_some()) {
        if ui
            .add_enabled(
                body.is_sketch(),
                egui::Button::new(language.text("Edit Sketch", "编辑草图")),
            )
            .clicked()
        {
            app.edit_sketch(id);
        }
        let region_errors = app.selected_region_validation_errors();
        if !region_errors.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(220, 70, 65),
                language.text(
                    "The selected region cannot be filled. Its outline is retained.",
                    "所选区域暂时无法填充，已保留轮廓。",
                ),
            );
            ui.collapsing(
                language.text("Region validation details", "区域验证详情"),
                |ui| {
                    for error in &region_errors {
                        ui.label(error);
                    }
                },
            );
        }
        ui.horizontal(|ui| {
            if ui.button(language.text("Extrude", "拉伸")).clicked() {
                // 与 E 快捷键完全一致：不清除选区（挤当前高亮区域）
                app.start_extrude();
            }
            if ui.button(language.text("Pocket", "切除")).clicked() {
                // 与 P 快捷键完全一致：不清除选区
                app.start_pocket();
            }
        });
    }

    ui.separator();
    let dup = ui.button(language.text("Duplicate", "复制"));
    if dup.clicked() {
        app.duplicate_selected();
    }
    let del = ui.button(language.text("Delete", "删除"));
    if del.clicked() {
        app.delete_selected();
    }
}

fn measured_face_area(face: &crate::app::FaceHit) -> Option<f64> {
    (face.area_mm2.is_finite() && face.area_mm2 > 0.0).then_some(face.area_mm2)
}

fn triangle_indices_area(mesh: &s33d_kernel::MeshData, indices: &[u32]) -> f64 {
    indices
        .chunks_exact(3)
        .map(|triangle| {
            let a = glam::DVec3::from_array(mesh.positions[triangle[0] as usize].map(f64::from));
            let b = glam::DVec3::from_array(mesh.positions[triangle[1] as usize].map(f64::from));
            let c = glam::DVec3::from_array(mesh.positions[triangle[2] as usize].map(f64::from));
            (b - a).cross(c - a).length() * 0.5
        })
        .sum()
}

pub(crate) fn mesh_surface_area_and_volume(mesh: &s33d_kernel::MeshData) -> (f64, f64) {
    let mut signed_volume = 0.0;
    for triangle in mesh.indices.chunks_exact(3) {
        let a = glam::DVec3::from_array(mesh.positions[triangle[0] as usize].map(f64::from));
        let b = glam::DVec3::from_array(mesh.positions[triangle[1] as usize].map(f64::from));
        let c = glam::DVec3::from_array(mesh.positions[triangle[2] as usize].map(f64::from));
        signed_volume += a.dot(b.cross(c)) / 6.0;
    }
    (
        triangle_indices_area(mesh, &mesh.indices),
        signed_volume.abs(),
    )
}

fn format_area(unit: crate::app::UiUnit, area_mm2: f64) -> String {
    let converted = area_mm2 / unit.millimeters_per_unit().powi(2);
    format!("{:.*} {}²", unit.decimals(), converted, unit.symbol())
}

fn format_volume(unit: crate::app::UiUnit, volume_mm3: f64) -> String {
    let converted = volume_mm3 / unit.millimeters_per_unit().powi(3);
    format!("{:.*} {}³", unit.decimals(), converted, unit.symbol())
}

fn beginner_guidance(app: &mut S33DApp, ui: &mut egui::Ui) {
    if !app.settings.beginner_mode {
        return;
    }
    let language = app.settings.language;
    let (tool, actions): (&str, Vec<&str>) =
        match app.mode {
            Mode::EquationCurve => (
                language.text("3D equation curve", "三维方程曲线"),
                vec![language.text(
                    "Set equations and domain, Apply preview, then Done",
                    "设置方程与定义域，应用预览，再完成",
                )],
            ),
            Mode::OffsetShell => (
                language.text("Exact Offset / Shell", "精确偏移 / 抽壳"),
                vec![language.text(
                    "Select faces, set thickness, Apply preview, then Done",
                    "选面、设置厚度、应用预览，再完成",
                )],
            ),
            Mode::PickPlane => (
                language.text("Sketch plane", "草图平面"),
                vec![
                    language.text("Choose XY, YZ or ZX", "选择 XY、YZ 或 ZX"),
                    language.text("Or enable a linear equation plane", "或启用线性方程平面"),
                    language.text("Esc cancels", "Esc 取消"),
                ],
            ),
            Mode::Sketch => {
                let tool = app
                    .sketch
                    .as_ref()
                    .map(|sketch| sketch.tool)
                    .unwrap_or(SketchTool::Pick);
                let hint = localized_sketch_tool_hint(language, tool);
                (
                    language.text("Sketch", "草图"),
                    vec![
                        hint,
                        language.text(
                            "Adjust the selected geometry on the right",
                            "在右侧调整选中几何",
                        ),
                        language.text(
                            "Done finishes the tool; Exit Sketch commits",
                            "Done 完成工具；Exit Sketch 提交草图",
                        ),
                    ],
                )
            }
            Mode::Extrude => (
                language.text("Extrude / Pocket", "拉伸 / 切除"),
                vec![
                    language.text(
                        "Set depth or drag in the viewport",
                        "设置深度或在工作区拖动",
                    ),
                    language.text(
                        "Apply updates this edge; another edge keeps previous work",
                        "Apply 更新当前边；换边应用会保留之前的修改",
                    ),
                    language.text(
                        "Exit or double/triple-click to return to selection",
                        "退出或双击/三击，返回面/对象选择",
                    ),
                ],
            ),
            Mode::RevolveAxis | Mode::Revolve => (
                language.text("Revolve", "旋转体"),
                vec![
                    language.text("Select an axis and set the angle", "选择旋转轴并设置角度"),
                    language.text("Apply, then Done", "依次点击 Apply、Done"),
                ],
            ),
            Mode::Boolean => (
                app.boolean_ctx.as_ref().map_or(
                    language.text("Boolean", "布尔运算"),
                    |context| match context.op {
                        BoolOp::Union => language.text("Union", "并集"),
                        BoolOp::Subtract => language.text("Subtract", "差集"),
                        BoolOp::Intersect => language.text("Intersect", "交集"),
                    },
                ),
                vec![
                    language.text(
                        "Select the highlighted operand field",
                        "选择高亮的运算对象字段",
                    ),
                    language.text(
                        "Review preview colors, then Apply",
                        "确认预览颜色后点击 Apply",
                    ),
                ],
            ),
            Mode::Split => (
                language.text("Split", "分割"),
                vec![
                    language.text("Choose objects and a cutting plane", "选择物体与切割平面"),
                    language.text("Apply commits the split", "Apply 提交切割"),
                ],
            ),
            Mode::SectionView => (
                language.text("Section View", "剖切视图"),
                vec![
                    language.text("Choose a section plane or equation", "选择剖切面或方程"),
                    language.text(
                        "Esc exits without changing geometry",
                        "Esc 退出且不修改几何",
                    ),
                ],
            ),
            Mode::Mirror => (
                language.text("Mirror", "镜像"),
                vec![
                    language.text("Confirm source objects", "确认源物体"),
                    language.text(
                        "Choose a mirror plane, then Apply",
                        "选择镜像面后点击 Apply",
                    ),
                ],
            ),
            Mode::Insert3D => (
                language.text("Draw 3D", "绘制3D"),
                vec![
                    language.text(
                        "Choose a primitive and adjust its parameters",
                        "选择基础体并调整参数",
                    ),
                    language.text(
                        "Done creates it; Exit Draw 3D finishes",
                        "Done 创建；Exit Draw 3D 结束",
                    ),
                ],
            ),
            Mode::Idle if app.gizmo == Some(GizmoMode::Translate) => (
                language.text("Move", "移动"),
                vec![
                    language.text("Drag the gizmo or enter offsets", "拖动 Gizmo 或输入偏移"),
                    language.text("Apply Move commits the values", "Apply Move 提交数值"),
                ],
            ),
            Mode::Idle if app.gizmo == Some(GizmoMode::Rotate) => (
                language.text("Rotate", "旋转"),
                vec![
                    language.text(
                        "Choose an axis or pick a model edge",
                        "选择坐标轴或拾取模型边",
                    ),
                    language.text("±90° applies immediately", "±90° 会立即生效"),
                ],
            ),
            Mode::Idle if app.gizmo == Some(GizmoMode::Scale) => (
                language.text("Scale", "缩放"),
                vec![
                    language.text(
                        "Scale by percent or to an exact size",
                        "按百分比或具体尺寸缩放",
                    ),
                    language.text("Uniform keeps proportions", "Uniform 保持比例"),
                ],
            ),
            Mode::Idle if app.align_active => (
                language.text("Align", "对齐"),
                vec![
                    language.text("Add two or more objects", "加入两个或更多物体"),
                    language.text(
                        "Lock a reference, choose Axis and Mode, then Apply",
                        "锁定参考物体，选择 Axis、Mode 后 Apply",
                    ),
                ],
            ),
            Mode::Idle if app.array_active => (
                language.text("Array", "阵列"),
                vec![
                    language.text(
                        "Copies includes the original object",
                        "Copies 数量包含原物体",
                    ),
                    language.text(
                        "Choose axes, spacing and count, then Apply",
                        "选择轴、间距和数量后 Apply",
                    ),
                ],
            ),
            Mode::Idle if app.chamfer_active => (
                language.text("Chamfer / Fillet", "倒角 / 圆角"),
                vec![
                    language.text(
                        "Select an edge and adjust distance/radius",
                        "选择边并调整距离或半径",
                    ),
                    language.text(
                        "Apply updates this edge; Done finishes it",
                        "Apply 更新当前边；Done 完成当前边",
                    ),
                    language.text(
                        "Long calculations show progress; Cancel keeps the original model",
                        "耗时计算会显示进度；取消保留原模型",
                    ),
                ],
            ),
            Mode::Idle => (
                language.text("Selection", "选择"),
                vec![
                    language.text(
                        "Single-click edge · double-click surface · triple-click object",
                        "单击边 · 双击面 · 三击物体",
                    ),
                    language.text("Ctrl/Shift adds to the selection", "Ctrl/Shift 添加选择"),
                    language.text("Choose a tool on the left", "从左侧选择工具"),
                ],
            ),
        };
    let mut close_clicked = false;
    egui::Frame::new()
        .fill(app.settings.accent_soft())
        .stroke(egui::Stroke::new(1.0_f32, app.settings.accent()))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {tool}",
                        language.text("Beginner Guide", "新手引导")
                    ))
                    .strong()
                    .color(app.settings.accent()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text(language.text("Turn off Beginner Mode", "关闭新手模式"))
                        .clicked()
                    {
                        close_clicked = true;
                    }
                });
            });
            for action in actions {
                ui.label(format!("• {action}"));
            }
        });
    if close_clicked {
        app.settings.beginner_mode = false;
        app.save_ui_preferences();
    }
    ui.add_space(8.0);
}

fn localized_sketch_tool_hint(language: UiLanguage, tool: SketchTool) -> &'static str {
    if language == UiLanguage::English {
        return tool.hint();
    }
    match tool {
        SketchTool::Trim => "悬停预览，单击删除顶点或交点之间的片段；Ctrl+Z 撤销",
        SketchTool::Freehand => "连续拖动绘制新笔画；切换到 Pick（选择）可移动或缩放已有笔画",
        SketchTool::Text => "单击工作区放置文字输入框，与右边栏同步编辑",
        SketchTool::Pick => "选择草图图元以编辑参数",
        SketchTool::Move => "选择一个或多个草图图元进行移动",
        SketchTool::Rotate => "选择一个或多个草图图元进行旋转",
        SketchTool::Align => "选择至少两个草图图元进行对齐",
        SketchTool::Line => "单击起点和终点绘制直线",
        SketchTool::Arc => "依次单击三个点绘制圆弧",
        SketchTool::Circle => "单击圆心，再单击一点设置半径",
        SketchTool::Rect => "单击两个对角点绘制矩形",
        SketchTool::Polygon => "单击中心，再单击一点设置多边形",
    }
}

/// 两套 Line 参数输入的编辑结果。
enum LineParamEdit {
    None,
    Start([f64; 2]),
    End([f64; 2]),
    /// 保持起点：角度（deg）+ 长度。
    Polar(f64, f64),
}

/// A per-panel focus chain for numeric Sketch fields. Enter commits the active
/// editor through egui, then advances to the next visible field. When there is
/// no next field the caller can transfer focus to its Done button.
#[derive(Default)]
struct SketchEnterFocusChain {
    fields: Vec<egui::Id>,
    entered_at: Option<usize>,
}

impl SketchEnterFocusChain {
    fn track(&mut self, ui: &egui::Ui, response: &egui::Response) -> bool {
        if !response.enabled() {
            return false;
        }
        let index = self.fields.len();
        self.fields.push(response.id);
        let enter_pressed = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let entered = enter_pressed && (response.has_focus() || response.lost_focus());
        if entered && self.entered_at.is_none() {
            self.entered_at = Some(index);
        }
        entered
    }

    /// Returns true when focus should move to the panel's Done button.
    fn finish(self, ui: &mut egui::Ui) -> bool {
        let Some(index) = self.entered_at else {
            return false;
        };
        if let Some(next) = self.fields.get(index + 1) {
            ui.memory_mut(|memory| memory.request_focus(*next));
            false
        } else {
            true
        }
    }
}

/// 渲染两套 Line 参数输入（Set 1 坐标式 / Set 2 角度式），返回编辑动作。
fn line_param_ui(
    ui: &mut egui::Ui,
    start: [f64; 2],
    end: [f64; 2],
    uname: &str,
    vname: &str,
    language: UiLanguage,
    unit: UiUnit,
    locked: Option<usize>,
) -> (LineParamEdit, bool) {
    let mut focus_chain = SketchEnterFocusChain::default();
    let mut start_x = start[0];
    let mut start_y = start[1];
    let mut end_x = end[0];
    let mut end_y = end[1];
    let length = ((end_x - start_x).powi(2) + (end_y - start_y).powi(2)).sqrt();
    let angle_deg = (end_y - start_y).atan2(end_x - start_x).to_degrees();

    // ---- Set 1：起点/终点坐标 + 长度（相关联动）----
    ui.label(
        egui::RichText::new(language.text("Set 1 - coordinates", "方式 1 - 坐标"))
            .small()
            .weak(),
    );
    ui.add_enabled_ui(locked != Some(0), |ui| {
        ui.horizontal(|ui| {
            ui.label(language.text("Start", "起点"));
            let response = tool_numeric(
                ui,
                length_drag_value(&mut start_x, unit)
                    .speed(0.5)
                    .prefix(format!("{uname} ")),
            );
            focus_chain.track(ui, &response);
            let response = tool_numeric(
                ui,
                length_drag_value(&mut start_y, unit)
                    .speed(0.5)
                    .prefix(format!("{vname} ")),
            );
            focus_chain.track(ui, &response);
        });
    });
    ui.add_enabled_ui(locked != Some(1), |ui| {
        ui.horizontal(|ui| {
            ui.label(language.text("End  ", "终点  "));
            let response = tool_numeric(
                ui,
                length_drag_value(&mut end_x, unit)
                    .speed(0.5)
                    .prefix(format!("{uname} ")),
            );
            focus_chain.track(ui, &response);
            let response = tool_numeric(
                ui,
                length_drag_value(&mut end_y, unit)
                    .speed(0.5)
                    .prefix(format!("{vname} ")),
            );
            focus_chain.track(ui, &response);
        });
    });
    let mut len = length;
    ui.horizontal(|ui| {
        ui.label(language.text("Length", "长度"));
        let response = tool_numeric(
            ui,
            length_drag_value(&mut len, unit)
                .speed(0.5)
                .range(0.0..=1e6),
        );
        focus_chain.track(ui, &response);
    });

    // ---- Set 2：起点坐标 + 角度 + 长度 ----
    ui.label(
        egui::RichText::new(language.text("Set 2 - angle", "方式 2 - 角度"))
            .small()
            .weak(),
    );
    ui.add_enabled_ui(locked != Some(0), |ui| {
        ui.horizontal(|ui| {
            ui.label(language.text("Start", "起点"));
            let response = tool_numeric(
                ui,
                length_drag_value(&mut start_x, unit)
                    .speed(0.5)
                    .prefix(format!("{uname} ")),
            );
            focus_chain.track(ui, &response);
            let response = tool_numeric(
                ui,
                length_drag_value(&mut start_y, unit)
                    .speed(0.5)
                    .prefix(format!("{vname} ")),
            );
            focus_chain.track(ui, &response);
        });
    });
    let mut ang = angle_deg;
    ui.horizontal(|ui| {
        ui.label(if language == UiLanguage::English {
            format!("Angle (from {uname} axis)")
        } else {
            format!("角度（从 {uname} 轴起）")
        });
        let response = tool_numeric(ui, egui::DragValue::new(&mut ang).speed(0.5).suffix(" deg"));
        focus_chain.track(ui, &response);
    });
    let mut len2 = length;
    ui.horizontal(|ui| {
        ui.label(language.text("Length", "长度"));
        let response = tool_numeric(
            ui,
            length_drag_value(&mut len2, unit)
                .speed(0.5)
                .range(0.0..=1e6),
        );
        focus_chain.track(ui, &response);
    });

    // 编辑检测（终点 > 长度 > 角度）
    let edit = if (start_x - start[0]).abs() > 1e-6 || (start_y - start[1]).abs() > 1e-6 {
        LineParamEdit::Start([start_x, start_y])
    } else if (end_x - end[0]).abs() > 1e-6 || (end_y - end[1]).abs() > 1e-6 {
        LineParamEdit::End([end_x, end_y])
    } else if (len - length).abs() > 1e-6 {
        LineParamEdit::Polar(angle_deg, len.max(0.0))
    } else if (ang - angle_deg).abs() > 1e-6 {
        LineParamEdit::Polar(ang, len2.max(0.0))
    } else {
        LineParamEdit::None
    };
    (edit, focus_chain.finish(ui))
}

fn world_coordinate_row(
    ui: &mut egui::Ui,
    label: &str,
    point: &mut [f64; 3],
    unit: UiUnit,
    focus_chain: &mut SketchEnterFocusChain,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
            let response = tool_numeric(
                ui,
                length_drag_value(&mut point[axis], unit)
                    .speed(0.5)
                    .prefix(format!("{name} ")),
            );
            focus_chain.track(ui, &response);
        }
    });
}

/// Equation-plane Line editor. The model remains 2D, while all positions are
/// presented as constrained world XYZ coordinates. Editing any component is
/// projected back onto the equation plane, so the three fields remain valid.
fn line_param_ui_world(
    ui: &mut egui::Ui,
    start: [f64; 2],
    end: [f64; 2],
    plane: &PlaneDef,
    language: UiLanguage,
    unit: UiUnit,
    locked: Option<usize>,
) -> (LineParamEdit, bool) {
    let mut focus_chain = SketchEnterFocusChain::default();
    let original_start = plane.to_world(start[0], start[1]);
    let original_end = plane.to_world(end[0], end[1]);
    let mut start_world = original_start;
    let mut end_world = original_end;
    let length = ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2)).sqrt();
    let angle_deg = (end[1] - start[1]).atan2(end[0] - start[0]).to_degrees();

    ui.label(
        egui::RichText::new(language.text("Set 1 - world coordinates", "方式 1 - 世界坐标"))
            .small()
            .weak(),
    );
    ui.add_enabled_ui(locked != Some(0), |ui| {
        world_coordinate_row(
            ui,
            language.text("Start", "起点"),
            &mut start_world,
            unit,
            &mut focus_chain,
        );
    });
    ui.add_enabled_ui(locked != Some(1), |ui| {
        world_coordinate_row(
            ui,
            language.text("End  ", "终点  "),
            &mut end_world,
            unit,
            &mut focus_chain,
        );
    });
    let mut len = length;
    ui.horizontal(|ui| {
        ui.label(language.text("Length", "长度"));
        let response = tool_numeric(
            ui,
            length_drag_value(&mut len, unit)
                .speed(0.5)
                .range(0.0..=1e6),
        );
        focus_chain.track(ui, &response);
    });

    ui.label(
        egui::RichText::new(language.text("Set 2 - in-plane angle", "方式 2 - 平面内角度"))
            .small()
            .weak(),
    );
    ui.add_enabled_ui(locked != Some(0), |ui| {
        world_coordinate_row(
            ui,
            language.text("Start", "起点"),
            &mut start_world,
            unit,
            &mut focus_chain,
        );
    });
    let mut angle = angle_deg;
    ui.horizontal(|ui| {
        ui.label(language.text("Angle (in sketch plane)", "角度（草图平面内）"));
        let response = tool_numeric(
            ui,
            egui::DragValue::new(&mut angle).speed(0.5).suffix(" deg"),
        );
        focus_chain.track(ui, &response);
    });
    let mut len2 = length;
    ui.horizontal(|ui| {
        ui.label(language.text("Length", "长度"));
        let response = tool_numeric(
            ui,
            length_drag_value(&mut len2, unit)
                .speed(0.5)
                .range(0.0..=1e6),
        );
        focus_chain.track(ui, &response);
    });

    let changed = |current: [f64; 3], original: [f64; 3]| {
        (0..3).any(|axis| (current[axis] - original[axis]).abs() > 1e-6)
    };
    let edit = if changed(start_world, original_start) {
        let (u, v) = plane.to_local(start_world);
        LineParamEdit::Start([u, v])
    } else if changed(end_world, original_end) {
        let (u, v) = plane.to_local(end_world);
        LineParamEdit::End([u, v])
    } else if (len - length).abs() > 1e-6 {
        LineParamEdit::Polar(angle_deg, len.max(0.0))
    } else if (angle - angle_deg).abs() > 1e-6 {
        LineParamEdit::Polar(angle, len2.max(0.0))
    } else {
        LineParamEdit::None
    };
    (edit, focus_chain.finish(ui))
}

/// Line 工具参数面板：显示当前段（只此一段），与鼠标双向同步；
/// Next 继续下一条，Done 退出 Line 工具。
fn line_param_panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.label(egui::RichText::new(language.text("Line (parametric)", "直线（参数化）")).strong());
    // 未开始时：等待点击视口设起点，不给默认原点
    let state = app.sketch.as_ref().and_then(|s| s.param_line_state());
    let mut focus_done = false;
    if let Some(pl) = state {
        let coordinate_display = app
            .sketch
            .as_ref()
            .map(|sketch| (sketch.world_xyz_coordinates, sketch.plane))
            .expect("line state requires a sketch");
        let (edit, focus_after_last) = if coordinate_display.0 {
            line_param_ui_world(
                ui,
                pl.start,
                pl.end,
                &coordinate_display.1,
                language,
                app.settings.unit,
                None,
            )
        } else {
            let (uname, vname) = app.sketch_axis_names();
            line_param_ui(
                ui,
                pl.start,
                pl.end,
                &uname,
                &vname,
                language,
                app.settings.unit,
                None,
            )
        };
        match edit {
            LineParamEdit::None => {}
            LineParamEdit::Start(s) => {
                if let Some(sk) = app.sketch.as_mut() {
                    sk.param_line_set_start(s);
                }
                app.bump_scene();
            }
            LineParamEdit::End(e) => {
                if let Some(sk) = app.sketch.as_mut() {
                    sk.param_line_set_end(e);
                }
                app.bump_scene();
            }
            LineParamEdit::Polar(a, l) => {
                if let Some(sk) = app.sketch.as_mut() {
                    sk.param_line_set_polar(a, l);
                }
                app.bump_scene();
            }
        }
        focus_done = focus_after_last;
    } else {
        ui.label(
            egui::RichText::new(language.text(
                "Click in the viewport to set the start point",
                "在工作区中单击以设置起点",
            ))
            .italics()
            .weak(),
        );
    }

    ui.separator();
    let (mut next, mut done, mut exit_sketch) = (false, false, false);
    ui.horizontal(|ui| {
        next = ui.button(language.text("Next", "下一条")).clicked();
        let done_response = ui.button(egui::RichText::new(language.text("Done", "完成")).strong());
        if focus_done {
            done_response.request_focus();
        }
        done = done_response.clicked();
    });
    if ui
        .button(language.text("Exit Sketch", "退出草图"))
        .clicked()
    {
        exit_sketch = true;
    }
    if next {
        if let Some(sk) = app.sketch.as_mut() {
            if sk.param_line_state().is_some() {
                sk.param_line_next();
            }
        }
        app.bump_scene();
    }
    if done {
        // 只完成当前工具工作：退出 Line 工具（不退出 sketch）
        if let Some(sk) = app.sketch.as_mut() {
            sk.param_line_done();
            sk.set_tool(SketchTool::Pick);
        }
        app.bump_scene();
    }
    if exit_sketch {
        app.exit_sketch();
    }
}

/// Pick Properties card fields for a Line (allows editing a chain segment).
fn picked_line_fields(app: &mut S33DApp, ui: &mut egui::Ui, idx: usize) -> bool {
    let language = app.settings.language;
    ui.label(egui::RichText::new(language.text("LINE", "直线")).strong());
    let Some((a, b)) = app.sketch_line_entity(idx) else {
        return false;
    };
    let coordinate_display = app
        .sketch
        .as_ref()
        .map(|sketch| (sketch.world_xyz_coordinates, sketch.plane));
    let locked = app
        .sketch
        .as_ref()
        .and_then(|sketch| sketch.locked_vertex(idx));
    let (edit, focus_done) = if let Some((true, plane)) = coordinate_display {
        line_param_ui_world(ui, a, b, &plane, language, app.settings.unit, locked)
    } else {
        let (uname, vname) = app.sketch_axis_names();
        line_param_ui(
            ui,
            a,
            b,
            &uname,
            &vname,
            language,
            app.settings.unit,
            locked,
        )
    };
    match edit {
        LineParamEdit::None => {}
        LineParamEdit::Start(s) => {
            app.sketch_update_line(idx, s, b);
        }
        LineParamEdit::End(e) => {
            app.sketch_update_line(idx, a, e);
        }
        LineParamEdit::Polar(ang, len) => {
            let rad = ang.to_radians();
            let e = [a[0] + len * rad.cos(), a[1] + len * rad.sin()];
            app.sketch_update_line(idx, a, e);
        }
    }
    focus_done
}

/// 拉伸参数输入界面（深度/对称/确认）。
fn extrude_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let unit = app.settings.unit;
    let title = if app.extrude_ctx.as_ref().map(|c| c.pocket).unwrap_or(false) {
        language.text("Pocket (extruded cut)", "切除（拉伸切除）")
    } else {
        language.text("Extrude", "拉伸")
    };
    ui.label(
        egui::RichText::new(title)
            .strong()
            .color(theme_color(ui, TEXT)),
    );
    ui.separator();
    let (mut confirm, mut cancel) = (false, false);
    let mut depth_changed = false;
    // Pocket 目标体信息（在借用 block 外读取）
    let target_id = app.extrude_ctx.as_ref().and_then(|c| c.target_body);
    let target_name = target_id
        .and_then(|t| app.body(t))
        .map(|b| b.name.clone())
        .unwrap_or_else(|| {
            language
                .text(
                    "none - click a solid in the viewport",
                    "无——请在工作区中单击一个实体",
                )
                .to_string()
        });
    {
        let Some(ctx) = app.extrude_ctx.as_mut() else {
            return;
        };
        if ctx.pocket {
            ui.label(if language == UiLanguage::English {
                format!("Target body: {target_name}")
            } else {
                format!("目标实体：{target_name}")
            });
            let mut through = ctx.through;
            ui.checkbox(&mut through, language.text("Through", "贯穿"));
            if through != ctx.through {
                ctx.through = through;
                depth_changed = true;
            }
            ui.label(
                egui::RichText::new(language.text(
                    "Depth cuts into the target body (from the sketch plane inward)",
                    "深度从草图平面向内切入目标实体",
                ))
                .small()
                .weak(),
            );
        }
        let mut depth = ctx.depth;
        let mut sym = ctx.symmetric;
        ui.horizontal(|ui| {
            ui.label(language.text("Depth", "深度"));
            tool_numeric(ui, length_drag_value(&mut depth, unit).speed(1.0));
        });
        ui.checkbox(&mut sym, language.text("Symmetric", "对称"));
        if (depth - ctx.depth).abs() > 1e-9 || sym != ctx.symmetric {
            ctx.depth = depth;
            ctx.symmetric = sym;
            depth_changed = true;
        }
        if let Some(error) = &ctx.preview_error {
            ui.colored_label(
                theme_error(ui, egui::Color32::from_rgb(190, 74, 48)),
                if language == UiLanguage::English {
                    format!("Preview failed: {error}")
                } else {
                    format!("预览失败：{error}")
                },
            );
        }
        ui.separator();
        ui.horizontal(|ui| {
            confirm = ui
                .add(primary_button(language.text("Done", "完成"), &app.settings))
                .clicked();
            cancel = ui
                .button(language.text("Cancel (Esc)", "取消（Esc）"))
                .clicked();
        });
        ui.label(
            egui::RichText::new(language.text(
                "Drag in the viewport to set depth - release to fix it, then Done",
                "在工作区中拖动设置深度，松开后固定，再点击完成",
            ))
            .small()
            .weak(),
        );
    }
    if depth_changed {
        // 重建 ghost 预览
        let d = app.extrude_ctx.as_ref().map(|c| c.depth).unwrap_or(20.0);
        app.set_extrude_depth(d);
    }
    if confirm {
        app.commit_extrude();
    }
    if cancel {
        app.cancel_extrude();
    }
}

/// 旋转参数输入界面（角度/确认）。
fn revolve_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.label(egui::RichText::new(language.text("Revolve", "旋转体")).strong());
    ui.separator();
    let (mut confirm, mut cancel) = (false, false);
    {
        let Some(ctx) = app.revolve_ctx.as_mut() else {
            return;
        };
        let mut angle = ctx.angle_deg;
        ui.horizontal(|ui| {
            ui.label(language.text("Angle (clockwise +)", "角度（顺时针为正）"));
            tool_numeric(
                ui,
                egui::DragValue::new(&mut angle).speed(1.0).suffix(" deg"),
            );
        });
        ctx.angle_deg = angle;
        ui.separator();
        ui.horizontal(|ui| {
            confirm = ui
                .button(language.text("Confirm (Enter)", "确认（Enter）"))
                .clicked();
            cancel = ui
                .button(language.text("Cancel (Esc)", "取消（Esc）"))
                .clicked();
        });
    }
    if confirm {
        app.commit_revolve();
    }
    if cancel {
        app.cancel_revolve();
    }
}

fn revolve_axis_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    section_title(
        ui,
        language.text("Revolve axis", "旋转轴"),
        Some(language.text("Profile plane", "轮廓平面")),
    );
    ui.label(
        egui::RichText::new(language.text(
            "Choose a local profile axis, or click a straight boundary edge in the viewport.",
            "请选择轮廓局部轴，或在工作区中单击一条直线边界。",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );
    let axis_labels = app
        .revolve_ctx
        .as_ref()
        .map(|ctx| {
            let bindings = world_axis_bindings_for_plane(&ctx.sketch.plane);
            let for_local = |local_axis| {
                bindings
                    .iter()
                    .find(|binding| binding.local_axis == local_axis)
                    .map(|binding| binding.label())
                    .unwrap_or("Axis")
            };
            (for_local(0), for_local(1))
        })
        .unwrap_or(("Axis 1", "Axis 2"));
    ui.horizontal(|ui| {
        let axis_1 = if language == UiLanguage::English {
            format!("{}-axis edge", axis_labels.0)
        } else {
            format!("{} 轴边", axis_labels.0)
        };
        if ui.button(axis_1).clicked() {
            app.revolve_with_plane_axis('X');
        }
        let axis_2 = if language == UiLanguage::English {
            format!("{}-axis edge", axis_labels.1)
        } else {
            format!("{} 轴边", axis_labels.1)
        };
        if ui.button(axis_2).clicked() {
            app.revolve_with_plane_axis('Y');
        }
    });
    ui.label(
        egui::RichText::new(language.text(
            "The selected edge must lie in the bounded surface plane.",
            "选定边必须位于有界表面所在平面内。",
        ))
        .small()
        .weak(),
    );
    if ui
        .button(language.text("Cancel (Esc)", "取消（Esc）"))
        .clicked()
    {
        app.cancel_revolve();
    }
}

fn sketch_align_panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let (mut axis, mut mode, reference, entries, axis_bindings) = {
        let sketch = app.sketch.as_ref().expect("checked by caller");
        let entries = sketch
            .align_selection
            .iter()
            .filter_map(|index| {
                sketch.entities.get(*index).map(|entity| {
                    let kind = match entity.kind() {
                        "line" => language.text("Line", "直线"),
                        "arc" => language.text("Arc", "圆弧"),
                        "circle" => language.text("Circle", "圆"),
                        "rect" => language.text("Rectangle", "矩形"),
                        "polygon" => language.text("Polygon", "多边形"),
                        "freehand" => language.text("Freehand", "手绘"),
                        "text" => language.text("Text", "文字"),
                        other => other,
                    };
                    (*index, format!("{kind} {}", index + 1))
                })
            })
            .collect::<Vec<_>>();
        (
            sketch.align_axis,
            sketch.align_mode,
            sketch.align_reference,
            entries,
            sketch.displayed_world_axes(),
        )
    };

    let subtitle = if language == UiLanguage::English {
        format!("{} entities", entries.len())
    } else {
        format!("{} 个图元", entries.len())
    };
    section_title(
        ui,
        language.text("Sketch Align", "草图对齐"),
        Some(&subtitle),
    );
    ui.horizontal(|ui| {
        ui.label(language.text("Axis:", "轴："));
        for displayed_axis in axis_bindings {
            ui.selectable_value(&mut axis, displayed_axis.world_axis, displayed_axis.label());
        }
    });
    ui.horizontal(|ui| {
        ui.label(language.text("Mode:", "模式："));
        ui.selectable_value(&mut mode, AlignMode::Min, language.text("Min", "最小"));
        ui.selectable_value(
            &mut mode,
            AlignMode::Center,
            language.text("Center", "居中"),
        );
        ui.selectable_value(&mut mode, AlignMode::Max, language.text("Max", "最大"));
    });
    if let Some(sketch) = app.sketch.as_mut() {
        sketch.align_axis = axis;
        sketch.align_mode = mode;
    }

    let can_apply = entries.len() >= 2 && reference.is_some();
    if ui
        .add_enabled(
            can_apply,
            primary_button(language.text("Apply", "应用"), &app.settings),
        )
        .clicked()
    {
        app.sketch_apply_align();
    }
    ui.label(
        egui::RichText::new(if entries.len() < 2 {
            language.text(
                "Click at least two entities in the viewport or Objects list",
                "请在工作区或对象列表中至少选择两个图元",
            )
        } else if reference.is_none() {
            language.text(
                "Lock one entity as the fixed alignment reference",
                "锁定一个图元作为固定对齐基准",
            )
        } else {
            language.text(
                "The locked entity stays fixed; all others move to it",
                "锁定图元保持不动，其他图元向它对齐",
            )
        })
        .small()
        .color(theme_color(ui, MUTED)),
    );
    ui.add_space(5.0);

    egui::Frame::new()
        .fill(theme_color(ui, egui::Color32::from_rgb(242, 246, 251)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(9, 6))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("sketch_align_entities_scroll")
                .max_height(180.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, name) in &entries {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let mut locked = reference == Some(*index);
                                    if ui
                                        .checkbox(&mut locked, language.text("Lock", "锁定"))
                                        .changed()
                                    {
                                        if let Some(sketch) = app.sketch.as_mut() {
                                            sketch.set_align_reference(locked.then_some(*index));
                                        }
                                    }
                                },
                            );
                        });
                    }
                    if entries.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                language
                                    .text("No sketch entities selected yet", "尚未选择草图图元"),
                            )
                            .weak(),
                        );
                    }
                });
        });
    if ui.button(language.text("Done", "完成")).clicked() {
        app.set_sketch_tool(SketchTool::Pick);
    }
    if ui
        .button(language.text("Exit Sketch", "退出草图"))
        .clicked()
    {
        app.exit_sketch();
    }
}

fn sketch_transform_panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let unit = app.settings.unit;
    let Some((
        tool,
        selection_count,
        plane,
        world_xyz,
        displayed_axes,
        move_delta,
        angle,
        pivot,
        custom_pivot_enabled,
        custom_pivot,
    )) = app.sketch.as_ref().map(|sketch| {
        (
            sketch.tool,
            sketch.transform_selection.len(),
            sketch.plane,
            sketch.world_xyz_coordinates,
            sketch
                .displayed_world_axes()
                .into_iter()
                .map(|axis| axis.world_axis)
                .collect::<Vec<_>>(),
            sketch.move_delta,
            sketch.rotate_angle_deg,
            sketch.rotation_pivot(),
            sketch.rotate_custom_pivot,
            sketch.rotate_pivot,
        )
    })
    else {
        return;
    };
    let title = if tool == SketchTool::Move {
        language.text("Sketch Move", "草图移动")
    } else {
        language.text("Sketch Rotate", "草图旋转")
    };
    let subtitle = if language == UiLanguage::English {
        format!("{selection_count} selected")
    } else {
        format!("已选择 {selection_count} 个")
    };
    section_title(ui, title, Some(&subtitle));
    ui.label(
        egui::RichText::new(language.text(
            "Select in the viewport or Objects. Ctrl/Shift adds or removes entities.",
            "在工作区或对象列表中选择；Ctrl/Shift 可添加或移除图元。",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );
    ui.add_space(5.0);

    if tool == SketchTool::Move {
        let u = plane.x_axis();
        let v = plane.y_axis();
        let mut world_delta = (u * move_delta[0] + v * move_delta[1]).to_array();
        let original = world_delta;
        let visible_axes: Vec<usize> = if world_xyz {
            vec![0, 1, 2]
        } else {
            displayed_axes.clone()
        };
        ui.label(egui::RichText::new(language.text("Quick move", "快捷移动")).strong());
        let mut quick_move = None;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(selection_count > 0, |ui| {
                for (direction, delta, label, tip) in [
                    (
                        egui::vec2(-1.0, 0.0),
                        [-1.0, 0.0],
                        "left",
                        "Move left by Step",
                    ),
                    (
                        egui::vec2(1.0, 0.0),
                        [1.0, 0.0],
                        "right",
                        "Move right by Step",
                    ),
                    (egui::vec2(0.0, -1.0), [0.0, 1.0], "up", "Move up by Step"),
                    (
                        egui::vec2(0.0, 1.0),
                        [0.0, -1.0],
                        "down",
                        "Move down by Step",
                    ),
                ] {
                    if solid_triangle_button(ui, direction, tip).clicked() {
                        quick_move = Some((delta, label));
                    }
                }
            });
            ui.separator();
            ui.label(language.text("Step", "步长"));
            tool_numeric(
                ui,
                length_drag_value(&mut app.move_step, unit)
                    .speed(0.1)
                    .range(0.001..=100_000.0),
            );
        });
        if let Some((direction, label)) = quick_move {
            let step = f64::from(app.move_step.max(0.001));
            app.sketch_quick_move([direction[0] * step, direction[1] * step], label);
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new(language.text("Offset", "偏移")).strong());
        for axis in visible_axes {
            ui.horizontal(|ui| {
                ui.label(["X", "Y", "Z"][axis]);
                tool_numeric(
                    ui,
                    length_drag_value(&mut world_delta[axis], unit).speed(0.5),
                );
            });
        }
        if world_delta != original {
            let delta = glam::DVec3::from_array(world_delta);
            app.sketch_set_move_delta([delta.dot(u), delta.dot(v)]);
        }
    } else {
        let mut next_angle = angle;
        ui.horizontal(|ui| {
            ui.label(language.text("Angle (clockwise +)", "角度（顺时针为正）"));
            tool_numeric(
                ui,
                egui::DragValue::new(&mut next_angle).speed(0.5).suffix("°"),
            );
        });
        if (next_angle - angle).abs() > 1e-9 {
            app.sketch_set_rotate_angle(next_angle);
        }
        ui.horizontal(|ui| {
            ui.label(language.text("Quick:", "快捷："));
            if ui
                .add_enabled(selection_count > 0, egui::Button::new("↻ 90°"))
                .on_hover_text(
                    language.text("Rotate clockwise 90° immediately", "立即顺时针旋转 90°"),
                )
                .clicked()
            {
                app.sketch_quick_rotate(90.0);
            }
            if ui
                .add_enabled(selection_count > 0, egui::Button::new("↺ 90°"))
                .on_hover_text(language.text(
                    "Rotate counterclockwise 90° immediately",
                    "立即逆时针旋转 90°",
                ))
                .clicked()
            {
                app.sketch_quick_rotate(-90.0);
            }
        });
        ui.add_space(4.0);
        if let Some(index) = app.sketch.as_ref().and_then(|sketch| {
            (sketch.transform_selection.len() == 1).then(|| sketch.transform_selection[0])
        }) {
            sketch_vertex_lock(app, ui, index);
        }
        let locked = app.sketch.as_ref().map_or(0, |sketch| {
            sketch
                .transform_selection
                .iter()
                .filter(|&&index| sketch.locked_vertex(index).is_some())
                .count()
        });
        if locked != 0 {
            ui.label(language.text(
                "Locked points stay fixed during rotation.",
                "旋转时保持锁定点的位置。",
            ));
        }
        let shared_pivot = locked < selection_count;
        let mut use_custom_pivot = custom_pivot_enabled && shared_pivot;
        if ui
            .add_enabled(
                shared_pivot,
                egui::Checkbox::new(
                    &mut use_custom_pivot,
                    language.text(
                        "Use custom pivot for unlocked objects",
                        "为未锁定图元使用自定义旋转中心",
                    ),
                ),
            )
            .on_hover_text(language.text(
                "Rotate around the axis normal to the sketch plane through this point",
                "绕经过此点且垂直于草图平面的轴旋转",
            ))
            .changed()
        {
            app.sketch_set_rotate_custom_pivot_enabled(use_custom_pivot);
        }
        if use_custom_pivot {
            let mut world = plane.to_world(custom_pivot[0], custom_pivot[1]);
            let original = world;
            let visible_axes = if world_xyz {
                vec![0, 1, 2]
            } else {
                displayed_axes.clone()
            };
            ui.label(
                egui::RichText::new(language.text("Pivot coordinates", "旋转中心坐标")).strong(),
            );
            for axis in visible_axes {
                ui.horizontal(|ui| {
                    ui.label(["X", "Y", "Z"][axis]);
                    tool_numeric(ui, length_drag_value(&mut world[axis], unit).speed(0.5));
                });
            }
            if world != original {
                let (u, v) = plane.to_local(world);
                app.sketch_set_rotate_pivot([u, v]);
            }
            ui.label(
                egui::RichText::new(language.text(
                    "The red target marks the pivot in the viewport",
                    "工作区中的红色标记表示旋转中心",
                ))
                .small()
                .color(theme_color(ui, MUTED)),
            );
        }
        if let Some(pivot) = pivot.filter(|_| shared_pivot) {
            let world = plane.to_world(pivot[0], pivot[1]);
            ui.label(
                egui::RichText::new(format!(
                    "{}  X {}   Y {}   Z {}",
                    if use_custom_pivot {
                        language.text("Active pivot", "当前旋转中心")
                    } else {
                        language.text("Selection center", "选区中心")
                    },
                    unit.format_mm(world[0]),
                    unit.format_mm(world[1]),
                    unit.format_mm(world[2])
                ))
                .small()
                .color(theme_color(ui, MUTED)),
            );
        }
    }

    ui.add_space(5.0);
    let has_change = if tool == SketchTool::Move {
        move_delta[0].abs() > 1e-9 || move_delta[1].abs() > 1e-9
    } else {
        angle.abs() > 1e-9
    };
    if ui
        .add_enabled(
            selection_count > 0 && has_change,
            primary_button(language.text("Apply", "应用"), &app.settings),
        )
        .clicked()
    {
        app.sketch_apply_transform();
    }
    if selection_count == 0 {
        ui.label(
            egui::RichText::new(language.text(
                "Select at least one sketch entity",
                "请至少选择一个草图图元",
            ))
            .small()
            .color(theme_color(ui, MUTED)),
        );
    } else {
        ui.label(
            egui::RichText::new(language.text(
                "Orange dashed geometry is the pending result",
                "橙色虚线几何表示待应用结果",
            ))
            .small()
            .color(theme_color(ui, MUTED)),
        );
    }
    ui.horizontal(|ui| {
        if ui.button(language.text("Done", "完成")).clicked() {
            app.set_sketch_tool(SketchTool::Pick);
        }
        if ui.button(language.text("Reset", "重置")).clicked() {
            if tool == SketchTool::Move {
                app.sketch_set_move_delta([0.0, 0.0]);
            } else {
                app.sketch_set_rotate_angle(0.0);
            }
        }
    });
    if ui
        .button(language.text("Exit Sketch", "退出草图"))
        .clicked()
    {
        app.exit_sketch();
    }
}

fn sketch_text_inputs(
    ui: &mut egui::Ui,
    language: UiLanguage,
    content: &mut String,
    font: &mut String,
) {
    ui.label(language.text("Text content", "文字内容"));
    ui.add(
        egui::TextEdit::multiline(content)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .char_limit(64),
    );
    egui::ComboBox::from_id_salt("outline-font")
        .selected_text(font.as_str())
        .show_ui(ui, |ui| {
            for name in crate::sketch_text::FONTS {
                ui.selectable_value(font, name.to_owned(), name);
            }
        });
    ui.label(
        egui::RichText::new(language.text(
            "Noto Sans SC supports Chinese. Outlines are saved with the project.",
            "Noto Sans SC 支持中文。字形轮廓随项目保存。",
        ))
        .small()
        .color(theme_color(ui, MUTED)),
    );
}

// Both editors share one draft (including invalid input), rather than keeping
// independent sidebar/canvas buffers that can overwrite each other.
fn shared_text_draft(
    app: &S33DApp,
    context: &egui::Context,
    idx: usize,
    text: &s33d_model::sketch::SketchText,
) -> (egui::Id, String, String) {
    let id = egui::Id::new(("shared-sketch-text", app.sketch_edit_body, idx));
    let draft = context.data_mut(|d| d.get_temp::<(String, String, String, String)>(id));
    let (content, font) = match draft {
        Some((content, font, source, source_font))
            if source == text.content && source_font == text.font =>
        {
            (content, font)
        }
        _ => (text.content.clone(), text.font.clone()),
    };
    (id, content, font)
}

fn commit_text_draft(
    app: &mut S33DApp,
    context: &egui::Context,
    idx: usize,
    id: egui::Id,
    content: String,
    font: String,
    previous: &s33d_model::sketch::SketchText,
) {
    let changed = content != previous.content || font != previous.font;
    if changed
        && context
            .data_mut(|d| d.get_temp::<(String, String, String, String)>(id))
            .is_some_and(|draft| {
                draft
                    == (
                        content.clone(),
                        font.clone(),
                        previous.content.clone(),
                        previous.font.clone(),
                    )
            })
    {
        return; // Keep invalid input shared; retry only when it changes.
    }
    let result = if changed {
        app.sketch_replace_text(idx, &content, &font)
    } else {
        Ok(())
    };
    let (source, source_font) = if result.is_ok() {
        (content.clone(), font.clone())
    } else {
        (previous.content.clone(), previous.font.clone())
    };
    if let Some(sketch) = app.sketch.as_mut() {
        sketch.drawing_error = result.err();
        if changed && sketch.drawing_error.is_none() {
            sketch.text_content = content.clone();
            sketch.text_font = font.clone();
        }
    }
    context.data_mut(|d| d.insert_temp(id, (content, font, source, source_font)));
    if changed {
        context.request_repaint();
    }
}

fn draw_sketch_text_editor(app: &mut S33DApp, context: &egui::Context, viewport: egui::Rect) {
    let Some(sketch) = app.sketch.as_ref().filter(|s| s.tool == SketchTool::Text) else {
        return;
    };
    let Some(idx) = sketch.selected_entity else {
        return;
    };
    let Some(s33d_model::SketchEntity::Path {
        origin,
        scale,
        rot,
        text: Some(text),
        ..
    }) = sketch.entities.get(idx)
    else {
        return;
    };
    let text = text.clone();
    let entity = sketch.entities[idx].clone();
    let plane = sketch.plane;
    let (origin, scale, rot) = (*origin, *scale, *rot);
    let vp = app
        .camera
        .proj_matrix(viewport.width() / viewport.height().max(1.0))
        * app.camera.view_matrix();
    let project = |p: [f64; 2]| {
        let clip =
            vp * glam::Vec3::from_array(plane.to_world(p[0], p[1]).map(|v| v as f32)).extend(1.0);
        let ndc = clip.truncate() / clip.w;
        egui::pos2(
            viewport.left() + (ndc.x + 1.0) * viewport.width() * 0.5,
            viewport.top() + (1.0 - ndc.y) * viewport.height() * 0.5,
        )
    };
    let (sin, cos) = rot.to_radians().sin_cos();
    let em_point = |p: [f64; 2]| {
        project([
            origin[0] + scale * (p[0] * cos - p[1] * sin),
            origin[1] + scale * (p[0] * sin + p[1] * cos),
        ])
    };
    let carets = crate::sketch_text::caret_positions(&text.content, &text.font);
    let mut border = egui::Rect::NOTHING;
    for p in &carets {
        border.extend_with(em_point([p[0], p[1] - 0.25]));
        border.extend_with(em_point([p[0] + 0.1, p[1] + 1.0]));
    }
    for (chain, _) in entity.to_chains() {
        for p in chain {
            border.extend_with(project(p));
        }
    }
    border = border.expand(5.0);
    if !border.is_finite() || !viewport.intersects(border) {
        return;
    }
    if text.content.is_empty() {
        border.max.x = border.max.x.max(border.min.x + 90.0);
    }
    let font_pixels = em_point([0.0, 1.0])
        .distance(em_point([0.0, 0.0]))
        .clamp(1.0, 500.0);
    let focus = sketch.text_focus_requested;
    let (id, mut content, font) = shared_text_draft(app, context, idx, &text);
    let input_id = id.with("canvas-input");
    // Enter finishes the text object, but must not swallow an IME candidate
    // confirmation. Shift+Enter remains available for a deliberate new line.
    let mut composing = context
        .data_mut(|d| d.get_temp::<bool>(id.with("ime")))
        .unwrap_or(false);
    let mut ime_event = false;
    context.input(|i| {
        for event in &i.events {
            if let egui::Event::Ime(event) = event {
                ime_event = true;
                match event {
                    egui::ImeEvent::Preedit(value) => composing = !value.is_empty(),
                    egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled => composing = false,
                    _ => {}
                }
            }
        }
    });
    context.data_mut(|d| d.insert_temp(id.with("ime"), composing));
    let done = context.memory(|m| m.has_focus(input_id))
        && !composing
        && !ime_event
        && context.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let cache_id = id.with("outline-fill");
    let cached = context.data_mut(|d| {
        d.get_temp::<(s33d_model::SketchEntity, Result<Vec<[[f64; 2]; 3]>, String>)>(cache_id)
    });
    let triangles = match cached {
        Some((previous, triangles)) if previous == entity => triangles,
        _ => {
            let triangles = crate::sketch_text::filled_triangles(&entity);
            if let Err(error) = &triangles {
                eprintln!("Text preview: {error}");
                app.toast(app.settings.language.text(
                    "Cannot fill this text; its outline is kept",
                    "无法填充此文字，已保留轮廓",
                ));
            }
            context.data_mut(|d| d.insert_temp(cache_id, (entity.clone(), triangles.clone())));
            triangles
        }
    };
    egui::Area::new(egui::Id::new(("sketch-text-editor", idx)))
        .order(egui::Order::Foreground)
        .fixed_pos(border.min)
        .movable(false)
        .show(context, |ui| {
            ui.set_clip_rect(viewport);
            // Use TextEdit only for keyboard/IME/clipboard state. Its UI
            // font is never painted: the displayed glyphs below are the
            // exact outlines used by Extrude, at the exact world position.
            ui.visuals_mut().text_cursor.stroke = egui::Stroke::NONE;
            ui.visuals_mut().selection.bg_fill = egui::Color32::TRANSPARENT;
            let mut output = egui::TextEdit::multiline(&mut content)
                .id(input_id)
                .frame(false)
                .text_color(egui::Color32::TRANSPARENT)
                .font(egui::FontId::proportional(font_pixels))
                .min_size(border.size())
                .desired_rows(1)
                .desired_width(border.width())
                .char_limit(64)
                .return_key(egui::KeyboardShortcut::new(
                    egui::Modifiers::SHIFT,
                    egui::Key::Enter,
                ))
                .show(ui);
            if focus {
                output.response.request_focus();
            }
            if output.response.clicked() {
                if let Some(pointer) = output.response.interact_pointer_pos() {
                    let nearest = carets
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            em_point([a[0], a[1] + 0.35])
                                .distance_sq(pointer)
                                .total_cmp(&em_point([b[0], b[1] + 0.35]).distance_sq(pointer))
                        })
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(nearest),
                        )));
                    output.state.store(context, input_id);
                }
            }
            let mut mesh = egui::Mesh::default();
            for triangle in triangles.as_ref().map(Vec::as_slice).unwrap_or(&[]) {
                let start = mesh.vertices.len() as u32;
                for p in triangle {
                    mesh.colored_vertex(
                        project(*p),
                        if app.settings.appearance == UiAppearance::PorcelainWorkshop {
                            app.settings.appearance.palette().outline
                        } else {
                            egui::Color32::from_rgb(239, 177, 85)
                        },
                    );
                }
                mesh.add_triangle(start, start + 1, start + 2);
            }
            ui.painter().add(egui::Shape::mesh(mesh));
            if output.response.has_focus() {
                if let Some(range) = output.cursor_range {
                    let p = carets[range.primary.ccursor.index.min(carets.len() - 1)];
                    ui.painter().line_segment(
                        [em_point([p[0], p[1] - 0.15]), em_point([p[0], p[1] + 0.85])],
                        egui::Stroke::new(
                            1.0_f32,
                            if app.settings.appearance == UiAppearance::PorcelainWorkshop {
                                app.settings.appearance.palette().sketch_line
                            } else {
                                egui::Color32::WHITE
                            },
                        ),
                    );
                }
            }
            let corners = [
                border.left_top(),
                border.right_top(),
                border.right_bottom(),
                border.left_bottom(),
                border.left_top(),
            ];
            ui.painter().extend(egui::Shape::dashed_line(
                &corners,
                egui::Stroke::new(
                    1.0_f32,
                    if app.settings.appearance == UiAppearance::PorcelainWorkshop {
                        app.settings.appearance.palette().outline
                    } else {
                        egui::Color32::from_rgb(190, 203, 220)
                    },
                ),
                5.0,
                4.0,
            ));
        });
    commit_text_draft(app, context, idx, id, content, font, &text);
    if let Some(sketch) = app.sketch.as_mut() {
        sketch.text_focus_requested = false;
    }
    if done
        && app
            .sketch
            .as_ref()
            .is_some_and(|s| s.drawing_error.is_none())
    {
        context.memory_mut(|m| m.surrender_focus(input_id));
        app.deselect_sketch_entity(idx);
        app.bump_scene();
    }
}

fn sketch_text_error(language: UiLanguage, error: &str) -> String {
    if language == UiLanguage::English {
        return error.to_owned();
    }
    if error.starts_with("Font has no glyph") || error.starts_with("No outline") {
        "该字体不支持输入字符，请改用 Noto Sans SC 或更换字符。".into()
    } else {
        match error {
            "Enter text first" => "请先输入文字",
            "Use at most 64 characters per text object" => "每个文字对象最多 64 个字符",
            "Font size must be greater than 0" => "字号必须为大于零的有效数字",
            "Text is too complex; split it into shorter text objects" => {
                "文字轮廓过于复杂，请拆分成较短的文字对象"
            }
            "Text contains no drawable outlines" => "文字没有可绘制的轮廓",
            _ => "无法生成文字轮廓，请检查内容、字体和字号",
        }
        .into()
    }
}

fn sketch_properties(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    ui.label(egui::RichText::new(language.text("Sketch", "草图")).strong());
    ui.separator();
    if app.sketch.is_none() {
        return;
    }

    if app
        .sketch
        .as_ref()
        .is_some_and(|sketch| sketch.tool == SketchTool::Align)
    {
        sketch_align_panel(app, ui);
        return;
    }

    if app
        .sketch
        .as_ref()
        .is_some_and(|sketch| matches!(sketch.tool, SketchTool::Move | SketchTool::Rotate))
    {
        sketch_transform_panel(app, ui);
        return;
    }

    // Line 工具：两套参数化输入体制（与鼠标绘图双向同步）
    {
        let is_line = app
            .sketch
            .as_ref()
            .map(|s| s.tool == SketchTool::Line)
            .unwrap_or(false);
        if is_line {
            line_param_panel(app, ui);
            ui.separator();
        }
    }

    // Line 工具绘制中：参数面板已在顶部显示，不再显示选中实体编辑（避免两套）
    {
        let is_line_tool = app
            .sketch
            .as_ref()
            .map(|s| s.tool == SketchTool::Line)
            .unwrap_or(false);
        if is_line_tool {
            return;
        }
    }

    // Pick uses an ordered multi-selection. The active/latest entity is the
    // only expanded Properties card; all earlier selections remain folded.
    let selected_entities = app.sketch_selected_entities();
    if let Some(sketch) = app.sketch.as_mut() {
        if sketch.tool == SketchTool::Freehand {
            ui.checkbox(
                &mut sketch.freehand_close,
                language.text("Close each stroke", "自动闭合每一笔"),
            );
            ui.label(language.text(
                "Each drag draws a new stroke, including over existing strokes. Switch to Pick to select, move or resize.",
                "每次拖动都绘制新的一笔，可从已有笔画上起笔。需要选择、移动或缩放时切换到 Pick（选择）。",
            ));
        }
        if sketch.tool == SketchTool::Text && selected_entities.is_empty() {
            sketch_text_inputs(
                ui,
                language,
                &mut sketch.text_content,
                &mut sketch.text_font,
            );
            ui.horizontal(|ui| {
                ui.label(language.text("Font size", "字号"));
                tool_numeric(
                    ui,
                    length_drag_value(&mut sketch.text_size, app.settings.unit)
                        .range(0.01..=10000.0),
                );
            });
            ui.label(language.text(
                "Size is the font em height in the active unit, not the visible capital height.",
                "字号是当前单位下的字体 em 高度，不等于可见字形高度。",
            ));
        }
        if let Some(error) = &sketch.drawing_error {
            ui.colored_label(egui::Color32::RED, sketch_text_error(language, error));
        }
    }
    if selected_entities.is_empty() {
        let tool = app
            .sketch
            .as_ref()
            .map(|sketch| sketch.tool)
            .unwrap_or(SketchTool::Pick);
        let (title, instruction) = match tool {
            SketchTool::Trim => (
                language.text("Trim", "修剪"),
                language.text("Click a highlighted segment to remove it. An isolated curve is removed entirely. Trimmed outlines become separate lines/arcs. Ctrl+Z: undo.", "单击高亮片段剪除；没有分割点的独立线或圆会整条删除。剪开后的轮廓变为独立直线/圆弧。Ctrl+Z 撤销。"),
            ),
            SketchTool::Freehand => (
                language.text("Freehand", "手绘"),
                localized_sketch_tool_hint(language, tool),
            ),
            SketchTool::Text => (
                language.text("Text", "文字"),
                localized_sketch_tool_hint(language, tool),
            ),
            SketchTool::Pick => (
                language.text("Pick", "选择"),
                language.text("Select a sketch entity to edit", "选择要编辑的草图图元"),
            ),
            SketchTool::Move => (
                language.text("Move", "移动"),
                language.text("Select sketch entities to move", "选择要移动的草图图元"),
            ),
            SketchTool::Rotate => (
                language.text("Rotate", "旋转"),
                language.text("Select sketch entities to rotate", "选择要旋转的草图图元"),
            ),
            SketchTool::Arc => (
                language.text("Arc", "圆弧"),
                language.text("Click three points to create an arc", "单击三个点创建圆弧"),
            ),
            SketchTool::Circle => (
                language.text("Circle", "圆"),
                language.text(
                    "Click the center, then set the radius",
                    "先单击圆心，再设置半径",
                ),
            ),
            SketchTool::Rect => (
                language.text("Rectangle", "矩形"),
                language.text("Click two opposite corners", "单击两个对角点"),
            ),
            SketchTool::Polygon => (
                language.text("Polygon", "多边形"),
                language.text(
                    "Click the center, then set its size",
                    "先单击中心，再设置尺寸",
                ),
            ),
            SketchTool::Line => (
                language.text("Line", "直线"),
                language.text(
                    "Click in the viewport to draw a line",
                    "在工作区中单击绘制直线",
                ),
            ),
            SketchTool::Align => (
                language.text("Align", "对齐"),
                language.text("Select sketch entities to align", "选择要对齐的草图图元"),
            ),
        };
        ui.label(egui::RichText::new(title).strong());
        ui.label(
            egui::RichText::new(instruction)
                .small()
                .color(theme_color(ui, MUTED)),
        );
        if ui
            .button(egui::RichText::new(language.text("Done", "完成")).strong())
            .clicked()
        {
            if let Some(sketch) = app.sketch.as_mut() {
                sketch.finish_chain();
                sketch.set_tool(SketchTool::Pick);
            }
            app.bump_scene();
        }
        if ui
            .button(language.text("Exit Sketch", "退出草图"))
            .clicked()
        {
            app.exit_sketch();
        }
        return;
    }
    let active_entity = app.sketch_selected_entity();
    ui.separator();
    if selected_entities.len() > 1 {
        ui.label(
            egui::RichText::new(if language == UiLanguage::English {
                format!("{} sketch objects selected", selected_entities.len())
            } else {
                format!("已选择 {} 个草图对象", selected_entities.len())
            })
            .small()
            .color(theme_color(ui, MUTED)),
        );
    }

    let mut toggle_entity = None;
    let mut delete_entity = None;
    let mut done_entity = None;
    for idx in selected_entities {
        let expanded = active_entity == Some(idx);
        let name = sketch_entity_display_name(app, idx, language);
        let arrow = if expanded { "▾" } else { "▸" };
        let header = ui.add_sized(
            [ui.available_width(), 30.0],
            egui::Button::new(egui::RichText::new(format!("{arrow}  {name}")).strong())
                .selected(expanded),
        );
        if header.clicked() {
            toggle_entity = Some(idx);
        }
        if expanded {
            egui::Frame::new()
                .fill(theme_color(ui, egui::Color32::from_rgb(242, 246, 251)))
                .stroke(egui::Stroke::new(1.0_f32, theme_color(ui, BORDER)))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(9, 7))
                .show(ui, |ui| {
                    let focus_done = sketch_entity_property_fields(app, ui, idx);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button(language.text("Delete", "删除")).clicked() {
                            delete_entity = Some(idx);
                        }
                        let done_response = ui.button(language.text("Done", "完成"));
                        if focus_done {
                            done_response.request_focus();
                        }
                        if done_response.clicked() {
                            done_entity = Some(idx);
                        }
                    });
                });
        }
        ui.add_space(3.0);
    }

    if let Some(index) = toggle_entity {
        app.toggle_sketch_entity_properties(index);
    } else if let Some(index) = delete_entity {
        app.sketch_delete_entity(index);
    } else if let Some(index) = done_entity {
        app.deselect_sketch_entity(index);
    }
    if ui
        .button(language.text("Exit Sketch", "退出草图"))
        .clicked()
    {
        app.exit_sketch();
    }
}

fn sketch_entity_display_name(app: &S33DApp, index: usize, language: UiLanguage) -> String {
    let kind = app
        .sketch_entity_kind(index)
        .unwrap_or_else(|| "Object".to_string());
    let kind = match kind.as_str() {
        "line" => language.text("Line", "直线"),
        "arc" => language.text("Arc", "圆弧"),
        "circle" => language.text("Circle", "圆"),
        "rect" => language.text("Rectangle", "矩形"),
        "polygon" => language.text("Polygon", "多边形"),
        "freehand" => language.text("Freehand", "手绘"),
        "text" => language.text("Text", "文字"),
        _ => language.text("Object", "对象"),
    };
    format!("{kind} {}", index + 1)
}

fn sketch_entity_property_fields(app: &mut S33DApp, ui: &mut egui::Ui, idx: usize) -> bool {
    let language = app.settings.language;
    let unit = app.settings.unit;
    let text = app
        .sketch
        .as_ref()
        .and_then(|s| s.entities.get(idx))
        .and_then(|e| {
            if let s33d_model::SketchEntity::Path {
                text: Some(text), ..
            } = e
            {
                Some(text.clone())
            } else {
                None
            }
        });
    if let Some(text) = text {
        let (id, mut content, mut font) = shared_text_draft(app, ui.ctx(), idx, &text);
        sketch_text_inputs(ui, language, &mut content, &mut font);
        commit_text_draft(app, ui.ctx(), idx, id, content, font, &text);
        ui.separator();
    }
    if app.sketch_line_entity(idx).is_some() {
        sketch_vertex_lock(app, ui, idx);
        return picked_line_fields(app, ui, idx);
    }
    if let Some(kind) = app.sketch_entity_kind(idx) {
        let kind = match kind.as_str() {
            "line" => language.text("LINE", "直线"),
            "arc" => language.text("ARC", "圆弧"),
            "circle" => language.text("CIRCLE", "圆"),
            "rect" => language.text("RECTANGLE", "矩形"),
            "polygon" => language.text("POLYGON", "多边形"),
            "freehand" => language.text("FREEHAND", "手绘"),
            "text" => language.text("TEXT", "文字"),
            _ => language.text("OBJECT", "对象"),
        };
        ui.label(egui::RichText::new(kind).strong());
    }
    sketch_vertex_lock(app, ui, idx);
    if let Some(params) = app.sketch_entity_params(idx) {
        let mut focus_chain = SketchEnterFocusChain::default();
        let mut vals: std::collections::HashMap<&str, f64> = Default::default();
        for (key, label, value, hidden) in &params {
            if *hidden {
                ui.horizontal(|ui| {
                    ui.label(sketch_parameter_label(language, label));
                    let text = if sketch_parameter_is_length(key) {
                        unit.format_mm(*value)
                    } else {
                        format!("{value:.3}")
                    };
                    ui.label(egui::RichText::new(text).weak());
                });
                continue;
            }
            let mut value = *value;
            let response = ui
                .horizontal(|ui| {
                    ui.label(sketch_parameter_label(language, label));
                    if sketch_parameter_is_length(key) {
                        tool_numeric(
                            ui,
                            length_drag_value(&mut value, unit).speed(unit.to_mm(0.5)),
                        )
                    } else {
                        let drag = egui::DragValue::new(&mut value).speed(0.5);
                        let drag = if matches!(*key, "a0" | "a1" | "rot") {
                            drag.suffix(" deg")
                        } else {
                            drag
                        };
                        ui.add(drag)
                    }
                })
                .inner;
            let entered = focus_chain.track(ui, &response);
            if response.changed() || entered {
                vals.insert(key, value);
            }
        }
        if !vals.is_empty() {
            if let Err(error) = app.sketch_apply_edit(idx, &vals) {
                app.toast(error);
            }
        }
        return focus_chain.finish(ui);
    }
    false
}

fn sketch_vertex_lock(app: &mut S33DApp, ui: &mut egui::Ui, index: usize) {
    let Some(sketch) = app.sketch.as_mut() else {
        return;
    };
    let count = sketch.vertex_count(index);
    if count == 0 {
        return;
    }
    let language = app.settings.language;
    let original = sketch.locked_vertex(index);
    let polygon = matches!(
        sketch.entities.get(index),
        Some(s33d_model::SketchEntity::Polygon { .. })
    );
    let mut locked = original;
    let label = |vertex: Option<usize>| match vertex {
        None => language.text("Unlocked", "不锁定").to_string(),
        Some(0) if polygon => language.text("Center", "中心").to_string(),
        Some(0) if count == 2 => language.text("Start", "起点").to_string(),
        Some(1) if count == 2 => language.text("End", "终点").to_string(),
        Some(vertex) => format!("V{}", vertex + usize::from(!polygon)),
    };
    ui.horizontal(|ui| {
        ui.label(if polygon {
            language.text("Locked point", "锁定点")
        } else {
            language.text("Locked vertex", "锁定顶点")
        });
        egui::ComboBox::from_id_salt(("sketch_vertex_lock", index))
            .selected_text(label(locked))
            .show_ui(ui, |ui| {
                for vertex in (0..count).map(Some).chain(std::iter::once(None)) {
                    ui.selectable_value(&mut locked, vertex, label(vertex));
                }
            });
    });
    if locked != original {
        sketch.set_locked_vertex(index, locked);
        app.sync_sketch_history();
        app.bump_scene();
    }
}

fn sketch_parameter_is_length(key: &str) -> bool {
    !matches!(key, "n" | "a0" | "a1" | "rot" | "scale")
}

fn sketch_parameter_label(language: UiLanguage, label: &str) -> &str {
    if language == UiLanguage::English {
        return label;
    }
    match label {
        "Font size" => "字号",
        "Scale factor" => "缩放倍数",
        "Rotation deg" | "Rotation clockwise deg" => "顺时针角度",
        "Position X" => "位置 X",
        "Position Y" => "位置 Y",
        "Position Z" => "位置 Z",
        "Position -X" => "位置 -X",
        "Position -Y" => "位置 -Y",
        "Position -Z" => "位置 -Z",
        "Start X" => "起点 X",
        "Start Y" => "起点 Y",
        "End X" => "终点 X",
        "End Y" => "终点 Y",
        "Center X" => "中心 X",
        "Center Y" => "中心 Y",
        "Length" => "长度",
        "Width" => "宽度",
        "Radius" => "半径",
        "Sides" => "边数",
        "Start deg" => "起始角度",
        "End deg" => "结束角度",
        _ => label,
    }
}

// ================= 3D 视口 =================

fn viewport(app: &mut S33DApp, ui: &mut egui::Ui) {
    if app.drawing.open {
        crate::drawing2d::poll(&mut app.drawing, ui.ctx());
    }
    if app.drawing.show_sheet() {
        if ui.ctx().memory(|m| m.focused().is_none()) && app.project_session.pending.is_none() {
            handle_keys(app, ui);
        }
        app.sync_drawing_preview();
        crate::drawing2d::canvas(
            &mut app.drawing,
            ui,
            app.settings.unit,
            app.settings.language == UiLanguage::ChineseSimplified,
        );
        return;
    }
    let available = ui.available_size();
    // 单次注册视口交互（painter 用于渲染回调）
    let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
    let rect = response.rect;
    app.viewport_rect = rect;
    let controls = draw_viewport_display_controls(app, ui.ctx(), rect);
    let over_controls = ui.input(|i| i.pointer.hover_pos().is_some_and(|p| controls.contains(p)));

    // 交互：相机 + 模式点击
    // egui can stop reporting `hovered` while this widget owns a drag.
    // Keep routing its captured pointer until release (also outside the
    // viewport), otherwise direct manipulation never receives drag updates.
    let draw_3d_captured = app
        .insert_3d_ctx
        .as_ref()
        .is_some_and(|ctx| ctx.direct_drag.is_some());
    let owns_drag = response.drag_started()
        || response.dragged()
        || response.drag_stopped()
        || response.is_pointer_button_down_on()
        || draw_3d_captured;
    if !app.progress_busy() && !over_controls && (response.hovered() || owns_drag) {
        if let Some(pos) = response
            .interact_pointer_pos()
            .or_else(|| response.hover_pos())
        {
            app.on_pointer_move(pos, rect);
            app.update_hover(pos, rect);
        }
    } else {
        app.clear_hover_preview();
    }
    if !app.progress_busy()
        && (response.drag_stopped()
            || (draw_3d_captured && ui.input(|i| i.pointer.primary_released())))
    {
        app.on_pointer_up();
    }
    if !app.progress_busy() && !over_controls && (response.hovered() || owns_drag) {
        app.viewport_pointer(&response);
    }
    if !app.progress_busy()
        && !app.tool_exit_prompt_open()
        && app.mode == Mode::Idle
        && (!over_controls || response.context_menu_opened())
    {
        viewport_surface_menu(app, &response);
    }
    // 键盘（文本输入框聚焦时忽略，避免打字触发快捷键）
    let focused = ui.ctx().memory(|m| m.focused().is_some());
    if !focused
        && app.step_import_job.is_none()
        && app.face_extrude_job.is_none()
        && app.project_session.pending.is_none()
    {
        handle_keys(app, ui);
    }

    // 渲染回调
    app.refresh_scene();
    let scene = app.scene.clone();
    let renderer = app.renderer.clone();
    let cb = ViewportCallback::new(scene, renderer, rect);
    let paint_cb = egui_wgpu::Callback::new_paint_callback(rect, cb);
    painter.add(egui::Shape::Callback(paint_cb));
    draw_viewport_chrome(app, &painter, rect);
    app.paint_section_handle(&painter, rect);
    draw_plane_pick_prompt(app, &painter, rect);
    draw_sketch_viewport_controls(app, ui.ctx(), rect);
    draw_sketch_text_editor(app, ui.ctx(), rect);

    if app.mode == Mode::Idle && !app.chamfer_active {
        if let Some(pointer) = response.hover_pos() {
            let hint = if app.rotate_axis_pick_active {
                app.hover_edge
                    .as_ref()
                    .map(|_| "Click to use this straight edge as rotation axis")
            } else if app.hover_body.is_some() {
                Some("Click edge  ·  Double-click face  ·  Triple-click object")
            } else {
                None
            };
            if let Some(hint) = hint {
                draw_selection_preview_hint(&painter, pointer, hint, rect);
            }
        }
    }

    // Extrude 模式：鼠标旁的实时深度数字
    if app.mode == Mode::Extrude {
        if let Some(pos) = response.hover_pos() {
            if let Some(ctx) = &app.extrude_ctx {
                painter.text(
                    pos + egui::vec2(14.0, -14.0),
                    egui::Align2::LEFT_BOTTOM,
                    app.settings.unit.format_mm(ctx.depth),
                    egui::FontId::proportional(18.0),
                    if app.settings.appearance == UiAppearance::PorcelainWorkshop {
                        app.settings.appearance.palette().outline
                    } else {
                        egui::Color32::from_rgb(120, 180, 255)
                    },
                );
            }
        }
    }

    // Draw 3D placement uses the same compact live measurement treatment as
    // Sketch: first click fixes the center, pointer motion sizes, second click
    // unlocks the precision editor.
    if app.mode == Mode::Insert3D {
        if let (Some(pos), Some(context)) = (response.hover_pos(), app.insert_3d_ctx.as_ref()) {
            match context.stage {
                Draw3DPlacementStage::AwaitBase => draw_selection_preview_hint(
                    &painter,
                    pos,
                    app.settings
                        .language
                        .text("Click to set the base center", "单击确定底部中心点"),
                    rect,
                ),
                Draw3DPlacementStage::Sizing => {
                    if let Some(params) = context.params.as_ref() {
                        let text = format!(
                            "{}\n{}\n{}",
                            primitive_name(app.settings.language, params.kind),
                            draw_3d_size_summary(app.settings.language, app.settings.unit, params),
                            app.settings
                                .language
                                .text("Click again to confirm", "再次单击确认")
                        );
                        draw_sketch_dimension(&painter, pos, &text, rect);
                    }
                }
                Draw3DPlacementStage::Precision if context.direct_drag.is_some() => {
                    if let Some(params) = context.params.as_ref() {
                        draw_sketch_dimension(
                            &painter,
                            pos,
                            &draw_3d_size_summary(app.settings.language, app.settings.unit, params),
                            rect,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // Sketch 模式：尺寸与坐标始终随鼠标显示；吸附到红色关键点时
    // 读数来自精确关键点坐标，而不是近似鼠标位置。
    if app.mode == Mode::Sketch {
        let unit = app.settings.unit;
        if let (Some(pos), Some(text)) = (
            response.hover_pos(),
            app.sketch.as_ref().and_then(|sk| {
                sk.cursor_readout_in(
                    1.0 / unit.millimeters_per_unit(),
                    unit.symbol(),
                    unit.decimals().min(4),
                )
            }),
        ) {
            draw_sketch_dimension(&painter, pos, &text, rect);
        }
        if let (Some(pos), true) = (
            response.hover_pos(),
            app.sketch
                .as_ref()
                .is_some_and(|sketch| sketch.hovering_resize_handle()),
        ) {
            draw_selection_preview_hint(
                &painter,
                pos,
                app.settings
                    .language
                    .text("Drag key point to resize", "拖动关键点调整大小"),
                rect,
            );
        }
    }
    draw_selected_sketch_annotations(app, &painter, rect);
    draw_selected_3d_measurement(app, &painter, rect);

    // 右下角 XYZ 轴系 gizmo（随相机转动；点击轴切换视角；拖拽旋转视角）
    draw_axes_gizmo(app, ui, &painter, rect);

    // Sketch 工作区轴只显示其世界 XYZ 含义；局部 U/V 保持为内部实现。
    draw_sketch_axis_labels(app, &painter, rect);

    // PickPlane 模式：三个平面上的 zx / xy / yz 标签（3D 投影）
    draw_plane_labels(app, &painter, rect);
}

#[derive(Clone, Copy)]
enum SurfaceMenu {
    Planar,
    Cylinder,
}

fn viewport_surface_menu(app: &mut S33DApp, response: &egui::Response) {
    let menu_id = response.id.with("surface_menu_kind");
    let opened = response.context_menu_opened();
    let menu = if response.secondary_clicked() {
        let cylinder = app
            .face_pick
            .clone()
            .is_some_and(|face| app.thread_target_for_face(&face).is_some());
        let menu = if cylinder {
            Some(SurfaceMenu::Cylinder)
        } else {
            app.has_bounded_planar_selection()
                .then_some(SurfaceMenu::Planar)
        };
        response
            .ctx
            .data_mut(|data| data.insert_temp(menu_id, menu));
        menu
    } else if opened {
        response.ctx.data(|data| data.get_temp(menu_id)).flatten()
    } else {
        return;
    };
    if menu.is_none() && !opened {
        return;
    }

    response.context_menu(|ui| {
        let language = app.settings.language;
        match menu {
            Some(SurfaceMenu::Planar) => {
                let actions: [(&str, fn(&mut S33DApp)); 4] = [
                    (
                        language.text("Sketch on this surface", "在此表面创建草图"),
                        S33DApp::sketch_on_selected_profile,
                    ),
                    (
                        language.text("Extrude (E)", "拉伸（E）"),
                        S33DApp::start_extrude,
                    ),
                    (
                        language.text("Pocket (P)", "切除（P）"),
                        S33DApp::start_pocket,
                    ),
                    (language.text("Revolve", "旋转体"), S33DApp::start_revolve),
                ];
                for (label, action) in actions {
                    if ui.button(label).clicked() {
                        if app.has_bounded_planar_selection() {
                            action(app);
                        }
                        ui.close_menu();
                    }
                }
                if ui
                    .button(language.text("Deselect surface (Esc)", "取消选择表面（Esc）"))
                    .clicked()
                {
                    app.deselect_surface();
                    ui.close_menu();
                }
            }
            Some(SurfaceMenu::Cylinder) => {
                if ui
                    .button(language.text("Generate thread", "生成螺纹"))
                    .clicked()
                {
                    if let Some(face) = app.face_pick.clone() {
                        app.begin_thread_on_face(&face);
                    }
                    ui.close_menu();
                }
            }
            None => ui.close_menu(),
        }
    });
}

fn project_world_point(app: &S33DApp, rect: egui::Rect, point: glam::DVec3) -> Option<egui::Pos2> {
    let aspect = (rect.width() / rect.height().max(1.0)).max(1e-4);
    let clip =
        (app.camera.proj_matrix(aspect) * app.camera.view_matrix()) * point.as_vec3().extend(1.0);
    if clip.w <= 1e-6 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.x.abs() > 1.2 || ndc.y.abs() > 1.2 || ndc.z < -1.2 || ndc.z > 1.2 {
        return None;
    }
    Some(egui::pos2(
        rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
        rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
    ))
}

fn draw_selected_sketch_annotations(app: &S33DApp, painter: &egui::Painter, rect: egui::Rect) {
    if app.mode != Mode::Sketch {
        return;
    }
    let Some(sketch) = app.sketch.as_ref() else {
        return;
    };
    let unit = app.settings.unit;
    for index in sketch.selected_entities() {
        let Some(entity) = sketch.entities.get(index) else {
            continue;
        };
        let (anchor, text) = match entity {
            s33d_model::SketchEntity::Path {
                origin,
                scale,
                text,
                ..
            } => (
                *origin,
                if text.is_some() {
                    format!(
                        "{} {}",
                        app.settings.language.text("Font size", "字号"),
                        unit.format_mm(*scale)
                    )
                } else {
                    format!("× {scale:.3}")
                },
            ),
            s33d_model::SketchEntity::Line { a, b } => {
                let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                (
                    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5],
                    format!("L {}", unit.format_mm(length)),
                )
            }
            s33d_model::SketchEntity::Arc { c, r, .. } => (*c, format!("R {}", unit.format_mm(*r))),
            s33d_model::SketchEntity::Circle { c, r } => {
                ([c[0] + *r, c[1]], format!("Ø {}", unit.format_mm(*r * 2.0)))
            }
            s33d_model::SketchEntity::Rect { c1, c2 } => (
                [(c1[0] + c2[0]) * 0.5, (c1[1] + c2[1]) * 0.5],
                format!(
                    "{} × {}",
                    unit.format_mm((c2[0] - c1[0]).abs()),
                    unit.format_mm((c2[1] - c1[1]).abs())
                ),
            ),
            s33d_model::SketchEntity::RotatedRect {
                c, width, height, ..
            } => (
                *c,
                format!("{} × {}", unit.format_mm(*width), unit.format_mm(*height)),
            ),
            s33d_model::SketchEntity::Polygon { c, r, n, .. } => (
                [c[0] + *r, c[1]],
                format!("R {}  ·  {} sides", unit.format_mm(*r), n),
            ),
        };
        let world = sketch.plane.to_world(anchor[0], anchor[1]);
        if let Some(position) = project_world_point(app, rect, glam::DVec3::from_array(world)) {
            draw_sketch_dimension(painter, position, &text, rect);
        }
    }
}

fn draw_selected_3d_measurement(app: &S33DApp, painter: &egui::Painter, rect: egui::Rect) {
    if app.mode != Mode::Idle || app.chamfer_active {
        return;
    }
    if let Some(edge) = app.selected_edge.as_ref() {
        if edge.segments.is_empty() {
            return;
        }
        let mut center = glam::DVec3::ZERO;
        let mut length = 0.0;
        for segment in &edge.segments {
            let a = glam::DVec3::from_array(segment.a.map(f64::from));
            let b = glam::DVec3::from_array(segment.b.map(f64::from));
            center += (a + b) * 0.5;
            length += a.distance(b);
        }
        center /= edge.segments.len() as f64;
        if let Some(position) = project_world_point(app, rect, center) {
            draw_sketch_dimension(
                painter,
                position,
                &format!("L {}", app.settings.unit.format_mm(length)),
                rect,
            );
        }
    } else if let Some(face) = app.face_pick.as_ref() {
        if let (Some(position), Some(area)) = (
            project_world_point(
                app,
                rect,
                glam::DVec3::from_array(face.center.map(f64::from)),
            ),
            measured_face_area(face),
        ) {
            draw_sketch_dimension(
                painter,
                position,
                &format!("A {}", format_area(app.settings.unit, area)),
                rect,
            );
        }
    }
}

/// Put an X/Y/Z label at the positive end of each active sketch axis. The
/// direction comes from the same signed world binding used by Sketch Align, so
/// the viewport, Align panel and bottom-right Gizmo cannot disagree.
fn draw_sketch_axis_labels(app: &S33DApp, painter: &egui::Painter, rect: egui::Rect) {
    if app.mode != Mode::Sketch {
        return;
    }
    let Some(sketch) = app.sketch.as_ref() else {
        return;
    };
    let aspect = (rect.width() / rect.height().max(1.0)).max(1e-4);
    let vp = app.camera.proj_matrix(aspect) * app.camera.view_matrix();
    let project = |point: glam::DVec3| -> Option<egui::Pos2> {
        let clip = vp * point.as_vec3().extend(1.0);
        if clip.w.abs() <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        ))
    };
    let Some(origin) = project(sketch.plane.origin()) else {
        return;
    };
    let label_rect = rect.shrink(24.0);
    let colors = [
        egui::Color32::from_rgb(224, 108, 117),
        egui::Color32::from_rgb(123, 201, 111),
        egui::Color32::from_rgb(97, 160, 240),
    ];
    for displayed_axis in sketch.displayed_world_axes() {
        let local_direction = sketch.plane.x_axis() * displayed_axis.local_direction[0]
            + sketch.plane.y_axis() * displayed_axis.local_direction[1];
        let Some(next) = project(sketch.plane.origin() + local_direction) else {
            continue;
        };
        let direction = next - origin;
        if direction.length_sq() <= 1e-6 {
            continue;
        }
        let direction = direction.normalized();
        let mut minimum = f32::NEG_INFINITY;
        let mut maximum = f32::INFINITY;
        let slabs = [
            (origin.x, direction.x, label_rect.left(), label_rect.right()),
            (origin.y, direction.y, label_rect.top(), label_rect.bottom()),
        ];
        let mut intersects = true;
        for (position, delta, low, high) in slabs {
            if delta.abs() <= 1e-6 {
                intersects &= position >= low && position <= high;
                continue;
            }
            let first = (low - position) / delta;
            let second = (high - position) / delta;
            minimum = minimum.max(first.min(second));
            maximum = maximum.min(first.max(second));
        }
        if !intersects || minimum > maximum || maximum < 0.0 {
            continue;
        }
        let position = origin + direction * (maximum - 5.0).max(minimum.max(0.0));
        painter.text(
            position,
            egui::Align2::CENTER_CENTER,
            displayed_axis.label(),
            egui::FontId::proportional(13.0),
            colors[displayed_axis.world_axis],
        );
    }
}

/// Sketch 视口顶部的两个独立开关：关键点显示与几何磁吸。
fn draw_sketch_viewport_controls(app: &mut S33DApp, context: &egui::Context, viewport: egui::Rect) {
    if app.mode != Mode::Sketch {
        return;
    }
    let language = app.settings.language;
    let position = egui::pos2(viewport.center().x - 145.0, viewport.top() + 14.0);
    egui::Area::new(egui::Id::new("sketch_viewport_controls"))
        .order(egui::Order::Foreground)
        .fixed_pos(position)
        .show(context, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(205))
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    egui::Color32::from_rgb(77, 83, 94),
                ))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.visuals_mut().override_text_color =
                        Some(egui::Color32::from_rgb(225, 230, 238));
                    ui.visuals_mut().selection.bg_fill = egui::Color32::from_rgb(37, 118, 221);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("●")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(247, 48, 56)),
                        );
                        let points = ui
                            .selectable_label(
                                app.sketch_points_visible,
                                language.text("Key points", "关键点"),
                            )
                            .on_hover_text(language.text(
                                "Show sketch endpoints, midpoints and centers",
                                "显示草图端点、中点和中心点",
                            ));
                        if points.clicked() {
                            app.sketch_points_visible = !app.sketch_points_visible;
                            app.bump_scene();
                        }
                        ui.separator();
                        let snap = ui
                            .selectable_label(
                                app.sketch_geometry_snap,
                                language.text("Magnetic snap", "磁性捕捉"),
                            )
                            .on_hover_text(language.text(
                                "Snap new geometry to nearby sketch key points",
                                "将新几何捕捉到附近的草图关键点",
                            ));
                        if snap.clicked() {
                            app.sketch_geometry_snap = !app.sketch_geometry_snap;
                            if !app.sketch_geometry_snap {
                                if let Some(sketch) = app.sketch.as_mut() {
                                    sketch.clear_snap();
                                }
                            }
                            app.bump_scene();
                        }
                    });
                });
        });
}

/// Super3D 风格的紧凑尺寸浮标：深色底、橙色细边、靠近但不遮挡光标。
fn draw_sketch_dimension(
    painter: &egui::Painter,
    pointer: egui::Pos2,
    text: &str,
    viewport: egui::Rect,
) {
    let color = egui::Color32::from_rgb(240, 161, 63);
    let galley = painter.layout(
        text.to_owned(),
        egui::FontId::proportional(12.0),
        color,
        320.0,
    );
    let size = galley.size() + egui::vec2(12.0, 7.0);
    let desired = pointer + egui::vec2(15.0, -size.y - 10.0);
    let min = egui::pos2(
        desired.x.clamp(
            viewport.left() + 4.0,
            (viewport.right() - size.x - 4.0).max(viewport.left() + 4.0),
        ),
        desired.y.clamp(
            viewport.top() + 4.0,
            (viewport.bottom() - size.y - 4.0).max(viewport.top() + 4.0),
        ),
    );
    let label_rect = egui::Rect::from_min_size(min, size);
    painter.rect_filled(
        label_rect,
        egui::CornerRadius::same(4),
        egui::Color32::from_black_alpha(205),
    );
    painter.rect_stroke(
        label_rect,
        egui::CornerRadius::same(4),
        egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgba_unmultiplied(240, 161, 63, 175),
        ),
        egui::StrokeKind::Inside,
    );
    painter.galley(min + egui::vec2(6.0, 3.5), galley, color);
}

fn draw_selection_preview_hint(
    painter: &egui::Painter,
    pointer: egui::Pos2,
    text: &str,
    viewport: egui::Rect,
) {
    let galley = painter.layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(11.0),
        egui::Color32::from_rgb(221, 225, 232),
    );
    let size = galley.size() + egui::vec2(14.0, 8.0);
    let mut pos = pointer + egui::vec2(14.0, 16.0);
    pos.x = pos.x.min(viewport.right() - size.x - 6.0);
    pos.y = pos.y.min(viewport.bottom() - size.y - 6.0);
    let badge = egui::Rect::from_min_size(pos, size);
    painter.rect_filled(
        badge,
        egui::CornerRadius::same(5),
        egui::Color32::from_black_alpha(196),
    );
    painter.rect_stroke(
        badge,
        egui::CornerRadius::same(5),
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(78, 84, 95)),
        egui::StrokeKind::Inside,
    );
    painter.galley(pos + egui::vec2(7.0, 4.0), galley, egui::Color32::WHITE);
}

/// Display-only controls share the viewport but never trigger modeling clicks.
fn draw_viewport_display_controls(
    app: &mut S33DApp,
    ctx: &egui::Context,
    rect: egui::Rect,
) -> egui::Rect {
    let language = app.settings.language;
    egui::Area::new(egui::Id::new("viewport_display_controls"))
        .order(egui::Order::Foreground)
        .default_size(egui::vec2(240.0, 40.0))
        .sense(egui::Sense::hover())
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(rect.right_top() + egui::vec2(-12.0, 12.0))
        .movable(false)
        .constrain_to(rect)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(
                            &mut app.show_grid,
                            language.text("Grid & RGB axes", "网格与 RGB 辅助轴"),
                        )
                        .changed()
                    {
                        app.bump_scene();
                    }
                });
            });
        })
        .response
        .rect
}

/// 视口上的轻量状态胶囊；仅呈现信息，不参与建模状态。
fn draw_viewport_chrome(app: &S33DApp, painter: &egui::Painter, rect: egui::Rect) {
    let projection = if app.camera.ortho {
        app.settings.language.text("Orthographic", "正交")
    } else {
        app.settings.language.text("Perspective", "透视")
    };
    let badge = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(16.0, 16.0),
        egui::vec2(132.0, 28.0),
    );
    painter.rect_filled(
        badge,
        egui::CornerRadius::same(8),
        egui::Color32::from_black_alpha(166),
    );
    painter.rect_stroke(
        badge,
        egui::CornerRadius::same(8),
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(74, 78, 87)),
        egui::StrokeKind::Inside,
    );
    painter.circle_filled(
        badge.left_center() + egui::vec2(14.0, 0.0),
        3.5,
        app.settings.accent(),
    );
    painter.text(
        badge.left_center() + egui::vec2(25.0, 0.0),
        egui::Align2::LEFT_CENTER,
        projection,
        egui::FontId::proportional(12.0),
        egui::Color32::from_rgb(222, 226, 233),
    );

    let hint = app.settings.language.text(
        "MMB Orbit   ·   RMB Pan   ·   Wheel Zoom",
        "中键环绕   ·   右键平移   ·   滚轮缩放",
    );
    let hint_pos = rect.left_bottom() + egui::vec2(16.0, -16.0);
    painter.text(
        hint_pos,
        egui::Align2::LEFT_BOTTOM,
        hint,
        egui::FontId::proportional(11.0),
        if app.settings.appearance == UiAppearance::PorcelainWorkshop {
            app.settings.appearance.palette().muted
        } else {
            egui::Color32::from_rgb(148, 154, 165)
        },
    );
}

/// PickPlane 模式的居中操作提示，避免用户只看到三个无上下文的色块。
fn draw_plane_pick_prompt(app: &S33DApp, painter: &egui::Painter, rect: egui::Rect) {
    if app.mode != Mode::PickPlane {
        return;
    }
    let prompt = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 41.0),
        egui::vec2(286.0, 50.0),
    );
    painter.rect_filled(
        prompt.translate(egui::vec2(0.0, 3.0)),
        egui::CornerRadius::same(10),
        egui::Color32::from_black_alpha(45),
    );
    painter.rect(
        prompt,
        egui::CornerRadius::same(10),
        egui::Color32::from_black_alpha(184),
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(82, 88, 99)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        egui::pos2(prompt.center().x, prompt.center().y - 8.0),
        egui::Align2::CENTER_CENTER,
        "Choose a sketch plane",
        egui::FontId::proportional(14.0),
        egui::Color32::from_rgb(239, 242, 247),
    );
    painter.text(
        egui::pos2(prompt.center().x, prompt.center().y + 10.0),
        egui::Align2::CENTER_CENTER,
        "Click XY, YZ, or ZX to begin",
        egui::FontId::proportional(11.0),
        egui::Color32::from_rgb(157, 166, 180),
    );
}

/// PickPlane 模式：以屏幕对齐的高对比标签标注 XY / YZ / ZX。
/// 标签保持水平，避免透视角度下文字倾斜、难以辨认。
fn draw_plane_labels(app: &S33DApp, painter: &egui::Painter, rect: egui::Rect) {
    if app.mode != Mode::PickPlane {
        return;
    }
    let aspect = (rect.width() / rect.height().max(1.0)).max(1e-4);
    let vp = app.camera.proj_matrix(aspect) * app.camera.view_matrix();
    let h = app.plane_block_size as f32;
    // 顺序与面板和命中检测一致：XY(0) / YZ(1) / ZX(2)。
    let labels = [
        (
            "XY",
            [h * 0.50, h * 0.50, 0.0],
            egui::Color32::from_rgb(97, 160, 240),
        ),
        (
            "YZ",
            [0.0, h * 0.50, h * 0.50],
            egui::Color32::from_rgb(224, 108, 117),
        ),
        (
            "ZX",
            [h * 0.50, 0.0, h * 0.50],
            egui::Color32::from_rgb(123, 201, 111),
        ),
    ];
    for (i, (text, p, color)) in labels.into_iter().enumerate() {
        let clip = vp * glam::vec4(p[0], p[1], p[2], 1.0);
        if clip.w <= 1e-6 {
            continue;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.z < 0.0 || ndc.z > 1.0 {
            continue;
        }
        let sp = egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        );
        let active = app.plane_pick_hover == Some(i);
        let size = if active {
            egui::vec2(66.0, 40.0)
        } else {
            egui::vec2(60.0, 36.0)
        };
        let badge = egui::Rect::from_center_size(sp, size);
        painter.rect_filled(
            badge.translate(egui::vec2(0.0, 2.0)),
            egui::CornerRadius::same(9),
            egui::Color32::from_black_alpha(42),
        );
        painter.rect(
            badge,
            egui::CornerRadius::same(9),
            if active {
                egui::Color32::from_black_alpha(212)
            } else {
                egui::Color32::from_black_alpha(174)
            },
            egui::Stroke::new(if active { 1.6_f32 } else { 1.0_f32 }, color),
            egui::StrokeKind::Inside,
        );
        painter.text(
            sp,
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(if active { 17.0 } else { 16.0 }),
            color,
        );
    }
}

/// 右下角 XYZ 轴系 gizmo：
/// - 三根彩色轴随相机视角实时转动（世界轴在屏幕上的投影）
/// - 点击某根轴：切换到从该轴正方向观察（X→Right、Y→Top、Z→Front）
/// - 在 gizmo 上拖拽：旋转视角
fn draw_axes_gizmo(
    app: &mut S33DApp,
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
) {
    let center = egui::pos2(rect.right() - 58.0, rect.bottom() - 58.0);
    let radius = 34.0f32;
    painter.circle_filled(center, radius + 13.0, egui::Color32::from_black_alpha(142));
    painter.circle_stroke(
        center,
        radius + 13.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(74, 78, 87)),
    );

    // 世界轴 → 屏幕投影（相机 view 矩阵的旋转部分；屏幕 y 向下）
    let view = app.camera.view_matrix();
    let rot = glam::Mat3::from_cols_array(&[
        view.x_axis.x,
        view.x_axis.y,
        view.x_axis.z,
        view.y_axis.x,
        view.y_axis.y,
        view.y_axis.z,
        view.z_axis.x,
        view.z_axis.y,
        view.z_axis.z,
    ]);
    let project = |w: glam::Vec3| -> egui::Vec2 {
        let c = rot * w;
        egui::vec2(c.x, -c.y) * radius
    };

    let axes = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z];
    let colors = [
        egui::Color32::from_rgb(224, 108, 117), // X 红
        egui::Color32::from_rgb(123, 201, 111), // Y 绿
        egui::Color32::from_rgb(97, 160, 240),  // Z 蓝
    ];
    let names = ["X", "Y", "Z"];
    let mut dirs = [egui::Vec2::ZERO; 3];
    for i in 0..3 {
        dirs[i] = project(axes[i]);
        let end = center + dirs[i];
        painter.line_segment([center, end], egui::Stroke::new(2.5_f32, colors[i]));
        if dirs[i].length() > 6.0 {
            let d = dirs[i].normalized();
            let label_pos = end + d * 9.0;
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                names[i],
                egui::FontId::proportional(13.0),
                colors[i],
            );
        }
    }
    // 外圈（装饰 + 拖拽热区）
    painter.circle_stroke(
        center,
        radius + 10.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(88, 93, 103)),
    );

    // 交互区域
    let gizmo_rect = egui::Rect::from_center_size(
        center,
        egui::vec2(2.0 * (radius + 16.0), 2.0 * (radius + 16.0)),
    );
    let resp = ui.interact(
        gizmo_rect,
        egui::Id::new("axes_gizmo"),
        egui::Sense::click_and_drag(),
    );

    // 拖拽 → 旋转视角
    if resp.dragged() {
        let delta = resp.drag_delta();
        app.camera.orbit(delta.x, delta.y, 0.008);
        app.bump_scene();
    }
    // 点击 → 命中轴则切换到该轴视角
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let rel = pos - center;
            let mut best: Option<(usize, f32)> = None;
            for i in 0..3 {
                if dirs[i].length() < 6.0 {
                    continue; // 轴垂直于屏幕，不可点击
                }
                let d = dist_point_seg2(rel, egui::Vec2::ZERO, dirs[i]);
                if d < 12.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((i, d));
                }
            }
            if let Some((i, _)) = best {
                match i {
                    0 => app.set_view(ViewDir::Right),
                    1 => app.set_view(ViewDir::Top),
                    _ => app.set_view(ViewDir::Front),
                }
                app.bump_scene();
            }
        }
    }
}

/// 2D 点 → 线段距离。
fn dist_point_seg2(p: egui::Vec2, a: egui::Vec2, b: egui::Vec2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 > 1e-8 {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = a + ab * t;
    (p - q).length()
}

/// 快捷键（与 Super3D 一致）。
fn handle_keys(app: &mut S33DApp, ui: &mut egui::Ui) {
    if app.tool_exit_prompt_open() || app.progress_busy() {
        return;
    }
    let input = ui.input(|i| i.clone());
    let command = input.modifiers.command || input.modifiers.ctrl;
    if !app.drawing.open {
        let copy = input.events.iter().any(|e| matches!(e, egui::Event::Copy))
            || (command && input.key_pressed(egui::Key::C));
        let cut = input.events.iter().any(|e| matches!(e, egui::Event::Cut))
            || (command && input.key_pressed(egui::Key::X));
        let paste = input
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::Paste(_)))
            || (command && input.key_pressed(egui::Key::V));
        if copy || cut || paste {
            if app.mode == Mode::Idle {
                if cut {
                    app.cut_selected();
                } else if copy {
                    app.copy_selected();
                } else {
                    app.paste_clipboard();
                }
            }
            return; // Modified clipboard keys must never activate Sketch C/V tools.
        }
    }
    if app.drawing.open {
        if input.key_pressed(egui::Key::Escape) {
            if !app.drawing.cancel_interaction() {
                app.drawing.open = false;
            }
        } else if command
            && (input.key_pressed(egui::Key::R)
                || (input.modifiers.shift && input.key_pressed(egui::Key::Z)))
        {
            app.drawing.redo();
        } else if command && input.key_pressed(egui::Key::Z) {
            app.drawing.undo();
        } else if command && input.key_pressed(egui::Key::A) {
            app.select_all_visible_solids();
        } else if command && input.key_pressed(egui::Key::S) {
            save_project(app, input.modifiers.shift);
        } else if command && input.key_pressed(egui::Key::N) {
            app.request_project_action(crate::app::project_session::ProjectAction::New, ui.ctx());
        } else if command && input.key_pressed(egui::Key::O) {
            open_project(app, ui.ctx());
        }
        return;
    }
    if command && input.key_pressed(egui::Key::S) {
        save_project(app, input.modifiers.shift);
    } else if command && input.key_pressed(egui::Key::O) {
        open_project(app, ui.ctx());
    } else if command && input.key_pressed(egui::Key::N) {
        app.request_project_action(crate::app::project_session::ProjectAction::New, ui.ctx());
    } else if command && input.key_pressed(egui::Key::A) {
        app.select_all_visible_solids();
    } else if command && input.modifiers.shift && input.key_pressed(egui::Key::Z) {
        app.redo();
    } else if command && input.key_pressed(egui::Key::Z) {
        app.undo();
    } else if command && input.key_pressed(egui::Key::R) {
        app.redo();
    } else if input.key_pressed(egui::Key::Escape) {
        // Idle 左栏工具统一由 Esc 退出，不打扰当前 object 选择。
        if app.exit_sidebar_tool() {
            return;
        }
        match app.mode {
            Mode::Sketch => app.escape_sketch(),
            Mode::Extrude => app.cancel_extrude(),
            Mode::Revolve | Mode::RevolveAxis => app.cancel_revolve(),
            Mode::Boolean => app.cancel_boolean(),
            Mode::Split => app.cancel_split(),
            Mode::SectionView => app.cancel_section_view(),
            Mode::Mirror => app.cancel_mirror(),
            Mode::Insert3D => app.escape_insert_3d(),
            Mode::OffsetShell => app.cancel_offset_shell(),
            Mode::EquationCurve => app.cancel_equation_curve(),
            Mode::PickPlane => {
                app.mode = Mode::Idle;
                app.bump_scene();
            }
            Mode::Idle => {
                // 清除区域/面选择
                let mut changed = false;
                if !app.selected_regions.is_empty() {
                    app.clear_region_selection();
                    changed = true;
                }
                if app.face_pick.is_some() {
                    app.face_pick = None;
                    changed = true;
                }
                if app.selected_edge.is_some() {
                    app.selected_edge = None;
                    changed = true;
                }
                if changed {
                    app.bump_scene();
                }
            }
        }
    } else if input.key_pressed(egui::Key::Enter) {
        // Enter 只确认当前数值输入；退出 sketch 仅通过 Done 按钮
        match app.mode {
            Mode::Extrude => app.commit_extrude(),
            Mode::Revolve => app.commit_revolve(),
            _ => {}
        }
    } else if input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace) {
        app.delete_selected();
    } else if input.key_pressed(egui::Key::S) && !input.modifiers.command {
        app.begin_plane_pick();
    } else if input.key_pressed(egui::Key::L) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Line);
    } else if input.key_pressed(egui::Key::A) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Arc);
    } else if input.key_pressed(egui::Key::C) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Circle);
    } else if input.key_pressed(egui::Key::R) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Rect);
    } else if input.key_pressed(egui::Key::G) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Polygon);
    } else if input.key_pressed(egui::Key::V) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Pick);
    } else if input.key_pressed(egui::Key::M) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Move);
    } else if input.key_pressed(egui::Key::T) && app.mode == Mode::Sketch {
        app.set_sketch_tool(SketchTool::Rotate);
    } else if input.key_pressed(egui::Key::M) && app.mode == Mode::Idle && !app.selected.is_empty()
    {
        app.toggle_gizmo_tool(GizmoMode::Translate);
    } else if input.key_pressed(egui::Key::T) && app.mode == Mode::Idle && !app.selected.is_empty()
    {
        app.toggle_gizmo_tool(GizmoMode::Rotate);
    } else if input.key_pressed(egui::Key::E) && !input.modifiers.command {
        app.start_extrude();
    } else if (input.key_pressed(egui::Key::P) || input.key_pressed(egui::Key::Q))
        && !input.modifiers.command
    {
        app.start_pocket();
    } else if input.key_pressed(egui::Key::O) && !input.modifiers.command {
        app.start_revolve();
    } else if input.key_pressed(egui::Key::Num1) {
        app.set_view(ViewDir::Front);
    } else if input.key_pressed(egui::Key::Num3) {
        app.set_view(ViewDir::Right);
    } else if input.key_pressed(egui::Key::Num7) {
        app.set_view(ViewDir::Top);
    } else if input.key_pressed(egui::Key::Num0) {
        app.set_view(ViewDir::Iso);
    } else if input.key_pressed(egui::Key::Num5) {
        app.toggle_projection();
    } else if input.key_pressed(egui::Key::F) {
        app.fit_view();
    }
}

// ================= 文件 IO =================

pub(crate) fn save_project(app: &mut S33DApp, save_as: bool) -> bool {
    let current = app.project_session.path.clone();
    let path = if save_as || current.is_none() {
        let mut dialog = file_dialog(app, true).add_filter("S33D project", &["s33d"]);
        if let Some(p) = &current {
            if let Some(parent) = p.parent() {
                dialog = dialog.set_directory(parent);
            }
            if let Some(name) = p.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
        } else {
            dialog = dialog.set_file_name("project.s33d");
        }
        dialog.save_file()
    } else {
        current
    };
    let Some(path) = path else {
        return false;
    };
    app.remember_directory(&path, true);
    match app.save_project_to_path(&path) {
        Ok(()) => {
            app.toast(format!(
                "{}: {}",
                app.settings.language.text("Saved", "已保存"),
                path.display()
            ));
            true
        }
        Err(e) => {
            app.project_session.save_error = Some(e.clone());
            app.toast(format!(
                "{}: {e}",
                app.settings.language.text("Save failed", "保存失败")
            ));
            false
        }
    }
}

fn open_project(app: &mut S33DApp, ctx: &egui::Context) {
    let path = file_dialog(app, false)
        .add_filter("S33D project", &["s33d"])
        .pick_file();
    if let Some(p) = path {
        app.remember_directory(&p, false);
        app.request_project_action(crate::app::project_session::ProjectAction::Open(p), ctx);
    }
}

pub(crate) fn open_project_path(app: &mut S33DApp, path: &std::path::Path) {
    app.remember_directory(path, false);
    match app.open_project_responsive(path) {
        Ok(()) if app.model_job.is_some() => {}
        Ok(()) => app.toast(format!("Opened {}", path.display())),
        Err(e) => app.toast(format!("Open failed: {e}")),
    }
}

/// 统一模型导入：按扩展名分发到对应格式读取器。
fn import_model(app: &mut S33DApp) {
    let path = file_dialog(app, false)
        .add_filter(
            "3D Models",
            &[
                "stp", "step", "igs", "iges", "stl", "obj", "ply", "glb", "gltf", "3mf", "sldprt",
                "sldasm", "slddrw", "x_t", "x_b",
            ],
        )
        .add_filter("STEP", &["stp", "step"])
        .add_filter("IGES", &["igs", "iges"])
        .add_filter(
            "SolidWorks / Parasolid (conversion required)",
            &["sldprt", "sldasm", "slddrw", "x_t", "x_b"],
        )
        .add_filter("3MF", &["3mf"])
        .add_filter("STL", &["stl"])
        .add_filter("OBJ", &["obj"])
        .add_filter("PLY", &["ply"])
        .add_filter("GLB / glTF", &["glb", "gltf"])
        .pick_file();
    let Some(p) = path else { return };
    app.remember_directory(&p, false);
    import_model_path(app, &p);
}

pub(crate) fn import_model_path(app: &mut S33DApp, p: &std::path::Path) {
    let path_str = p.to_string_lossy().to_string();
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if app.background_modeling
        && matches!(ext.as_str(), "stl" | "obj" | "ply" | "glb" | "gltf" | "3mf")
    {
        if app.progress_busy() {
            return;
        }
        if !app.finish_creation_for_tool_switch() {
            return;
        }
        app.exit_sidebar_tool();
        if app.progress_busy() {
            return;
        }
        let path = p.to_path_buf();
        if let Err(error) = app.start_model_job(
            ("Import model", "导入模型"),
            false,
            None,
            move |worker| {
                worker
                    .report_model_progress(
                        (
                            "Reading mesh and preparing display…",
                            "正在读取网格并准备显示…",
                        ),
                        None,
                    )
                    .map_err(|e| ("cancelled".into(), e))?;
                let before = worker.doc.bodies.len();
                import_model_path(worker, &path);
                if worker.doc.bodies.len() == before {
                    return Err((
                        "file".into(),
                        worker
                            .toasts
                            .last()
                            .map(|t| t.0.clone())
                            .unwrap_or_else(|| "No objects imported".into()),
                    ));
                }
                Ok(serde_json::Value::Null)
            },
        ) {
            app.toast(error);
        }
        return;
    }
    let result = match ext.as_str() {
        "stp" | "step" | "igs" | "iges" => {
            app.start_step_import(path_str);
            return;
        }
        "stl" => app.kernel.import_stl(&path_str),
        "obj" => app.kernel.import_obj(&path_str),
        "ply" => app.kernel.import_ply(&path_str),
        "3mf" => app.kernel.import_3mf(&path_str),
        "glb" | "gltf" => app.kernel.import_glb(&path_str),
        "sldprt" | "sldasm" | "slddrw" | "x_t" | "x_b" => {
            app.toast(crate::startup::native_cad_conversion_hint(
                app.settings.language,
            ));
            return;
        }
        _ => {
            app.toast(format!("Unsupported format: .{ext}"));
            return;
        }
    };
    match result {
        Ok(ids) => {
            let n = app.doc.bodies.len() + 1;
            let mut imported_body_ids = Vec::with_capacity(ids.len());
            if !ids.is_empty() {
                // A multi-shape file is one import command and therefore one
                // Undo/Redo transaction.
                app.begin_document_change();
            }
            for (i, shape) in ids.iter().enumerate() {
                let id = app.doc.alloc_id();
                app.doc.add_body(s33d_model::Body {
                    id,
                    name: format!("Imported {}", n + i),
                    kind: if app
                        .kernel
                        .exact_shape(*shape)
                        .is_some_and(|s| s.solid_count == 0)
                    {
                        BodyKind::Surface
                    } else {
                        BodyKind::Solid
                    },
                    feature: s33d_model::Feature::Imported {
                        path: path_str.clone(),
                    },
                    shape: Some(*shape),
                    material: s33d_model::Material {
                        color: crate::app::palette_color(n + i),
                        ..Default::default()
                    },
                    visible: true,
                });
                imported_body_ids.push(id);
            }
            // 导入结果立即成为当前 object 选择；多 shape 文件默认全选本次新增 objects。
            app.exit_sidebar_tool();
            app.selected = imported_body_ids;
            app.clear_region_selection();
            app.face_pick = None;
            app.selected_edge = None;
            app.hover_body = None;
            app.hover_face = None;
            app.hover_edge = None;
            app.toast(format!(
                "Imported and selected {} object(s) (.{ext})",
                ids.len()
            ));
            app.record_history(format!(
                "[sel] selected {} newly imported object(s)",
                ids.len()
            ));
            app.bump_scene();
            // 导入后：标准 iso 斜上方视角 + 自动适配视野与缩放
            app.camera.set_view(crate::camera::ViewDir::Iso);
            app.fit_view();
        }
        Err(e) => app.toast(format!("Import failed: {e}")),
    }
}

/// 统一导出：按格式名调用内核对应写出器。
pub(crate) fn file_dialog(app: &S33DApp, output: bool) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new();
    let directory = if output {
        &app.settings.last_output_directory
    } else {
        &app.settings.last_input_directory
    };
    if let Some(path) = directory.as_ref().filter(|p| p.is_dir()) {
        dialog.set_directory(path)
    } else {
        dialog
    }
}

const MODEL_EXPORT_FILTERS: &[(&str, &[&str])] = &[
    ("STEP", &["step", "stp"]),
    ("STL", &["stl"]),
    ("OBJ", &["obj"]),
    ("PLY", &["ply"]),
    ("GLB", &["glb"]),
];

fn export_project_name(name: &str) -> String {
    let safe = name.replace(
        |ch: char| {
            ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        },
        "_",
    );
    let safe = safe.trim().trim_end_matches(['.', ' ']);
    if safe.is_empty() {
        "Untitled Project".into()
    } else {
        safe.into()
    }
}

fn export_dialog(app: &mut S33DApp) {
    let model = app
        .selected
        .first()
        .copied()
        .filter(|id| app.body(*id).and_then(|b| b.shape).is_some());
    let drawing = app.drawing.views.len() == 3;
    if model.is_none() && !drawing {
        app.toast(app.settings.language.text(
            "Select an object or generate a drawing to export",
            "请选择对象或先生成工程图",
        ));
        return;
    }
    let mut dialog = file_dialog(app, true);
    // The first filter and its first suffix set the Windows native default.
    if model.is_some() {
        for &(label, extensions) in MODEL_EXPORT_FILTERS {
            dialog = dialog.add_filter(label, extensions);
        }
    }
    if drawing {
        for (label, ext) in [
            ("SVG drawing", "svg"),
            ("DXF drawing", "dxf"),
            ("PDF drawing", "pdf"),
            ("EPS drawing", "eps"),
        ] {
            dialog = dialog.add_filter(label, &[ext]);
        }
    }
    let Some(path) = dialog
        .set_file_name(export_project_name(&app.project_name))
        .save_file()
    else {
        return;
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    app.remember_directory(&path, true);
    match ext.as_str() {
        "svg" | "dxf" | "pdf" | "eps" if drawing => {
            crate::drawing2d::export_to_path(app, &path, &ext)
        }
        "stp" | "step" | "stl" | "obj" | "ply" | "glb" if model.is_some() => {
            let fmt = if ext == "stp" { "step" } else { &ext };
            if let Err(error) = app.start_export_job(model.unwrap(), fmt, path) {
                app.toast(error);
            }
        }
        _ => app.toast(app.settings.language.text(
            "Choose a supported output format and matching file extension",
            "请选择支持的输出格式和对应文件扩展名",
        )),
    }
}

// ================= toast =================

fn boolean_operand_menu(
    app: &mut S33DApp,
    ui: &mut egui::Ui,
    response: &egui::Response,
    slot: BooleanOperandSlot,
) {
    let popup_id = response.id.with("available_boolean_objects");
    if response.clicked() {
        app.begin_boolean_operand_pick(slot);
        ui.memory_mut(|memory| memory.toggle_popup(popup_id));
    }
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        response,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            let language = app.settings.language;
            let available: Vec<_> = app
                .doc
                .bodies
                .iter()
                .filter(|body| body.visible && body.kind == BodyKind::Solid && body.shape.is_some())
                .map(|body| (body.id, body.name.clone()))
                .collect();
            ui.weak(language.text("Available objects", "可用对象"));
            if available.is_empty() {
                ui.label(language.text("No visible solids", "没有可见实体"));
            }
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for (id, name) in available {
                        let selected = app.boolean_ctx.as_ref().and_then(|ctx| match slot {
                            BooleanOperandSlot::Target => ctx.target,
                            BooleanOperandSlot::Tool => ctx.tool,
                        }) == Some(id);
                        if ui
                            .selectable_label(selected, format!("{name}  (#{id})"))
                            .clicked()
                        {
                            app.begin_boolean_operand_pick(slot);
                            app.boolean_click(id, false);
                            ui.memory_mut(|memory| memory.close_popup());
                        }
                    }
                });
        },
    );
}

fn toasts(app: &mut S33DApp, ctx: &egui::Context) {
    let now = std::time::Instant::now();
    app.toasts
        .retain(|(_, t)| now.duration_since(*t).as_secs() < 4);
    if app.toasts.is_empty() {
        return;
    }
    egui::Window::new("notifications")
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -48.0))
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(34, 41, 51, 242))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(14, 9))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 5],
                    blur: 18,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(44),
                }),
        )
        .show(ctx, |ui| {
            for (msg, _) in &app.toasts {
                ui.label(
                    egui::RichText::new(localized_hint(app.settings.language, msg))
                        .color(egui::Color32::WHITE),
                );
            }
        });
}

// 让未使用的导入保持可用（egui_wgpu 通过 ui.painter 使用）
#[allow(unused_imports)]
use egui_wgpu;

#[cfg(test)]
mod objects_context_tests {
    use super::*;

    fn frame(
        app: &mut S33DApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        fn texts(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, out);
                    }
                }
                egui::epaint::Shape::Text(text) => out.push((
                    text.galley.job.text.clone(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                )),
                _ => {}
            }
        }
        let output = ctx.run(
            egui::RawInput {
                modifiers: events
                    .iter()
                    .find_map(|event| match event {
                        egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                        _ => None,
                    })
                    .unwrap_or_default(),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 700.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| objects_list(app, ui));
                object_color_window(app, ctx);
            },
        );
        let mut out = Vec::new();
        for shape in output.shapes {
            texts(&shape.shape, &mut out);
        }
        out
    }

    fn click(app: &mut S33DApp, ctx: &egui::Context, pos: egui::Pos2, button: egui::PointerButton) {
        click_with_modifiers(app, ctx, pos, button, egui::Modifiers::default());
    }

    fn click_with_modifiers(
        app: &mut S33DApp,
        ctx: &egui::Context,
        pos: egui::Pos2,
        button: egui::PointerButton,
        modifiers: egui::Modifiers,
    ) {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                },
            ],
        );
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers,
            }],
        );
    }

    #[test]
    fn objects_context_hidden_rows_support_shift_selection_and_batch_show() {
        let ctx = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = S33DApp::new(&creation);
        app.settings.language = UiLanguage::English;
        app.mode = Mode::Idle;
        for id in 1..=5 {
            app.doc.add_body(s33d_model::Body {
                id,
                name: format!("Hidden range {id}"),
                kind: BodyKind::Solid,
                feature: Feature::Imported {
                    path: String::new(),
                },
                shape: None,
                material: Default::default(),
                visible: id % 2 == 0,
            });
        }
        frame(&mut app, &ctx, vec![]);
        let rows = frame(&mut app, &ctx, vec![]);
        let row = |id| {
            rows.iter()
                .find(|(text, _)| text == &format!("Hidden range {id} (solid)"))
                .unwrap()
                .1
                .center()
        };
        click(&mut app, &ctx, row(1), egui::PointerButton::Primary);
        assert_eq!(app.selected, vec![1]);
        click_with_modifiers(
            &mut app,
            &ctx,
            row(5),
            egui::PointerButton::Primary,
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(app.selected, vec![1, 2, 3, 4, 5]);
        assert_eq!(app.object_list_anchor, Some(1));
        for id in [1, 3, 5] {
            assert!(!app.doc.body(id).unwrap().visible);
        }
        click(&mut app, &ctx, row(3), egui::PointerButton::Secondary);
        let menu = frame(&mut app, &ctx, vec![]);
        assert!(menu.iter().any(|(text, _)| text == "5 objects selected"));
        let visible = menu
            .iter()
            .rev()
            .find(|(text, _)| text == "Visible")
            .unwrap()
            .1
            .center();
        click(&mut app, &ctx, visible, egui::PointerButton::Primary);
        assert!(app.doc.bodies.iter().all(|body| body.visible));
        assert_eq!(app.selected, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn objects_context_menu_and_color_dialog_keep_the_selected_group() {
        let ctx = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = S33DApp::new(&creation);
        app.settings.language = UiLanguage::English;
        app.mode = Mode::Idle;
        for id in 1..=3 {
            app.doc.add_body(s33d_model::Body {
                id,
                name: format!("Row {id}"),
                kind: BodyKind::Solid,
                feature: Feature::Imported {
                    path: String::new(),
                },
                shape: None,
                material: Default::default(),
                visible: true,
            });
        }
        app.selected = vec![2, 3];
        frame(&mut app, &ctx, vec![]);
        let rows = frame(&mut app, &ctx, vec![]);
        let row = rows
            .iter()
            .find(|(text, _)| text == "Row 1 (solid)")
            .unwrap()
            .1
            .center();
        click(&mut app, &ctx, row, egui::PointerButton::Secondary);
        let menu = frame(&mut app, &ctx, vec![]);
        assert!(
            menu.iter().any(|(text, _)| text == "2 objects selected"),
            "{menu:?}"
        );
        assert!(!menu
            .iter()
            .any(|(text, _)| text == "Rename" || text == "Rename object"));
        for label in ["Visible", "Color", "Delete"] {
            assert!(menu.iter().any(|(text, _)| text == label));
        }
        assert_eq!(app.selected, vec![2, 3]);
        let choose = menu
            .iter()
            .find(|(text, _)| text == "Choose…")
            .unwrap()
            .1
            .center();
        click(&mut app, &ctx, choose, egui::PointerButton::Primary);
        assert_eq!(app.object_color_editor.as_ref().unwrap().0, vec![2, 3]);
        let original = app.doc.body(1).unwrap().material.color;
        let color = [0.9, 0.1, 0.2, 1.];
        app.object_color_editor.as_mut().unwrap().1 = color;
        app.selected = vec![1];
        frame(&mut app, &ctx, vec![]);
        let dialog = frame(&mut app, &ctx, vec![]);
        let apply = dialog
            .iter()
            .find(|(text, _)| text == "Apply Color")
            .unwrap()
            .1
            .center();
        click(&mut app, &ctx, apply, egui::PointerButton::Primary);
        assert!(app.object_color_editor.is_none());
        assert_eq!(app.doc.body(1).unwrap().material.color, original);
        for id in [2, 3] {
            assert_eq!(app.doc.body(id).unwrap().material.color, color);
        }
    }
}
