//! Small, stable plug-in boundary for optional S33D tools.
//!
//! The first implementation is built into the executable, while its manifest
//! lives under `Plug-ins/`. Keeping UI lifecycle behind this trait lets future
//! dynamic or scripted plug-ins use the same enable/open contract.

use crate::app::{UiLanguage, UiUnit};
use eframe::egui;
use s33d_model::{
    BodyId, GearKind, GearParams, SurfaceTextureParams, SurfaceTexturePattern,
    SurfaceTextureSurface, TireParams, TireTreadPattern, WheelRimParams, WheelRimStyle,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Portable layout: installed executables use a sibling Plug-ins folder.
/// Cargo's target/{debug,release} layout uses the workspace's Plug-ins folder.
fn plugin_directory(executable: &Path) -> PathBuf {
    let mut parent = executable.parent().unwrap_or_else(|| Path::new("."));
    // Rust test executables live one level below the corresponding app.
    if parent.file_name().is_some_and(|name| name == "deps") {
        parent = parent.parent().unwrap_or(parent);
    }
    if matches!(
        parent.file_name().and_then(|s| s.to_str()),
        Some("debug" | "release")
    ) && parent
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|s| s == "target")
    {
        if let Some(root) = parent.parent().and_then(Path::parent) {
            return root.join("Plug-ins");
        }
    }
    parent.join("Plug-ins")
}

#[derive(serde::Deserialize)]
struct PluginManifest {
    id: String,
    entry: String,
}

fn installed_plugin_ids(directory: &Path) -> (Vec<String>, Vec<String>) {
    let mut ids = Vec::new();
    let mut diagnostics = Vec::new();
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => return (ids, vec![format!("{}: {error}", directory.display())]),
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path().join("plugin.json"))
        .collect();
    paths.sort();
    for path in paths {
        let manifest = std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|data| {
                serde_json::from_slice::<PluginManifest>(&data).map_err(|error| error.to_string())
            });
        match manifest {
            Ok(manifest)
                if matches!(
                    manifest.id.as_str(),
                    "calculator"
                        | "gear-generator"
                        | "surface-texture"
                        | "tire-generator"
                        | "wheel-rim-generator"
                        | "screw-generator"
                        | "nut-generator"
                ) && manifest.entry == format!("builtin:{}", manifest.id) =>
            {
                if ids.contains(&manifest.id) {
                    diagnostics.push(format!(
                        "{}: duplicate plug-in ID {}",
                        path.display(),
                        manifest.id
                    ));
                } else {
                    ids.push(manifest.id);
                }
            }
            Ok(_) => diagnostics.push(format!(
                "{}: unsupported plug-in ID or entry",
                path.display()
            )),
            Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
        }
    }
    (ids, diagnostics)
}

fn calculator_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(Arc::from("s33d-calculator")))
}

#[derive(Debug, Clone)]
pub struct PluginDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub enabled: bool,
    pub open: bool,
}

#[derive(Debug, Clone)]
pub enum PluginCommand {
    GenerateFastener {
        params: s33d_model::FastenerParams,
        replace_body: Option<BodyId>,
    },
    GenerateGear {
        params: GearParams,
        replace_body: Option<BodyId>,
    },
    GenerateTire {
        params: TireParams,
        replace_body: Option<BodyId>,
    },
    GenerateWheelRim {
        params: WheelRimParams,
        replace_body: Option<BodyId>,
    },
    GenerateSurfaceTexture {
        source_body: BodyId,
        surface: SurfaceTextureSurface,
        params: SurfaceTextureParams,
        replace_body: Option<BodyId>,
    },
}

pub trait S33DPlugin {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
    fn open(&mut self);
    fn is_open(&self) -> bool;
    fn close(&mut self);
    fn draw(
        &mut self,
        ctx: &egui::Context,
        language: UiLanguage,
        unit: UiUnit,
    ) -> Option<PluginCommand>;
    fn sync_selected_gear(&mut self, _selection: Option<(BodyId, GearParams)>) {}
    fn sync_selected_fastener(&mut self, _selection: Option<(BodyId, s33d_model::FastenerParams)>) {
    }
    fn sync_selected_tire(&mut self, _selection: Option<(BodyId, TireParams)>) {}
    fn sync_selected_wheel_rim(&mut self, _selection: Option<(BodyId, WheelRimParams)>) {}
    fn sync_selected_surface_texture(
        &mut self,
        _selection: Option<(BodyId, BodyId, SurfaceTextureSurface, SurfaceTextureParams)>,
    ) {
    }
    fn sync_surface_texture_target(&mut self, _target: Option<(BodyId, SurfaceTextureSurface)>) {}
}

#[derive(Default)]
pub struct PluginHost {
    plugins: Vec<Box<dyn S33DPlugin>>,
    pub discovery_notes: Vec<String>,
}

impl PluginHost {
    pub fn new(
        calculator_enabled: bool,
        gear_enabled: bool,
        surface_texture_enabled: bool,
        tire_enabled: bool,
        wheel_rim_enabled: bool,
        screw_enabled: bool,
        nut_enabled: bool,
    ) -> Self {
        let mut host = Self {
            plugins: vec![
                Box::new(CalculatorPlugin::new(calculator_enabled)),
                Box::new(GearGeneratorPlugin::new(gear_enabled)),
                Box::new(SurfaceTexturePlugin::new(surface_texture_enabled)),
                Box::new(TireGeneratorPlugin::new(tire_enabled)),
                Box::new(WheelRimGeneratorPlugin::new(wheel_rim_enabled)),
                Box::new(crate::fastener_plugin::FastenerPlugin::new(
                    false,
                    screw_enabled,
                )),
                Box::new(crate::fastener_plugin::FastenerPlugin::new(
                    true,
                    nut_enabled,
                )),
            ],
            discovery_notes: Vec::new(),
        };
        match std::env::current_exe() {
            Ok(executable) => {
                let (ids, notes) = installed_plugin_ids(&plugin_directory(&executable));
                host.plugins
                    .retain(|plugin| ids.iter().any(|id| id == plugin.id()));
                host.discovery_notes = notes;
            }
            Err(error) => {
                host.plugins.clear();
                host.discovery_notes.push(error.to_string());
            }
        }
        host
    }

    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.plugins
            .iter()
            .map(|plugin| PluginDescriptor {
                id: plugin.id(),
                name: plugin.name(),
                description: plugin.description(),
                enabled: plugin.enabled(),
                open: plugin.is_open(),
            })
            .collect()
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(plugin) = self.plugins.iter_mut().find(|plugin| plugin.id() == id) {
            plugin.set_enabled(enabled);
        }
    }

    pub fn toggle(&mut self, id: &str) {
        if let Some(p) = self
            .plugins
            .iter_mut()
            .find(|p| p.id() == id && p.enabled())
        {
            if p.is_open() {
                p.close();
            } else {
                p.open();
            }
        }
    }

    pub fn open(&mut self, id: &str) -> bool {
        if let Some(plugin) = self.plugins.iter_mut().find(|plugin| plugin.id() == id) {
            // Opening a plug-in is an explicit user action. Make it reliable
            // even when the persisted enable flag is off or missing.
            plugin.set_enabled(true);
            plugin.open();
            true
        } else {
            false
        }
    }

    pub fn sync_selected_gear(&mut self, selection: Option<(BodyId, GearParams)>) {
        for plugin in &mut self.plugins {
            plugin.sync_selected_gear(selection);
        }
    }

    pub fn sync_selected_fastener(
        &mut self,
        selection: Option<(BodyId, s33d_model::FastenerParams)>,
    ) {
        for plugin in &mut self.plugins {
            plugin.sync_selected_fastener(selection);
        }
    }

    pub fn sync_selected_tire(&mut self, selection: Option<(BodyId, TireParams)>) {
        for plugin in &mut self.plugins {
            plugin.sync_selected_tire(selection);
        }
    }

    pub fn sync_selected_wheel_rim(&mut self, selection: Option<(BodyId, WheelRimParams)>) {
        for plugin in &mut self.plugins {
            plugin.sync_selected_wheel_rim(selection);
        }
    }

    pub fn sync_selected_surface_texture(
        &mut self,
        selection: Option<(BodyId, BodyId, SurfaceTextureSurface, SurfaceTextureParams)>,
    ) {
        for plugin in &mut self.plugins {
            plugin.sync_selected_surface_texture(selection);
        }
    }

    pub fn sync_surface_texture_target(&mut self, target: Option<(BodyId, SurfaceTextureSurface)>) {
        for plugin in &mut self.plugins {
            plugin.sync_surface_texture_target(target);
        }
    }

    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        language: UiLanguage,
        unit: UiUnit,
    ) -> Vec<PluginCommand> {
        let mut commands = Vec::new();
        for plugin in &mut self.plugins {
            if plugin.enabled() {
                if let Some(command) = plugin.draw(ctx, language, unit) {
                    commands.push(command);
                }
            }
        }
        commands
    }
}

struct CalculatorPlugin {
    enabled: bool,
    open: bool,
    /// One calculator-style display: expression while entering, result after
    /// `=`, and exactly `Error` after a failed evaluation.
    display: String,
    last_value: f64,
    angle_mode: AngleMode,
    replace_on_next_entry: bool,
    memory: f64,
    history: Vec<(String, String)>,
    error: Option<CalculatorDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalculatorDiagnostic {
    message: String,
    position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AngleMode {
    Degrees,
    Radians,
}

impl CalculatorPlugin {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            open: false,
            display: "0".to_string(),
            last_value: 0.0,
            angle_mode: AngleMode::Degrees,
            replace_on_next_entry: true,
            memory: 0.0,
            history: Vec::new(),
            error: None,
        }
    }

    fn evaluate(&mut self) {
        if self.display == "Error" {
            return;
        }
        let completed = complete_calculator_parentheses(&self.display);
        let expression = completed.clone();
        self.display = match evaluate_expression_with_diagnostics(
            &completed,
            self.last_value,
            self.angle_mode,
        ) {
            Ok(value) if value.is_finite() => {
                self.last_value = value;
                self.error = None;
                let result = format_calculator_value(value);
                self.history.push((expression, result.clone()));
                if self.history.len() > 24 {
                    self.history.remove(0);
                }
                result
            }
            Ok(_) => {
                self.error = Some(CalculatorDiagnostic {
                    message: "result is not finite".into(),
                    position: completed.chars().count().max(1),
                });
                "Error".to_string()
            }
            Err(error) => {
                self.error = Some(error);
                "Error".to_string()
            }
        };
        self.replace_on_next_entry = true;
    }

    fn current_value(&self) -> Option<f64> {
        if self.display == "Error" {
            return None;
        }
        evaluate_expression_with_context(&self.display, self.last_value, self.angle_mode)
            .ok()
            .filter(|value| value.is_finite())
    }

    fn button(&mut self, action: &str) {
        match action {
            "EVAL" => self.evaluate(),
            "CLEAR" => {
                self.display = "0".to_string();
                self.replace_on_next_entry = true;
                self.error = None;
            }
            "BACK" => {
                if self.replace_on_next_entry || self.display == "Error" {
                    self.display = "0".to_string();
                    self.replace_on_next_entry = true;
                } else {
                    self.display.pop();
                    if self.display.is_empty() {
                        self.display = "0".to_string();
                        self.replace_on_next_entry = true;
                    }
                }
            }
            "SIGN" => {
                if self.display == "Error" {
                    self.display = "0".to_string();
                    self.replace_on_next_entry = true;
                } else if self.display == "0" {
                    // Keep zero canonical, just like a physical calculator.
                } else if self.display.starts_with("-(") && self.display.ends_with(')') {
                    self.display = self.display[2..self.display.len() - 1].to_string();
                } else {
                    self.display = format!("-({})", self.display);
                }
                self.replace_on_next_entry = false;
                self.error = None;
            }
            "RECIP" => {
                if self.display == "Error" || (self.display == "0" && self.replace_on_next_entry) {
                    self.display = "1/(".to_string();
                } else {
                    self.display = format!("1/({})", self.display);
                }
                self.replace_on_next_entry = false;
                self.error = None;
            }
            "MC" => self.memory = 0.0,
            "MR" => {
                self.display = format_calculator_value(self.memory);
                self.replace_on_next_entry = true;
                self.error = None;
            }
            "MPLUS" => {
                if let Some(value) = self.current_value() {
                    self.memory += value;
                }
            }
            "MMINUS" => {
                if let Some(value) = self.current_value() {
                    self.memory -= value;
                }
            }
            token => {
                let binary_operator = matches!(token, "+" | "-" | "*" | "/" | "^");
                if self.display == "Error" {
                    self.display = "0".to_string();
                    self.replace_on_next_entry = true;
                }
                if self.replace_on_next_entry {
                    if binary_operator {
                        self.display.push_str(token);
                    } else {
                        self.display = if token == "." {
                            "0.".to_string()
                        } else {
                            token.to_string()
                        };
                    }
                } else if self.display == "0" && token.chars().all(|ch| ch.is_ascii_digit()) {
                    self.display = token.to_string();
                } else {
                    self.display.push_str(token);
                }
                self.replace_on_next_entry = false;
                self.error = None;
            }
        }
    }
}

impl S33DPlugin for CalculatorPlugin {
    fn id(&self) -> &'static str {
        "calculator"
    }

    fn name(&self) -> &'static str {
        "Calculator"
    }

    fn description(&self) -> &'static str {
        "Evaluate arithmetic expressions without leaving S33D."
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = false;
        }
    }

    fn open(&mut self) {
        self.open = true;
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn close(&mut self) {
        self.open = false;
    }

    fn draw(
        &mut self,
        ctx: &egui::Context,
        language: UiLanguage,
        _unit: UiUnit,
    ) -> Option<PluginCommand> {
        let mut open = self.open;
        egui::Window::new(language.text("Scientific Calculator", "科学计算器"))
            .id(egui::Id::new("plugin_calculator"))
            .default_width(455.0)
            .min_width(425.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let response = ui.add_sized(
                    [ui.available_width(), 43.0],
                    egui::TextEdit::singleline(&mut self.display)
                        .desired_width(f32::INFINITY)
                        .font(calculator_font(23.0))
                        .horizontal_align(egui::Align::RIGHT),
                );
                if response.changed() {
                    if self.display.is_empty() {
                        self.display = "0".to_string();
                        self.replace_on_next_entry = true;
                    } else {
                        self.replace_on_next_entry = false;
                    }
                }
                // A single-line edit does not consistently surrender focus on
                // Enter on every platform. Evaluate while it owns focus, as
                // well as on the legacy lost-focus path.
                let enter_pressed = ui.input(|input| input.key_pressed(egui::Key::Enter));
                let enter = enter_pressed && (response.has_focus() || response.lost_focus());
                ui.horizontal(|ui| {
                    ui.label(language.text("Angle", "角度"));
                    ui.selectable_value(&mut self.angle_mode, AngleMode::Degrees, "DEG");
                    ui.selectable_value(&mut self.angle_mode, AngleMode::Radians, "RAD");
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("Ans = {}", format_calculator_value(self.last_value)))
                            .small()
                            .weak(),
                    );
                });
                if let Some(error) = &self.error {
                    let message = localized_calculator_error(language, &error.message);
                    ui.label(
                        egui::RichText::new(if language == UiLanguage::English {
                            format!("{message} · character {}", error.position)
                        } else {
                            format!("{message} · 第 {} 个字符", error.position)
                        })
                        .small()
                        .color(egui::Color32::from_rgb(190, 72, 62)),
                    );
                }
                ui.separator();

                ui.horizontal(|ui| {
                    let gap = ui.spacing().item_spacing.x;
                    let width = ((ui.available_width() - gap * 4.0) / 5.0).max(58.0);
                    for (label, action) in [
                        ("MC", "MC"),
                        ("MR", "MR"),
                        ("M+", "MPLUS"),
                        ("M−", "MMINUS"),
                    ] {
                        if ui
                            .add_sized(
                                [width, 29.0],
                                egui::Button::new(
                                    egui::RichText::new(label).font(calculator_font(14.0)),
                                ),
                            )
                            .clicked()
                        {
                            self.button(action);
                        }
                    }
                    ui.add_sized(
                        [width, 29.0],
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "M {}",
                                format_calculator_value(self.memory)
                            ))
                            .small()
                            .weak(),
                        ),
                    );
                });

                const ROWS: &[&[(&str, &str)]] = &[
                    &[
                        ("sin", "sin("),
                        ("cos", "cos("),
                        ("tan", "tan("),
                        ("asin", "asin("),
                        ("acos", "acos("),
                    ],
                    &[
                        ("atan", "atan("),
                        ("ln", "ln("),
                        ("log", "log("),
                        ("√", "sqrt("),
                        ("x²", "^2"),
                    ],
                    &[
                        ("(", "("),
                        (")", ")"),
                        ("π", "pi"),
                        ("e", "e"),
                        ("Back", "BACK"),
                    ],
                    &[("7", "7"), ("8", "8"), ("9", "9"), ("÷", "/"), ("xʸ", "^")],
                    &[("4", "4"), ("5", "5"), ("6", "6"), ("×", "*"), ("1/x", "RECIP")],
                    &[("1", "1"), ("2", "2"), ("3", "3"), ("−", "-"), ("Ans", "ans")],
                    &[("0", "0"), (".", "."), ("±", "SIGN"), ("+", "+"), ("=", "EVAL")],
                ];
                for row in ROWS {
                    ui.horizontal(|ui| {
                        let gap = ui.spacing().item_spacing.x;
                        let button_width = ((ui.available_width() - gap * 4.0) / 5.0).max(58.0);
                        for (label, action) in *row {
                            let is_back = *action == "BACK";
                            let visible_label = if is_back {
                                language.text("Back", "退格")
                            } else {
                                label
                            };
                            let size = if *action == "pi" { 20.0 } else { 15.0 };
                            let mut label =
                                egui::RichText::new(visible_label).font(calculator_font(size));
                            if is_back {
                                label = label.color(egui::Color32::from_rgb(52, 66, 84));
                            }
                            let mut button = egui::Button::new(label);
                            if is_back {
                                button = button
                                    .fill(egui::Color32::from_rgb(220, 228, 238))
                                    .stroke(egui::Stroke::new(
                                        1.0_f32,
                                        egui::Color32::from_rgb(145, 159, 177),
                                    ));
                            }
                            let response = ui.add_sized([button_width, 33.0], button);
                            if response.clicked() {
                                self.button(action);
                            }
                            if is_back {
                                response.on_hover_text(language.text(
                                    "Delete the last character",
                                    "删除最后一个字符",
                                ));
                            }
                        }
                    });
                }
                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new(language.text("Clear", "清空")),
                    )
                    .clicked()
                {
                    self.button("CLEAR");
                }
                egui::CollapsingHeader::new(language.text("History", "历史"))
                    .default_open(false)
                    .show(ui, |ui| {
                        if self.history.is_empty() {
                            ui.label(
                                egui::RichText::new(language.text(
                                    "No calculations yet",
                                    "暂无计算记录",
                                ))
                                .small()
                                .weak(),
                            );
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("calculator_history")
                                .max_height(130.0)
                                .show(ui, |ui| {
                                    let mut recall = None;
                                    for (expression, result) in self.history.iter().rev() {
                                        let response = ui.add_sized(
                                            [ui.available_width(), 25.0],
                                            egui::Button::new(
                                                egui::RichText::new(format!(
                                                    "{expression} = {result}"
                                                ))
                                                .font(calculator_font(13.0)),
                                            ),
                                        );
                                        if response
                                            .on_hover_text(language.text(
                                                "Recall this result",
                                                "重新使用此结果",
                                            ))
                                            .clicked()
                                        {
                                            recall = Some(result.clone());
                                        }
                                    }
                                    if let Some(result) = recall {
                                        self.display = result;
                                        self.replace_on_next_entry = true;
                                        self.error = None;
                                    }
                                });
                        }
                    });
                if enter {
                    self.evaluate();
                }
                ui.separator();
                ui.label(
                    egui::RichText::new(language.text(
                        "Functions: sin cos tan asin acos atan sqrt ln log abs exp · constants: pi e Ans",
                        "函数：sin cos tan asin acos atan sqrt ln log abs exp；常量：pi、e、Ans",
                    ))
                    .small()
                    .weak(),
                );
            });
        self.open = open;
        None
    }
}

struct GearGeneratorPlugin {
    enabled: bool,
    open: bool,
    params: GearParams,
    editing_body: Option<BodyId>,
    synced_selection: Option<BodyId>,
}

impl GearGeneratorPlugin {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            open: false,
            params: GearParams::default(),
            editing_body: None,
            synced_selection: None,
        }
    }
}

impl S33DPlugin for GearGeneratorPlugin {
    fn id(&self) -> &'static str {
        "gear-generator"
    }

    fn name(&self) -> &'static str {
        "Gear Generator"
    }

    fn description(&self) -> &'static str {
        "Create parametric spur, helical, bevel and internal gears."
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = false;
        }
    }

    fn open(&mut self) {
        self.open = true;
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn close(&mut self) {
        self.open = false;
    }

    fn sync_selected_gear(&mut self, selection: Option<(BodyId, GearParams)>) {
        let selection_id = selection.map(|(id, _)| id);
        if selection_id == self.synced_selection {
            return;
        }
        self.synced_selection = selection_id;
        if let Some((id, params)) = selection {
            self.params = params;
            self.editing_body = Some(id);
        } else {
            self.editing_body = None;
        }
    }

    fn draw(
        &mut self,
        ctx: &egui::Context,
        language: UiLanguage,
        unit: UiUnit,
    ) -> Option<PluginCommand> {
        let mut open = self.open;
        let mut command = None;
        egui::Window::new(language.text("Gear Generator", "齿轮生成器"))
            .id(egui::Id::new("plugin_gear_generator"))
            .default_width(390.0)
            .min_width(320.0)
            .default_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .min_height(0.0)
            .max_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let footer_id = ui.make_persistent_id("gear_footer_height");
                let footer_height = ctx.data(|d| d.get_temp::<f32>(footer_id));
                let body_height = (ui.available_height()
                    - footer_height.unwrap_or(100.0)
                    - ui.spacing().item_spacing.y)
                    .max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("gear_parameters_scroll")
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(language.text("Type", "类型"));
                            egui::ComboBox::from_id_salt("gear_kind")
                                .selected_text(gear_kind_label(self.params.kind, language))
                                .show_ui(ui, |ui| {
                                    for kind in GearKind::ALL {
                                        ui.selectable_value(
                                            &mut self.params.kind,
                                            kind,
                                            gear_kind_label(kind, language),
                                        );
                                    }
                                });
                            if self.editing_body.is_some()
                                && ui.button(language.text("Create new", "新建")).clicked()
                            {
                                self.editing_body = None;
                                // Keep the current selection marked as synchronized so
                                // the next frame does not immediately switch back into
                                // update mode. Selecting another object (or this gear
                                // again after deselecting it) resumes normal syncing.
                            }
                        });
                        if let Some(id) = self.editing_body {
                            ui.label(
                                egui::RichText::new(if language == UiLanguage::English {
                                    format!("Editing object {id}")
                                } else {
                                    format!("正在编辑对象 {id}")
                                })
                                .small()
                                .color(egui::Color32::from_rgb(54, 126, 188)),
                            );
                        }

                        draw_gear_preview(ui, &self.params);
                        ui.separator();
                        egui::Grid::new("gear_parameters")
                            .num_columns(3)
                            .spacing([10.0, 7.0])
                            .show(ui, |ui| {
                                ui.label(language.text("Module", "模数"));
                                gear_length_value(
                                    ui,
                                    &mut self.params.module,
                                    unit,
                                    0.1,
                                    0.1..=50.0,
                                );
                                ui.label(unit.symbol());
                                ui.end_row();

                                ui.label(language.text("Teeth", "齿数"));
                                ui.add(egui::DragValue::new(&mut self.params.teeth).range(8..=200));
                                ui.label("T");
                                ui.end_row();

                                ui.label(language.text("Pressure angle", "压力角"));
                                ui.add(
                                    egui::DragValue::new(&mut self.params.pressure_angle_deg)
                                        .speed(0.5)
                                        .range(14.0..=30.0),
                                );
                                ui.label("°");
                                ui.end_row();

                                ui.label(language.text("Face width", "齿宽"));
                                gear_length_value(
                                    ui,
                                    &mut self.params.face_width,
                                    unit,
                                    0.5,
                                    0.1..=1_000.0,
                                );
                                ui.label(unit.symbol());
                                ui.end_row();

                                if self.params.kind != GearKind::Internal {
                                    ui.label(language.text("Bore diameter", "孔径"));
                                    gear_length_value(
                                        ui,
                                        &mut self.params.bore_diameter,
                                        unit,
                                        0.5,
                                        0.0..=10_000.0,
                                    );
                                    ui.label(unit.symbol());
                                    ui.end_row();
                                }

                                ui.label(language.text("Backlash", "齿侧间隙"));
                                gear_length_value(
                                    ui,
                                    &mut self.params.backlash,
                                    unit,
                                    0.01,
                                    0.0..=100.0,
                                );
                                ui.label(unit.symbol());
                                ui.end_row();

                                if self.params.kind == GearKind::Helical {
                                    ui.label(language.text("Helix angle", "螺旋角"));
                                    ui.add(
                                        egui::DragValue::new(&mut self.params.helix_angle_deg)
                                            .speed(0.5)
                                            .range(-45.0..=45.0),
                                    );
                                    ui.label("°");
                                    ui.end_row();
                                }
                                if self.params.kind == GearKind::Bevel {
                                    ui.label(language.text("Cone angle", "锥角"));
                                    ui.add(
                                        egui::DragValue::new(&mut self.params.bevel_angle_deg)
                                            .speed(0.5)
                                            .range(5.0..=60.0),
                                    );
                                    ui.label("°");
                                    ui.end_row();
                                }
                                if self.params.kind == GearKind::Internal {
                                    ui.label(language.text("Ring wall", "齿圈壁厚"));
                                    gear_length_value(
                                        ui,
                                        &mut self.params.ring_wall_thickness,
                                        unit,
                                        0.5,
                                        0.1..=10_000.0,
                                    );
                                    ui.label(unit.symbol());
                                    ui.end_row();
                                }
                            });

                        ui.collapsing(language.text("Position", "位置"), |ui| {
                            egui::Grid::new("gear_position")
                                .num_columns(3)
                                .show(ui, |ui| {
                                    for axis in 0..3 {
                                        ui.label(["X", "Y", "Z"][axis]);
                                        gear_length_value(
                                            ui,
                                            &mut self.params.position[axis],
                                            unit,
                                            1.0,
                                            -1_000_000.0..=1_000_000.0,
                                        );
                                        ui.label(unit.symbol());
                                        ui.end_row();
                                    }
                                });
                        });

                        let dimensions = crate::gear::metrics(&self.params);
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!(
                                "{}: {}   ·   {}: {}   ·   {}: {}",
                                language.text("Pitch", "分度圆"),
                                unit.format_mm(dimensions.pitch_diameter),
                                language.text("Outside", "外径"),
                                unit.format_mm(dimensions.outside_diameter),
                                language.text("Root", "齿根径"),
                                unit.format_mm(dimensions.root_diameter),
                            ))
                            .small()
                            .weak(),
                        );
                    });
                let footer_top = ui.cursor().top();
                let validation = crate::gear::validate(&self.params);
                if let Err(error) = &validation {
                    ui.label(
                        egui::RichText::new(localized_gear_error(language, error))
                            .small()
                            .color(egui::Color32::from_rgb(190, 72, 62)),
                    );
                }
                let label = if self.editing_body.is_some() {
                    language.text("Update gear", "更新齿轮")
                } else {
                    language.text("Generate gear", "生成齿轮")
                };
                if ui
                    .add_enabled_ui(validation.is_ok(), |ui| {
                        ui.add_sized([ui.available_width(), 34.0], egui::Button::new(label))
                    })
                    .inner
                    .clicked()
                {
                    command = Some(PluginCommand::GenerateGear {
                        params: self.params,
                        replace_body: self.editing_body,
                    });
                }
                let measured = ui.cursor().top() - footer_top;
                ctx.data_mut(|d| d.insert_temp(footer_id, measured));
                if footer_height.is_none_or(|h| (h - measured).abs() > 0.5) {
                    ctx.request_discard("gear footer height changed");
                }
            });
        self.open = open;
        command
    }
}

struct SurfaceTexturePlugin {
    enabled: bool,
    open: bool,
    params: SurfaceTextureParams,
    source_body: Option<BodyId>,
    surface: Option<SurfaceTextureSurface>,
    editing_body: Option<BodyId>,
    synced_selection: Option<BodyId>,
}

impl SurfaceTexturePlugin {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            open: false,
            params: SurfaceTextureParams::default(),
            source_body: None,
            surface: None,
            editing_body: None,
            synced_selection: None,
        }
    }
}

impl S33DPlugin for SurfaceTexturePlugin {
    fn id(&self) -> &'static str {
        "surface-texture"
    }
    fn name(&self) -> &'static str {
        "Surface Texture"
    }
    fn description(&self) -> &'static str {
        "Cut straight, diagonal or diamond knurling into a planar or cylindrical surface."
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = false;
        }
    }
    fn open(&mut self) {
        self.open = true;
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn close(&mut self) {
        self.open = false;
    }

    fn sync_selected_surface_texture(
        &mut self,
        selection: Option<(BodyId, BodyId, SurfaceTextureSurface, SurfaceTextureParams)>,
    ) {
        let selection_id = selection.map(|value| value.0);
        if selection_id == self.synced_selection {
            return;
        }
        self.synced_selection = selection_id;
        if let Some((body_id, source_body, surface, params)) = selection {
            self.editing_body = Some(body_id);
            self.source_body = Some(source_body);
            self.surface = Some(surface);
            self.params = params;
        } else {
            self.editing_body = None;
        }
    }

    fn sync_surface_texture_target(&mut self, target: Option<(BodyId, SurfaceTextureSurface)>) {
        if let Some((source_body, surface)) = target {
            if self.source_body != Some(source_body) || self.surface != Some(surface) {
                self.source_body = Some(source_body);
                self.surface = Some(surface);
                self.editing_body = None;
                self.synced_selection = None;
            }
        }
    }

    fn draw(
        &mut self,
        ctx: &egui::Context,
        language: UiLanguage,
        unit: UiUnit,
    ) -> Option<PluginCommand> {
        let mut open = self.open;
        let mut command = None;
        egui::Window::new(language.text("Surface Texture", "表面纹理"))
            .id(egui::Id::new("plugin_surface_texture"))
            .default_width(390.0)
            .min_width(350.0)
            .default_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .min_height(0.0)
            .max_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let footer_id = ui.make_persistent_id("texture_footer_height");
                let footer_height = ctx.data(|d| d.get_temp::<f32>(footer_id));
                let body_height = (ui.available_height()
                    - footer_height.unwrap_or(100.0)
                    - ui.spacing().item_spacing.y)
                    .max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("texture_parameters_scroll")
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        ui.label(
                    egui::RichText::new(language.text(
                        "Select a planar face or a complete cylindrical wall in the viewport.",
                        "请在工作区选择平面或完整圆柱壁。",
                    ))
                    .small()
                    .weak(),
                );
                        match (self.source_body, self.surface) {
                            (Some(id), Some(surface)) => {
                                let description = match surface {
                                    SurfaceTextureSurface::Planar { width, height, .. } => {
                                        format!(
                                            "{} {id} · {} × {}",
                                            language.text("Planar object", "平面对象"),
                                            unit.format_mm(width),
                                            unit.format_mm(height)
                                        )
                                    }
                                    SurfaceTextureSurface::Cylinder { radius, length, .. } => {
                                        format!(
                                            "{} {id} · Ø{} × {}",
                                            language.text("Cylinder object", "圆柱对象"),
                                            unit.format_mm(radius * 2.0),
                                            unit.format_mm(length)
                                        )
                                    }
                                };
                                ui.label(
                                    egui::RichText::new(description)
                                        .color(egui::Color32::from_rgb(54, 126, 188)),
                                );
                            }
                            _ => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(190, 72, 62),
                                    language.text(
                                        "No supported surface selected",
                                        "尚未选择支持的表面",
                                    ),
                                );
                            }
                        }
                        if self.editing_body.is_some()
                            && ui.button(language.text("Create new", "新建")).clicked()
                        {
                            self.editing_body = None;
                        }
                        draw_texture_preview(ui, self.params.pattern);
                        egui::Grid::new("surface_texture_parameters")
                            .num_columns(3)
                            .spacing([10.0, 7.0])
                            .show(ui, |ui| {
                                ui.label(language.text("Pattern", "纹理"));
                                egui::ComboBox::from_id_salt("surface_texture_pattern")
                                    .selected_text(texture_pattern_label(
                                        self.params.pattern,
                                        language,
                                    ))
                                    .show_ui(ui, |ui| {
                                        for pattern in SurfaceTexturePattern::ALL {
                                            ui.selectable_value(
                                                &mut self.params.pattern,
                                                pattern,
                                                texture_pattern_label(pattern, language),
                                            );
                                        }
                                    });
                                ui.label("");
                                ui.end_row();
                                ui.label(language.text("Pitch", "节距"));
                                gear_length_value(
                                    ui,
                                    &mut self.params.pitch,
                                    unit,
                                    0.1,
                                    0.05..=1_000.0,
                                );
                                ui.label(unit.symbol());
                                ui.end_row();
                                ui.label(language.text("Groove width", "槽宽"));
                                gear_length_value(
                                    ui,
                                    &mut self.params.groove_width,
                                    unit,
                                    0.05,
                                    0.01..=1_000.0,
                                );
                                ui.label(unit.symbol());
                                ui.end_row();
                                ui.label(language.text("Depth", "深度"));
                                gear_length_value(
                                    ui,
                                    &mut self.params.depth,
                                    unit,
                                    0.05,
                                    0.01..=1_000.0,
                                );
                                ui.label(unit.symbol());
                                ui.end_row();
                                if self.params.pattern != SurfaceTexturePattern::Straight {
                                    ui.label(language.text("Angle", "角度"));
                                    ui.add(
                                        egui::DragValue::new(&mut self.params.angle_deg)
                                            .speed(1.0)
                                            .range(5.0..=85.0),
                                    );
                                    ui.label("°");
                                    ui.end_row();
                                }
                                ui.label(language.text("Edge margin", "边缘留白"));
                                gear_length_value(
                                    ui,
                                    &mut self.params.margin,
                                    unit,
                                    0.1,
                                    0.0..=100_000.0,
                                );
                                ui.label(unit.symbol());
                                ui.end_row();
                            });
                    });
                let footer_top = ui.cursor().top();
                let validation = self
                    .surface
                    .as_ref()
                    .map(|surface| crate::surface_texture::validate(surface, &self.params));
                if let Some(Err(error)) = &validation {
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 72, 62),
                        localized_texture_error(language, error),
                    );
                }
                let ready = self.source_body.is_some()
                    && self.surface.is_some()
                    && validation.is_some_and(|result| result.is_ok());
                let label = if self.editing_body.is_some() {
                    language.text("Update texture", "更新纹理")
                } else {
                    language.text("Apply texture", "应用纹理")
                };
                if ui
                    .add_enabled_ui(ready, |ui| {
                        ui.add_sized([ui.available_width(), 34.0], egui::Button::new(label))
                    })
                    .inner
                    .clicked()
                {
                    command = Some(PluginCommand::GenerateSurfaceTexture {
                        source_body: self.source_body.unwrap(),
                        surface: self.surface.unwrap(),
                        params: self.params,
                        replace_body: self.editing_body,
                    });
                }
                let measured = ui.cursor().top() - footer_top;
                ctx.data_mut(|d| d.insert_temp(footer_id, measured));
                if footer_height.is_none_or(|h| (h - measured).abs() > 0.5) {
                    ctx.request_discard("plugin footer height changed");
                }
            });
        self.open = open;
        command
    }
}

struct TireGeneratorPlugin {
    enabled: bool,
    open: bool,
    params: TireParams,
    editing_body: Option<BodyId>,
    synced_selection: Option<BodyId>,
}

impl TireGeneratorPlugin {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            open: false,
            params: TireParams::default(),
            editing_body: None,
            synced_selection: None,
        }
    }
}

impl S33DPlugin for TireGeneratorPlugin {
    fn id(&self) -> &'static str {
        "tire-generator"
    }
    fn name(&self) -> &'static str {
        "Tire Generator"
    }
    fn description(&self) -> &'static str {
        "Create dimensioned tires with common road and off-road tread patterns."
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = false;
        }
    }
    fn open(&mut self) {
        self.open = true;
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn close(&mut self) {
        self.open = false;
    }
    fn sync_selected_tire(&mut self, selection: Option<(BodyId, TireParams)>) {
        let selection_id = selection.map(|value| value.0);
        if selection_id == self.synced_selection {
            return;
        }
        self.synced_selection = selection_id;
        if let Some((body_id, params)) = selection {
            self.params = params;
            self.editing_body = Some(body_id);
        } else {
            self.editing_body = None;
        }
    }
    fn draw(
        &mut self,
        ctx: &egui::Context,
        language: UiLanguage,
        unit: UiUnit,
    ) -> Option<PluginCommand> {
        let mut open = self.open;
        let mut command = None;
        egui::Window::new(language.text("Tire Generator", "轮胎生成器"))
            .id(egui::Id::new("plugin_tire_generator"))
            .default_width(390.0)
            .min_width(350.0)
            .default_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .min_height(0.0)
            .max_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let footer_id = ui.make_persistent_id("tire_footer_height");
                let footer_height = ctx.data(|d| d.get_temp::<f32>(footer_id));
                let body_height = (ui.available_height()
                    - footer_height.unwrap_or(100.0)
                    - ui.spacing().item_spacing.y)
                    .max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("tire_parameters_scroll")
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(language.text("Tread", "胎纹"));
                            egui::ComboBox::from_id_salt("tire_tread_pattern")
                                .selected_text(tire_pattern_label(self.params.pattern, language))
                                .show_ui(ui, |ui| {
                                    for pattern in TireTreadPattern::ALL {
                                        ui.selectable_value(
                                            &mut self.params.pattern,
                                            pattern,
                                            tire_pattern_label(pattern, language),
                                        );
                                    }
                                });
                            if self.editing_body.is_some()
                                && ui.button(language.text("Create new", "新建")).clicked()
                            {
                                self.editing_body = None;
                            }
                        });
                        draw_tire_preview(ui, self.params.pattern);
                        egui::Grid::new("tire_parameters")
                            .num_columns(3)
                            .spacing([10.0, 7.0])
                            .show(ui, |ui| {
                                length_parameter_row(
                                    ui,
                                    language.text("Outer diameter", "外径"),
                                    &mut self.params.outer_diameter,
                                    unit,
                                    20.0..=10_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Section width", "断面宽"),
                                    &mut self.params.section_width,
                                    unit,
                                    4.0..=5_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Rim diameter", "轮辋直径"),
                                    &mut self.params.rim_diameter,
                                    unit,
                                    5.0..=9_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Tread depth", "胎纹深度"),
                                    &mut self.params.tread_depth,
                                    unit,
                                    0.0..=500.0,
                                );
                                ui.label(language.text("Tread repeats", "胎纹重复数"));
                                ui.add(
                                    egui::DragValue::new(&mut self.params.tread_repeats)
                                        .range(8..=120),
                                );
                                ui.label("");
                                ui.end_row();
                            });
                        position_editor(
                            ui,
                            "tire_position",
                            &mut self.params.position,
                            language,
                            unit,
                        );
                        let metrics = crate::tire::metrics(&self.params);
                        ui.label(
                            egui::RichText::new(format!(
                                "{}: {}",
                                language.text("Sidewall", "胎侧高度"),
                                unit.format_mm(metrics.sidewall_height)
                            ))
                            .small()
                            .weak(),
                        );
                    });
                let footer_top = ui.cursor().top();
                let validation = crate::tire::validate(&self.params);
                if let Err(error) = &validation {
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 72, 62),
                        localized_tire_error(language, error),
                    );
                }
                let label = if self.editing_body.is_some() {
                    language.text("Update tire", "更新轮胎")
                } else {
                    language.text("Generate tire", "生成轮胎")
                };
                if ui
                    .add_enabled_ui(validation.is_ok(), |ui| {
                        ui.add_sized([ui.available_width(), 34.0], egui::Button::new(label))
                    })
                    .inner
                    .clicked()
                {
                    command = Some(PluginCommand::GenerateTire {
                        params: self.params,
                        replace_body: self.editing_body,
                    });
                }
                let measured = ui.cursor().top() - footer_top;
                ctx.data_mut(|d| d.insert_temp(footer_id, measured));
                if footer_height.is_none_or(|h| (h - measured).abs() > 0.5) {
                    ctx.request_discard("plugin footer height changed");
                }
            });
        self.open = open;
        command
    }
}

struct WheelRimGeneratorPlugin {
    enabled: bool,
    open: bool,
    params: WheelRimParams,
    editing_body: Option<BodyId>,
    synced_selection: Option<BodyId>,
}

impl WheelRimGeneratorPlugin {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            open: false,
            params: WheelRimParams::default(),
            editing_body: None,
            synced_selection: None,
        }
    }
}

impl S33DPlugin for WheelRimGeneratorPlugin {
    fn id(&self) -> &'static str {
        "wheel-rim-generator"
    }
    fn name(&self) -> &'static str {
        "Wheel Rim Generator"
    }
    fn description(&self) -> &'static str {
        "Create dimensioned wheel rims with common spoke styles and bolt patterns."
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.open = false;
        }
    }
    fn open(&mut self) {
        self.open = true;
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn close(&mut self) {
        self.open = false;
    }
    fn sync_selected_wheel_rim(&mut self, selection: Option<(BodyId, WheelRimParams)>) {
        let selection_id = selection.map(|value| value.0);
        if selection_id == self.synced_selection {
            return;
        }
        self.synced_selection = selection_id;
        if let Some((body_id, params)) = selection {
            self.params = params;
            self.editing_body = Some(body_id);
        } else {
            self.editing_body = None;
        }
    }
    fn draw(
        &mut self,
        ctx: &egui::Context,
        language: UiLanguage,
        unit: UiUnit,
    ) -> Option<PluginCommand> {
        let mut open = self.open;
        let mut command = None;
        egui::Window::new(language.text("Wheel Rim Generator", "轮毂生成器"))
            .id(egui::Id::new("plugin_wheel_rim_generator"))
            .default_width(410.0)
            .min_width(370.0)
            .default_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .min_height(0.0)
            .max_height((ctx.screen_rect().height() - 64.0).max(0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let footer_id = ui.make_persistent_id("rim_footer_height");
                let footer_height = ctx.data(|d| d.get_temp::<f32>(footer_id));
                let body_height = (ui.available_height()
                    - footer_height.unwrap_or(100.0)
                    - ui.spacing().item_spacing.y)
                    .max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("rim_parameters_scroll")
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(language.text("Style", "样式"));
                            egui::ComboBox::from_id_salt("wheel_rim_style")
                                .selected_text(wheel_rim_style_label(self.params.style, language))
                                .show_ui(ui, |ui| {
                                    for style in WheelRimStyle::ALL {
                                        ui.selectable_value(
                                            &mut self.params.style,
                                            style,
                                            wheel_rim_style_label(style, language),
                                        );
                                    }
                                });
                            if self.editing_body.is_some()
                                && ui.button(language.text("Create new", "新建")).clicked()
                            {
                                self.editing_body = None;
                            }
                        });
                        draw_wheel_rim_preview(ui, self.params.style);

                        egui::Grid::new("wheel_rim_parameters")
                            .num_columns(3)
                            .spacing([10.0, 7.0])
                            .show(ui, |ui| {
                                length_parameter_row(
                                    ui,
                                    language.text("Diameter", "直径"),
                                    &mut self.params.diameter,
                                    unit,
                                    30.0..=10_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Width", "宽度"),
                                    &mut self.params.width,
                                    unit,
                                    4.0..=5_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Rim thickness", "轮缘厚度"),
                                    &mut self.params.rim_thickness,
                                    unit,
                                    1.0..=1_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Hub diameter", "轮心直径"),
                                    &mut self.params.hub_diameter,
                                    unit,
                                    5.0..=5_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Center bore", "中心孔径"),
                                    &mut self.params.center_bore_diameter,
                                    unit,
                                    1.0..=5_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Spoke width", "辐条宽度"),
                                    &mut self.params.spoke_width,
                                    unit,
                                    2.0..=2_000.0,
                                );
                                ui.label(language.text("Bolt count", "螺栓孔数"));
                                ui.add(
                                    egui::DragValue::new(&mut self.params.bolt_count).range(3..=10),
                                );
                                ui.label("");
                                ui.end_row();
                                length_parameter_row(
                                    ui,
                                    language.text("Bolt circle", "孔距圆"),
                                    &mut self.params.bolt_circle_diameter,
                                    unit,
                                    1.0..=5_000.0,
                                );
                                length_parameter_row(
                                    ui,
                                    language.text("Bolt hole", "螺栓孔径"),
                                    &mut self.params.bolt_hole_diameter,
                                    unit,
                                    1.0..=1_000.0,
                                );
                            });
                        position_editor(
                            ui,
                            "wheel_rim_position",
                            &mut self.params.position,
                            language,
                            unit,
                        );
                    });
                let footer_top = ui.cursor().top();
                let validation = crate::wheel_rim::validate(&self.params);
                if let Err(error) = &validation {
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 72, 62),
                        localized_wheel_rim_error(language, error),
                    );
                }
                let label = if self.editing_body.is_some() {
                    language.text("Update wheel rim", "更新轮毂")
                } else {
                    language.text("Generate wheel rim", "生成轮毂")
                };
                if ui
                    .add_enabled_ui(validation.is_ok(), |ui| {
                        ui.add_sized([ui.available_width(), 34.0], egui::Button::new(label))
                    })
                    .inner
                    .clicked()
                {
                    command = Some(PluginCommand::GenerateWheelRim {
                        params: self.params,
                        replace_body: self.editing_body,
                    });
                }
                let measured = ui.cursor().top() - footer_top;
                ctx.data_mut(|d| d.insert_temp(footer_id, measured));
                if footer_height.is_none_or(|h| (h - measured).abs() > 0.5) {
                    ctx.request_discard("plugin footer height changed");
                }
            });
        self.open = open;
        command
    }
}

fn length_parameter_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    unit: UiUnit,
    range: std::ops::RangeInclusive<f64>,
) {
    ui.label(label);
    gear_length_value(ui, value, unit, 0.5, range);
    ui.label(unit.symbol());
    ui.end_row();
}

fn position_editor(
    ui: &mut egui::Ui,
    id: &'static str,
    position: &mut [f64; 3],
    language: UiLanguage,
    unit: UiUnit,
) {
    ui.collapsing(language.text("Position", "位置"), |ui| {
        egui::Grid::new(id).num_columns(3).show(ui, |ui| {
            for axis in 0..3 {
                ui.label(["X", "Y", "Z"][axis]);
                gear_length_value(
                    ui,
                    &mut position[axis],
                    unit,
                    1.0,
                    -1_000_000.0..=1_000_000.0,
                );
                ui.label(unit.symbol());
                ui.end_row();
            }
        });
    });
}

fn tire_pattern_label(pattern: TireTreadPattern, language: UiLanguage) -> &'static str {
    match (pattern, language) {
        (TireTreadPattern::HighwayRib, UiLanguage::English) => "Highway rib",
        (TireTreadPattern::HighwayRib, UiLanguage::ChineseSimplified) => "公路纵向胎纹",
        (TireTreadPattern::Block, UiLanguage::English) => "Block tread",
        (TireTreadPattern::Block, UiLanguage::ChineseSimplified) => "块状胎纹",
        (TireTreadPattern::Chevron, UiLanguage::English) => "Chevron tread",
        (TireTreadPattern::Chevron, UiLanguage::ChineseSimplified) => "人字胎纹",
        (TireTreadPattern::AllTerrain, UiLanguage::English) => "All-terrain",
        (TireTreadPattern::AllTerrain, UiLanguage::ChineseSimplified) => "全地形胎纹",
    }
}

fn wheel_rim_style_label(style: WheelRimStyle, language: UiLanguage) -> &'static str {
    match (style, language) {
        (WheelRimStyle::FiveSpoke, UiLanguage::English) => "Five spoke",
        (WheelRimStyle::FiveSpoke, UiLanguage::ChineseSimplified) => "五辐",
        (WheelRimStyle::SixSpoke, UiLanguage::English) => "Six spoke",
        (WheelRimStyle::SixSpoke, UiLanguage::ChineseSimplified) => "六辐",
        (WheelRimStyle::SplitFive, UiLanguage::English) => "Split five spoke",
        (WheelRimStyle::SplitFive, UiLanguage::ChineseSimplified) => "双五辐",
        (WheelRimStyle::MultiSpoke, UiLanguage::English) => "Multi-spoke",
        (WheelRimStyle::MultiSpoke, UiLanguage::ChineseSimplified) => "多辐",
    }
}

fn texture_pattern_label(pattern: SurfaceTexturePattern, language: UiLanguage) -> &'static str {
    match (pattern, language) {
        (SurfaceTexturePattern::Straight, UiLanguage::English) => "Straight knurl",
        (SurfaceTexturePattern::Straight, UiLanguage::ChineseSimplified) => "直纹滚花",
        (SurfaceTexturePattern::DiagonalLeft, UiLanguage::English) => "Left diagonal",
        (SurfaceTexturePattern::DiagonalLeft, UiLanguage::ChineseSimplified) => "左斜纹",
        (SurfaceTexturePattern::DiagonalRight, UiLanguage::English) => "Right diagonal",
        (SurfaceTexturePattern::DiagonalRight, UiLanguage::ChineseSimplified) => "右斜纹",
        (SurfaceTexturePattern::Diamond, UiLanguage::English) => "Diamond knurl",
        (SurfaceTexturePattern::Diamond, UiLanguage::ChineseSimplified) => "菱形滚花",
    }
}

fn draw_tire_preview(ui: &mut egui::Ui, pattern: TireTreadPattern) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 108.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(8),
        egui::Color32::from_rgb(239, 243, 248),
    );
    let center = rect.center();
    let outer = rect.height() * 0.40;
    ui.painter()
        .circle_filled(center, outer, egui::Color32::from_rgb(61, 68, 78));
    ui.painter()
        .circle_filled(center, outer * 0.47, egui::Color32::from_rgb(239, 243, 248));
    let strokes = match pattern {
        TireTreadPattern::HighwayRib => 18,
        TireTreadPattern::Block => 14,
        TireTreadPattern::Chevron => 12,
        TireTreadPattern::AllTerrain => 10,
    };
    for index in 0..strokes {
        let angle = std::f32::consts::TAU * index as f32 / strokes as f32;
        let tangent = egui::vec2(-angle.sin(), angle.cos());
        let radial = egui::vec2(angle.cos(), angle.sin());
        let p = center + radial * outer * 0.82;
        let slant = if matches!(
            pattern,
            TireTreadPattern::Chevron | TireTreadPattern::AllTerrain
        ) {
            radial * 4.0
        } else {
            egui::Vec2::ZERO
        };
        ui.painter().line_segment(
            [p - tangent * 5.0 - slant, p + tangent * 5.0 + slant],
            egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(155, 174, 195)),
        );
    }
}

fn draw_wheel_rim_preview(ui: &mut egui::Ui, style: WheelRimStyle) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 116.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(8),
        egui::Color32::from_rgb(239, 243, 248),
    );
    let center = rect.center();
    let radius = rect.height() * 0.40;
    ui.painter().circle_stroke(
        center,
        radius,
        egui::Stroke::new(5.0_f32, egui::Color32::from_rgb(96, 116, 138)),
    );
    ui.painter()
        .circle_filled(center, radius * 0.18, egui::Color32::from_rgb(96, 116, 138));
    let (groups, split) = match style {
        WheelRimStyle::FiveSpoke => (5, false),
        WheelRimStyle::SixSpoke => (6, false),
        WheelRimStyle::SplitFive => (5, true),
        WheelRimStyle::MultiSpoke => (12, false),
    };
    for group in 0..groups {
        let angle = std::f32::consts::TAU * group as f32 / groups as f32;
        for offset in if split { vec![-0.10, 0.10] } else { vec![0.0] } {
            let direction = egui::vec2((angle + offset).cos(), (angle + offset).sin());
            ui.painter().line_segment(
                [
                    center + direction * radius * 0.16,
                    center + direction * radius * 0.78,
                ],
                egui::Stroke::new(
                    if split { 3.0_f32 } else { 5.0_f32 },
                    egui::Color32::from_rgb(93, 125, 158),
                ),
            );
        }
    }
}

fn draw_texture_preview(ui: &mut egui::Ui, pattern: SurfaceTexturePattern) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 92.0), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(8),
        egui::Color32::from_rgb(223, 230, 238),
    );
    let draw_family = |slope: f32| {
        for index in -12..=12 {
            let offset = index as f32 * 12.0;
            let start = egui::pos2(
                rect.left() - 50.0,
                rect.center().y + offset - slope * rect.width(),
            );
            let end = egui::pos2(
                rect.right() + 50.0,
                rect.center().y + offset + slope * rect.width(),
            );
            painter.line_segment(
                [start, end],
                egui::Stroke::new(1.6_f32, egui::Color32::from_rgb(76, 111, 145)),
            );
        }
    };
    match pattern {
        SurfaceTexturePattern::Straight => draw_family(0.0),
        SurfaceTexturePattern::DiagonalLeft => draw_family(0.35),
        SurfaceTexturePattern::DiagonalRight => draw_family(-0.35),
        SurfaceTexturePattern::Diamond => {
            draw_family(0.35);
            draw_family(-0.35);
        }
    }
}

fn localized_tire_error(language: UiLanguage, error: &str) -> String {
    if language == UiLanguage::English {
        return error.to_owned();
    }
    match error {
        "Position must be finite" => "位置参数必须是有限数值",
        "Outer diameter must be greater than 20 mm" => "外径必须大于 20 mm",
        "Rim diameter must remain below the tire outer diameter" => "轮辋直径必须小于轮胎外径",
        "Section width must be greater than 4 mm" => "断面宽必须大于 4 mm",
        "Section width is too large for the selected diameter" => "断面宽相对于外径过大",
        "Tread depth must be less than 35% of sidewall height" => "胎纹深度必须小于胎侧高度的 35%",
        "Tread repeats must be between 8 and 120" => "胎纹重复数必须介于 8 和 120",
        _ => error,
    }
    .to_owned()
}

fn localized_wheel_rim_error(language: UiLanguage, error: &str) -> String {
    if language == UiLanguage::English {
        return error.to_owned();
    }
    match error {
        "Position must be finite" => "位置参数必须是有限数值",
        "Rim diameter must be greater than 30 mm" => "轮毂直径必须大于 30 mm",
        "Rim width must be greater than 4 mm" => "轮毂宽度必须大于 4 mm",
        "Rim thickness must be between 1 mm and 35% of radius" => {
            "轮缘厚度必须介于 1 mm 和半径的 35%"
        }
        "Hub diameter must leave material around the center bore" => {
            "轮心直径必须在中心孔周围保留材料"
        }
        "Hub must remain inside the rim barrel" => "轮心必须位于轮缘内部",
        "Spoke width is outside the supported range" => "辐条宽度超出支持范围",
        "Bolt count must be between 3 and 10" => "螺栓孔数必须介于 3 和 10",
        "Bolt circle and holes must fit inside the hub" => "孔距圆和螺栓孔必须位于轮心内部",
        _ => error,
    }
    .to_owned()
}

fn localized_texture_error(language: UiLanguage, error: &str) -> String {
    if language == UiLanguage::English {
        return error.to_owned();
    }
    match error {
        "Pitch must be greater than 0.05 mm" => "节距必须大于 0.05 mm",
        "Groove width must be positive and smaller than 90% of pitch" => {
            "槽宽必须为正数且小于节距的 90%"
        }
        "Depth must be greater than 0.01 mm" => "深度必须大于 0.01 mm",
        "Knurl angle must be between 5° and 85°" => "滚花角度必须介于 5° 和 85°",
        "Margin cannot be negative" => "边缘留白不能为负数",
        "Selected planar face is too small for this pitch and margin" => {
            "所选平面相对于当前节距和留白过小"
        }
        "Selected cylinder is too small for this depth, pitch and margin" => {
            "所选圆柱相对于当前深度、节距和留白过小"
        }
        _ => error,
    }
    .to_owned()
}

fn gear_kind_label(kind: GearKind, language: UiLanguage) -> &'static str {
    match (kind, language) {
        (GearKind::Spur, UiLanguage::English) => "Spur gear",
        (GearKind::Spur, UiLanguage::ChineseSimplified) => "直齿圆柱齿轮",
        (GearKind::Helical, UiLanguage::English) => "Helical gear",
        (GearKind::Helical, UiLanguage::ChineseSimplified) => "斜齿圆柱齿轮",
        (GearKind::Bevel, UiLanguage::English) => "Bevel gear",
        (GearKind::Bevel, UiLanguage::ChineseSimplified) => "锥齿轮",
        (GearKind::Internal, UiLanguage::English) => "Internal gear",
        (GearKind::Internal, UiLanguage::ChineseSimplified) => "内齿圈",
    }
}

fn gear_length_value(
    ui: &mut egui::Ui,
    millimeters: &mut f64,
    unit: UiUnit,
    speed_mm: f64,
    range_mm: std::ops::RangeInclusive<f64>,
) -> egui::Response {
    let mut display = unit.from_mm(*millimeters);
    let response = ui.add(
        egui::DragValue::new(&mut display)
            .speed(unit.from_mm(speed_mm).abs().max(1e-6))
            .range(unit.from_mm(*range_mm.start())..=unit.from_mm(*range_mm.end())),
    );
    if response.changed() {
        *millimeters = unit.to_mm(display);
    }
    response
}

fn draw_gear_preview(ui: &mut egui::Ui, params: &GearParams) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 124.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(8),
        egui::Color32::from_rgb(239, 243, 248),
    );
    let center = rect.center();
    let outer = rect.height().min(rect.width()) * 0.40;
    let root = outer * 0.82;
    let shown_teeth = params.teeth.clamp(8, 48) as usize;
    let mut points = Vec::with_capacity(shown_teeth * 4);
    for tooth in 0..shown_teeth {
        for step in 0..4 {
            let radius = if step == 1 || step == 2 { outer } else { root };
            let angle =
                std::f32::consts::TAU * (tooth as f32 + step as f32 / 4.0) / shown_teeth as f32;
            points.push(center + egui::vec2(angle.cos(), angle.sin()) * radius);
        }
    }
    points.push(points[0]);
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(74, 121, 168)),
    ));
    let bore_ratio = if params.kind == GearKind::Internal {
        0.55
    } else {
        (params.bore_diameter / crate::gear::metrics(params).outside_diameter).clamp(0.04, 0.65)
            as f32
    };
    ui.painter().circle_stroke(
        center,
        outer * bore_ratio,
        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(104, 127, 151)),
    );
}

fn localized_gear_error(language: UiLanguage, error: &str) -> String {
    if language == UiLanguage::English {
        return error.to_owned();
    }
    if error.starts_with("Tooth count must be between ") {
        return error
            .replace("Tooth count must be between ", "齿数必须介于 ")
            .replace(" and ", " 和 ");
    }
    match error {
        "Module must be between 0.1 and 50 mm" => "模数必须介于 0.1 和 50 mm".into(),
        "Pressure angle must be between 14° and 30°" => "压力角必须介于 14° 和 30°".into(),
        "Face width must be greater than zero" => "齿宽必须大于零".into(),
        "Backlash is outside the supported range" => "齿侧间隙超出支持范围".into(),
        "Position must be finite" => "位置参数必须是有限数值".into(),
        "Bore diameter must remain inside the root diameter" => "孔径必须小于齿根圆直径".into(),
        "Helix angle must be between -45° and 45°" => "螺旋角必须介于 -45° 和 45°".into(),
        "Bevel angle must be between 5° and 60°" => "锥角必须介于 5° 和 60°".into(),
        "Face width and bevel angle collapse the small end" => "当前齿宽和锥角会使小端退化".into(),
        "Ring wall must be at least half a module" => "齿圈壁厚至少应为半个模数".into(),
        _ => error.to_owned(),
    }
}

fn evaluate_expression(text: &str) -> Result<f64, String> {
    evaluate_expression_with_context(text, 0.0, AngleMode::Radians)
}

/// Bounded equation evaluation shares the calculator's elementary functions.
pub(crate) fn evaluate_surface_expression(text: &str, degrees: bool) -> Result<f64, String> {
    if text.len() > 16384 {
        return Err("Expanded expression too long".into());
    }
    let value = evaluate_expression_with_context(
        text,
        0.0,
        if degrees {
            AngleMode::Degrees
        } else {
            AngleMode::Radians
        },
    )?;
    if !value.is_finite() {
        return Err("Surface coordinate is not finite".into());
    }
    Ok(value)
}

/// Bounded equation evaluation shares the calculator's elementary functions.
/// Only the standalone parameter t is substituted, never text inside names.
pub(crate) fn evaluate_curve_expression(text: &str, t: f64, degrees: bool) -> Result<f64, String> {
    if text.len() > 256 {
        return Err("Expression exceeds 256 bytes".into());
    }
    let mut normalized = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| -> Result<(), String> {
        if token.eq_ignore_ascii_case("ans") {
            return Err("Use t, not calculator Ans".into());
        }
        if token.eq_ignore_ascii_case("t") {
            out.push_str("ans");
        } else {
            out.push_str(token);
        }
        token.clear();
        Ok(())
    };
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            token.push(c);
        } else {
            flush(&mut token, &mut normalized)?;
            normalized.push(c);
        }
    }
    flush(&mut token, &mut normalized)?;
    let value = evaluate_expression_with_context(
        &normalized,
        t,
        if degrees {
            AngleMode::Degrees
        } else {
            AngleMode::Radians
        },
    )?;
    if !value.is_finite() {
        return Err("Curve coordinate is not finite".into());
    }
    Ok(value)
}

/// Button-based function entry intentionally starts expressions such as
/// `sin(` and `sqrt(`. Standard calculators accept `=` as confirmation, so
/// close any still-open groups before parsing. Extra closing parentheses stay
/// untouched and are reported as an error by the parser.
fn complete_calculator_parentheses(text: &str) -> String {
    let mut open = 0usize;
    for ch in text.chars() {
        match ch {
            '(' => open += 1,
            ')' if open > 0 => open -= 1,
            _ => {}
        }
    }
    let mut completed = text.to_owned();
    completed.extend(std::iter::repeat_n(')', open));
    completed
}

fn evaluate_expression_with_context(
    text: &str,
    ans: f64,
    angle_mode: AngleMode,
) -> Result<f64, String> {
    evaluate_expression_with_diagnostics(text, ans, angle_mode)
        .map_err(|diagnostic| diagnostic.message)
}

fn evaluate_expression_with_diagnostics(
    text: &str,
    ans: f64,
    angle_mode: AngleMode,
) -> Result<f64, CalculatorDiagnostic> {
    let normalized = text
        .replace('×', "*")
        .replace('÷', "/")
        .replace('−', "-")
        .replace('π', "pi")
        .replace('√', "sqrt");
    let mut parser = ExpressionParser {
        input: normalized.as_bytes(),
        cursor: 0,
        ans,
        angle_mode,
    };
    parser.skip_spaces();
    if parser.input.is_empty() {
        return Err(CalculatorDiagnostic {
            message: "expression is empty".into(),
            position: 1,
        });
    }
    let value = match parser.expression() {
        Ok(value) => value,
        Err(message) => {
            return Err(CalculatorDiagnostic {
                message,
                position: parser.cursor.saturating_add(1).min(normalized.len().max(1)),
            });
        }
    };
    parser.skip_spaces();
    if parser.cursor != parser.input.len() {
        return Err(CalculatorDiagnostic {
            message: "unexpected character".into(),
            position: parser.cursor + 1,
        });
    }
    Ok(value)
}

fn localized_calculator_error(language: UiLanguage, message: &str) -> String {
    if language == UiLanguage::English {
        return message.to_owned();
    }
    match message {
        "expression is empty" => "表达式为空".into(),
        "division by zero" => "不能除以零".into(),
        "invalid power" => "幂运算无效".into(),
        "missing closing parenthesis" => "缺少右括号".into(),
        "invalid function name" => "函数名无效".into(),
        "invalid number" => "数字无效".into(),
        "unexpected character" => "存在无法识别的字符".into(),
        "result is not finite" => "结果不是有限数值".into(),
        _ if message.starts_with("number expected") => "此处需要数字".into(),
        _ if message.starts_with("unknown constant") => "未知常量或函数缺少左括号".into(),
        _ if message.starts_with("missing ')' after") => "函数缺少右括号".into(),
        _ if message.starts_with("unknown function") => "未知函数".into(),
        _ if message.contains("is undefined for this value") => "函数在此数值处无定义".into(),
        _ => message.to_owned(),
    }
}

struct ExpressionParser<'a> {
    input: &'a [u8],
    cursor: usize,
    ans: f64,
    angle_mode: AngleMode,
}

impl ExpressionParser<'_> {
    fn expression(&mut self) -> Result<f64, String> {
        let mut value = self.term()?;
        loop {
            self.skip_spaces();
            if self.take(b'+') {
                value += self.term()?;
            } else if self.take(b'-') {
                value -= self.term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut value = self.unary()?;
        loop {
            self.skip_spaces();
            if self.take(b'*') {
                value *= self.unary()?;
            } else if self.take(b'/') {
                let divisor = self.unary()?;
                if divisor.abs() < f64::EPSILON {
                    return Err("division by zero".to_string());
                }
                value /= divisor;
            } else {
                return Ok(value);
            }
        }
    }

    fn unary(&mut self) -> Result<f64, String> {
        self.skip_spaces();
        if self.take(b'+') {
            return self.unary();
        }
        if self.take(b'-') {
            return Ok(-self.unary()?);
        }
        self.power()
    }

    fn power(&mut self) -> Result<f64, String> {
        let base = self.primary()?;
        self.skip_spaces();
        if self.take(b'^') {
            let exponent = self.unary()?;
            let value = base.powf(exponent);
            if !value.is_finite() {
                return Err("invalid power".to_string());
            }
            Ok(value)
        } else {
            Ok(base)
        }
    }

    fn primary(&mut self) -> Result<f64, String> {
        self.skip_spaces();
        if self.take(b'(') {
            let value = self.expression()?;
            self.skip_spaces();
            if !self.take(b')') {
                return Err("missing closing parenthesis".to_string());
            }
            return Ok(value);
        }
        if self
            .input
            .get(self.cursor)
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        {
            return self.identifier();
        }
        self.number()
    }

    fn identifier(&mut self) -> Result<f64, String> {
        let start = self.cursor;
        while self
            .input
            .get(self.cursor)
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        {
            self.cursor += 1;
        }
        let name = std::str::from_utf8(&self.input[start..self.cursor])
            .map_err(|_| "invalid function name".to_string())?
            .to_ascii_lowercase();
        self.skip_spaces();
        match name.as_str() {
            "pi" => return Ok(std::f64::consts::PI),
            "e" => return Ok(std::f64::consts::E),
            "ans" => return Ok(self.ans),
            _ => {}
        }
        if !self.take(b'(') {
            return Err(format!("unknown constant or missing '(' after {name}"));
        }
        let argument = self.expression()?;
        self.skip_spaces();
        if !self.take(b')') {
            return Err(format!("missing ')' after {name}"));
        }
        self.apply_function(&name, argument)
    }

    fn apply_function(&self, name: &str, argument: f64) -> Result<f64, String> {
        let angle = match self.angle_mode {
            AngleMode::Degrees => argument.to_radians(),
            AngleMode::Radians => argument,
        };
        let value = match name {
            "sin" => angle.sin(),
            "cos" => angle.cos(),
            "tan" => angle.tan(),
            "asin" => {
                let value = argument.asin();
                match self.angle_mode {
                    AngleMode::Degrees => value.to_degrees(),
                    AngleMode::Radians => value,
                }
            }
            "acos" => {
                let value = argument.acos();
                match self.angle_mode {
                    AngleMode::Degrees => value.to_degrees(),
                    AngleMode::Radians => value,
                }
            }
            "atan" => {
                let value = argument.atan();
                match self.angle_mode {
                    AngleMode::Degrees => value.to_degrees(),
                    AngleMode::Radians => value,
                }
            }
            "sqrt" => argument.sqrt(),
            "ln" => argument.ln(),
            "log" => argument.log10(),
            "abs" => argument.abs(),
            "exp" => argument.exp(),
            _ => return Err(format!("unknown function {name}")),
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(format!("{name} is undefined for this value"))
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_spaces();
        let start = self.cursor;
        let mut decimal = false;
        while let Some(ch) = self.input.get(self.cursor).copied() {
            if ch.is_ascii_digit() {
                self.cursor += 1;
            } else if ch == b'.' && !decimal {
                decimal = true;
                self.cursor += 1;
            } else {
                break;
            }
        }
        if self
            .input
            .get(self.cursor)
            .is_some_and(|ch| *ch == b'e' || *ch == b'E')
        {
            let exponent_mark = self.cursor;
            self.cursor += 1;
            if self
                .input
                .get(self.cursor)
                .is_some_and(|ch| *ch == b'+' || *ch == b'-')
            {
                self.cursor += 1;
            }
            let exponent_start = self.cursor;
            while self
                .input
                .get(self.cursor)
                .is_some_and(|ch| ch.is_ascii_digit())
            {
                self.cursor += 1;
            }
            if exponent_start == self.cursor {
                self.cursor = exponent_mark;
            }
        }
        if start == self.cursor {
            return Err(format!("number expected at {}", start + 1));
        }
        std::str::from_utf8(&self.input[start..self.cursor])
            .ok()
            .and_then(|number| number.parse::<f64>().ok())
            .ok_or_else(|| "invalid number".to_string())
    }

    fn skip_spaces(&mut self) {
        while self
            .input
            .get(self.cursor)
            .is_some_and(|ch| ch.is_ascii_whitespace())
        {
            self.cursor += 1;
        }
    }

    fn take(&mut self, expected: u8) -> bool {
        if self.input.get(self.cursor) == Some(&expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
}

fn format_calculator_value(value: f64) -> String {
    if value != 0.0 && (value.abs() >= 1.0e12 || value.abs() < 1.0e-9) {
        return format!("{value:.10e}");
    }
    let text = format!("{value:.12}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_directory_is_relative_to_executable_not_working_directory() {
        assert_eq!(
            plugin_directory(Path::new("portable/S33D/s33d-app.exe")),
            PathBuf::from("portable/S33D/Plug-ins")
        );
        assert_eq!(
            plugin_directory(Path::new("project/target/debug/s33d-app.exe")),
            PathBuf::from("project/Plug-ins")
        );
        assert_eq!(
            plugin_directory(Path::new("project/target/debug/deps/test.exe")),
            PathBuf::from("project/Plug-ins")
        );
    }

    #[test]
    fn plugin_discovery_requires_valid_known_manifests() {
        let root = std::env::temp_dir().join(format!(
            "s33d-plugin-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for (folder, content) in [
            ("a", r#"{"id":"calculator","entry":"builtin:calculator"}"#),
            ("b", r#"{"id":"calculator","entry":"builtin:calculator"}"#),
            ("c", r#"{"id":"gear-generator","entry":"external:evil"}"#),
            ("d", "invalid JSON"),
        ] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
            std::fs::write(root.join(folder).join("plugin.json"), content).unwrap();
        }
        let (ids, notes) = installed_plugin_ids(&root);
        assert_eq!(ids, ["calculator"]);
        assert_eq!(notes.len(), 3);
        std::fs::remove_file(root.join("a/plugin.json")).unwrap();
        std::fs::remove_file(root.join("b/plugin.json")).unwrap();
        assert!(installed_plugin_ids(&root).0.is_empty());
        // This uniquely allocated test directory contains only this test's fixtures.
        std::fs::remove_dir_all(&root).unwrap();
        assert!(!installed_plugin_ids(&root).1.is_empty());
    }

    #[test]
    fn calculator_honors_precedence_and_parentheses() {
        assert_eq!(evaluate_expression("2 + 3 * 4").unwrap(), 14.0);
        assert_eq!(evaluate_expression("(2 + 3) * -4").unwrap(), -20.0);
        assert_eq!(evaluate_expression("2^3^2").unwrap(), 512.0);
    }

    #[test]
    fn scientific_calculator_supports_functions_units_and_ans() {
        assert_eq!(
            evaluate_expression("sqrt(81) + log(100) + 2^3").unwrap(),
            19.0
        );
        let sin_30 = evaluate_expression_with_context("sin(30)", 0.0, AngleMode::Degrees).unwrap();
        assert!((sin_30 - 0.5).abs() < 1.0e-12);
        let ans = evaluate_expression_with_context("ans * 2", 12.5, AngleMode::Radians).unwrap();
        assert_eq!(ans, 25.0);
        assert!((evaluate_expression("pi").unwrap() - std::f64::consts::PI).abs() < 1.0e-12);
    }

    #[test]
    fn calculator_reports_invalid_input() {
        assert!(evaluate_expression("4 / 0").is_err());
        assert!(evaluate_expression("(2 + 3").is_err());
        assert!(evaluate_expression("two").is_err());
        let diagnostic =
            evaluate_expression_with_diagnostics("2 + * 3", 0.0, AngleMode::Radians).unwrap_err();
        assert!(diagnostic.position >= 4);
    }

    #[test]
    fn calculator_keeps_bounded_history_and_supports_memory_register() {
        let mut calculator = CalculatorPlugin::new(true);
        calculator.button("2");
        calculator.button("+");
        calculator.button("3");
        calculator.button("EVAL");
        assert_eq!(calculator.history, vec![("2+3".into(), "5".into())]);
        calculator.button("MPLUS");
        assert_eq!(calculator.memory, 5.0);
        calculator.button("CLEAR");
        calculator.button("MR");
        assert_eq!(calculator.display, "5");
        calculator.button("MMINUS");
        assert_eq!(calculator.memory, 0.0);
        calculator.button("4");
        calculator.button("/");
        calculator.button("0");
        calculator.button("EVAL");
        assert_eq!(calculator.display, "Error");
        assert_eq!(
            calculator.error.as_ref().unwrap().message,
            "division by zero"
        );
    }

    #[test]
    fn calculator_buttons_evaluate_functions_without_manual_closing_parenthesis() {
        let mut calculator = CalculatorPlugin::new(true);
        calculator.button("sin(");
        calculator.button("3");
        calculator.button("0");
        calculator.button("EVAL");
        assert_eq!(calculator.display, "0.5");
        assert!((calculator.last_value - 0.5).abs() < 1.0e-12);

        calculator.button("CLEAR");
        calculator.button("sqrt(");
        calculator.button("8");
        calculator.button("1");
        calculator.button("EVAL");
        assert_eq!(calculator.display, "9");
    }

    #[test]
    fn calculator_display_starts_at_zero_and_collapses_failures_to_error() {
        let mut calculator = CalculatorPlugin::new(true);
        assert_eq!(calculator.display, "0");
        calculator.button("4");
        calculator.button("/");
        calculator.button("0");
        calculator.button("EVAL");
        assert_eq!(calculator.display, "Error");
        calculator.button("7");
        assert_eq!(calculator.display, "7");
    }

    #[test]
    fn calculator_backspace_removes_one_character_and_stops_at_zero() {
        let mut calculator = CalculatorPlugin::new(true);
        calculator.button("1");
        calculator.button("2");
        calculator.button("3");
        calculator.button("BACK");
        assert_eq!(calculator.display, "12");
        calculator.button("BACK");
        calculator.button("BACK");
        assert_eq!(calculator.display, "0");
        calculator.button("BACK");
        assert_eq!(calculator.display, "0");
    }

    #[test]
    fn opening_calculator_enables_it_in_one_action() {
        let mut host = PluginHost::new(false, false, false, false, false, false, false);
        assert!(host.open("calculator"));
        assert!(host.descriptors()[0].enabled);
        assert!(!host.open("missing"));
    }

    #[test]
    fn opening_gear_generator_enables_only_that_plugin() {
        let mut host = PluginHost::new(false, false, false, false, false, false, false);
        assert!(host.open("gear-generator"));
        let descriptors = host.descriptors();
        assert!(!descriptors[0].enabled);
        assert_eq!(descriptors[1].id, "gear-generator");
        assert!(descriptors[1].enabled);
    }

    #[test]
    fn toolbar_toggle_and_disable_follow_one_lifecycle_for_all_plugins() {
        let mut host = PluginHost::new(false, false, false, false, false, false, false);
        assert_eq!(host.descriptors().len(), 7);
        for row in host.descriptors() {
            host.toggle(row.id);
            assert!(
                !host
                    .descriptors()
                    .iter()
                    .find(|p| p.id == row.id)
                    .unwrap()
                    .open
            );
            host.set_enabled(row.id, true);
            host.toggle(row.id);
            assert!(
                host.descriptors()
                    .iter()
                    .find(|p| p.id == row.id)
                    .unwrap()
                    .open
            );
            host.toggle(row.id);
            assert!(
                !host
                    .descriptors()
                    .iter()
                    .find(|p| p.id == row.id)
                    .unwrap()
                    .open
            );
            host.open(row.id);
            host.set_enabled(row.id, false);
            let row = host
                .descriptors()
                .into_iter()
                .find(|p| p.id == row.id)
                .unwrap();
            assert!(!row.enabled && !row.open);
        }
    }
}

#[cfg(test)]
#[path = "display_preview_qa.rs"]
mod display_preview_qa;
