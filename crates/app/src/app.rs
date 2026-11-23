//! S33D 应用核心：文档 + 内核 + 相机 + 场景组装 + 命令。

use crate::agent_protocol::{
    parse_params, protocol_description, user_ui_state_path, AgentRequest, AgentResponse,
    AgentServer, AgentSketchEntity, BodyIdParams, BodyIdsParams, BodyRenameParams,
    BodyVisibilityParams, BooleanApplyParams, DesktopLifetime, ExportParams, ExtrudeFeatureParams,
    PathParams, PrimitiveCreateParams, RevolveFeatureParams, SelectionSetParams,
    SketchCreateParams, SketchTrimParams, TransformParams, DEFAULT_AGENT_PORT, UI_STATE_FILE_NAME,
};
use crate::camera::Camera;
#[cfg(test)]
#[path = "agent_photo_journey_qa.rs"]
mod agent_photo_journey_qa;
#[path = "agent_visual.rs"]
pub mod agent_visual;
#[cfg(test)]
#[path = "agent_visual_qa.rs"]
mod agent_visual_qa;
#[path = "axis_align.rs"]
pub mod axis_align;
#[path = "cylinder_edit.rs"]
mod cylinder_edit;
#[path = "equation_curve.rs"]
pub mod equation_curve;
#[path = "equation_surface.rs"]
pub mod equation_surface;
#[path = "exact_tools.rs"]
pub mod exact_tools;
#[path = "export_jobs.rs"]
mod export_jobs;
#[path = "model_progress.rs"]
pub mod model_progress;
#[cfg(test)]
#[path = "model_progress_qa.rs"]
mod model_progress_qa;
#[path = "object_groups.rs"]
mod object_groups;
#[path = "pocket_bottom.rs"]
mod pocket_bottom;
#[path = "project_session.rs"]
pub mod project_session;
#[cfg(test)]
#[path = "region_integration_qa.rs"]
mod region_integration_qa;
#[path = "region_mesh_cache.rs"]
mod region_mesh_cache;
#[path = "region_selection.rs"]
mod region_selection;
#[path = "section_interaction.rs"]
pub(crate) mod section_interaction;
#[cfg(test)]
#[path = "selection_error_qa.rs"]
mod selection_error_qa;
#[path = "selection_gestures.rs"]
mod selection_gestures;
#[path = "thread_state.rs"]
mod thread_state;
#[cfg(test)]
#[path = "thread_surfaces_qa.rs"]
mod thread_surfaces_qa;
#[path = "tool_exit_prompt.rs"]
mod tool_exit_prompt;
use crate::mode::{AlignMode, Mode};
use crate::plugins::{PluginCommand, PluginHost};
use crate::render3d::{
    make_grid_lines, SceneData, SceneLines, SceneMesh, ScenePlane, SceneRegionSurface,
};
use crate::sketch_session::{SketchSession, SketchSnapshot, SketchTool};
use eframe::egui;
use s33d_kernel::{
    Axis2D, DatumPlane, EdgeCutterSegment, GeometryKernel, MeshData, PlaneDef, Region2D, ShapeId,
    TruckKernel,
};
use s33d_model::{
    Body, BodyId, BodyKind, DerivedBooleanOp, DerivedOperation, Document, DocumentArchive,
    ExtrudeParams, Feature, GearParams, Material, Primitive3DKind, Primitive3DParams,
    RevolveParams, SketchEntity, SketchFeature, SurfaceTextureParams, SurfaceTextureSurface,
    TireParams, WheelRimParams,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const S33D_PROJECT_FORMAT: &str = "S33D Project";
const S33D_PROJECT_VERSION: u32 = 2;
type AgentCommandResult = Result<Value, (String, String)>;
type CylinderPickCacheEntry = (Vec<(BodyId, Option<ShapeId>)>, Vec<crate::threads::Target>);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArchivedShape {
    old_id: ShapeId,
    mesh: MeshData,
    /// Version 1 archives only had meshes. Version 2 retains native surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    occt_brep: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectViewState {
    eye: [f32; 3],
    target: [f32; 3],
    up: [f32; 3],
    fov_y_deg: f32,
    ortho: bool,
    ortho_half: f32,
    near: f32,
    far: f32,
}

impl ProjectViewState {
    fn from_camera(camera: &Camera) -> Self {
        Self {
            eye: camera.eye.to_array(),
            target: camera.target.to_array(),
            up: camera.up.to_array(),
            fov_y_deg: camera.fov_y_deg,
            ortho: camera.ortho,
            ortho_half: camera.ortho_half,
            near: camera.near,
            far: camera.far,
        }
    }

    fn into_camera(self) -> Camera {
        Camera {
            eye: glam::Vec3::from_array(self.eye),
            target: glam::Vec3::from_array(self.target),
            up: glam::Vec3::from_array(self.up),
            fov_y_deg: self.fov_y_deg,
            ortho: self.ortho,
            ortho_half: self.ortho_half,
            near: self.near,
            far: self.far,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct S33DProjectFile {
    format: String,
    version: u32,
    document: DocumentArchive,
    shapes: Vec<ArchivedShape>,
    history: Vec<String>,
    selected: Vec<BodyId>,
    camera: ProjectViewState,
    show_grid: bool,
    snap_enabled: bool,
    snap_step: f64,
}

fn encode_s33d_project(
    document: &Document,
    kernel: &TruckKernel,
    history: &[String],
    selected: &[BodyId],
    camera: &Camera,
    show_grid: bool,
    snap_enabled: bool,
    snap_step: f64,
) -> Result<String, String> {
    let archive = document.archive();
    let mut shapes = Vec::new();
    for old_id in archive.shape_ids() {
        let mesh = kernel
            .tessellate(old_id)
            .map_err(|error| format!("cannot save geometry {old_id}: {error}"))?;
        let occt_brep = kernel.exact_shape(old_id).map(|shape| shape.brep.clone());
        shapes.push(ArchivedShape {
            old_id,
            mesh,
            occt_brep,
        });
    }
    serde_json::to_string_pretty(&S33DProjectFile {
        format: S33D_PROJECT_FORMAT.to_string(),
        version: S33D_PROJECT_VERSION,
        document: archive,
        shapes,
        history: history.to_vec(),
        selected: selected.to_vec(),
        camera: ProjectViewState::from_camera(camera),
        show_grid,
        snap_enabled,
        snap_step,
    })
    .map_err(|error| format!("cannot encode project: {error}"))
}

fn decode_s33d_project(
    text: &str,
) -> Result<
    (
        Document,
        TruckKernel,
        Vec<String>,
        Vec<BodyId>,
        Camera,
        bool,
        bool,
        f64,
    ),
    String,
> {
    let mut file: S33DProjectFile =
        serde_json::from_str(text).map_err(|error| format!("invalid .s33d file: {error}"))?;
    if file.format != S33D_PROJECT_FORMAT {
        return Err("this file is not an S33D project".to_string());
    }
    if !(1..=S33D_PROJECT_VERSION).contains(&file.version) {
        return Err(format!("unsupported S33D project version {}", file.version));
    }
    let mut kernel = TruckKernel::new();
    let mut shape_map = HashMap::new();
    for archived in file.shapes {
        let new_id = if let Some(brep) = archived.occt_brep {
            kernel.restore_exact(&brep).map_err(|error| {
                format!("Cannot restore exact geometry {}: {error}", archived.old_id)
            })?
        } else {
            kernel.insert_mesh(archived.mesh)
        };
        shape_map.insert(archived.old_id, new_id);
    }
    file.document.remap_shapes(&shape_map)?;
    // Display samples are disposable: the saved equation remains authoritative,
    // including snapshots restored later by Undo / Redo.
    let mut curve_cache: HashMap<String, Vec<[f64; 3]>> = HashMap::new();
    for snapshot in std::iter::once(&mut file.document.current)
        .chain(file.document.undo_stack.iter_mut())
        .chain(file.document.redo_stack.iter_mut())
    {
        for body in &mut snapshot.bodies {
            if let Feature::EquationCurve { params, points } = &mut body.feature {
                let key = serde_json::to_string(params).map_err(|e| e.to_string())?;
                if !curve_cache.contains_key(&key) {
                    curve_cache.insert(key.clone(), equation_curve::sample(params)?);
                }
                *points = curve_cache[&key].clone();
                body.kind = BodyKind::Curve;
                body.shape = None;
            }
        }
    }
    let document = Document::from_archive(file.document);
    let selected = file
        .selected
        .into_iter()
        .filter(|id| document.body(*id).is_some_and(|body| body.visible))
        .collect();
    Ok((
        document,
        kernel,
        file.history,
        selected,
        file.camera.into_camera(),
        file.show_grid,
        file.snap_enabled,
        file.snap_step,
    ))
}

/// 一条棱的引用（世界坐标快照 + 两相邻面法线，供 chamfer/fillet）。
/// 圆形棱被 tessellation 离散成多段：合并为链整体识别、恢复圆后连续加工。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeRef {
    pub body_id: BodyId,
    pub segments: Vec<EdgeSeg>,
}

#[cfg(test)]
#[path = "daily_workflow_qa.rs"]
mod daily_workflow_qa;
#[cfg(test)]
#[path = "pocket_selection_qa.rs"]
mod pocket_selection_qa;
#[cfg(test)]
#[path = "thread_workflow_qa.rs"]
mod thread_workflow_qa;
#[cfg(test)]
#[path = "viewport_recent_qa.rs"]
mod viewport_recent_qa;
#[cfg(test)]
#[path = "workflow_20261113_qa.rs"]
mod workflow_20261113_qa;

/// 棱链中的一段（两端点 + 两相邻面外法线）。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeSeg {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub n1: [f32; 3],
    pub n2: [f32; 3],
}

fn same_chamfer_location(picked: &[EdgeSeg], stored: &[s33d_model::ChamferEdgeSeg]) -> bool {
    let near = |a: [f32; 3], b: [f64; 3]| {
        a.iter()
            .zip(b)
            .all(|(&x, y)| (f64::from(x) - y).abs() <= 1e-4)
    };
    picked.len() == stored.len()
        && picked.iter().all(|p| {
            stored
                .iter()
                .any(|s| (near(p.a, s.a) && near(p.b, s.b)) || (near(p.a, s.b) && near(p.b, s.a)))
        })
}

struct PickGeometry {
    mesh: MeshData,
    chains: Vec<Vec<EdgeSeg>>,
    edge_to_chain: HashMap<(usize, usize), usize>,
}

impl PickGeometry {
    fn new(mut mesh: MeshData) -> Self {
        mesh.weld(1e-5);
        let edge_key = |a, b| if a < b { (a, b) } else { (b, a) };
        let mut normals: HashMap<(usize, usize), Vec<[f32; 3]>> = HashMap::new();
        for t in mesh.indices.chunks_exact(3) {
            let p = |i: u32| glam::Vec3::from_array(mesh.positions[i as usize]);
            let n = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
            if n.length() <= 1e-9 {
                continue;
            }
            let n = n.normalize().to_array();
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let entry = normals.entry(edge_key(a as usize, b as usize)).or_default();
                if entry.len() < 2 {
                    entry.push(n);
                }
            }
        }
        let mut edge_to_chain = HashMap::new();
        let chains = mesh
            .sharp_edge_chains(25.0, 20.0)
            .into_iter()
            .enumerate()
            .map(|(index, chain)| {
                chain
                    .into_iter()
                    .filter_map(|(a, b)| {
                        let key = edge_key(a, b);
                        edge_to_chain.insert(key, index);
                        let ns = normals.get(&key)?;
                        (ns.len() == 2).then(|| EdgeSeg {
                            a: mesh.positions[key.0],
                            b: mesh.positions[key.1],
                            n1: ns[0],
                            n2: ns[1],
                        })
                    })
                    .collect()
            })
            .collect();
        Self {
            mesh,
            chains,
            edge_to_chain,
        }
    }
}

/// 剪贴板条目（Copy/Paste）。
#[derive(Debug, Clone)]
pub struct ClipboardItem {
    pub body: Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UiLanguage {
    #[default]
    English,
    ChineseSimplified,
}

impl UiLanguage {
    pub fn text(self, english: &'static str, chinese: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::ChineseSimplified => chinese,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UiFontChoice {
    #[default]
    Proportional,
    Monospace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UiFontSize {
    Compact,
    #[default]
    Standard,
    Large,
}

impl UiFontSize {
    pub fn scale(self) -> f32 {
        match self {
            Self::Compact => 0.9,
            Self::Standard => 1.0,
            Self::Large => 1.15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UiColorTheme {
    #[default]
    Blue,
    Teal,
    Orange,
}

impl UiColorTheme {
    pub fn accent(self) -> egui::Color32 {
        match self {
            Self::Blue => egui::Color32::from_rgb(37, 118, 221),
            Self::Teal => egui::Color32::from_rgb(13, 137, 130),
            Self::Orange => egui::Color32::from_rgb(211, 103, 30),
        }
    }

    pub fn accent_soft(self) -> egui::Color32 {
        match self {
            Self::Blue => egui::Color32::from_rgb(226, 238, 253),
            Self::Teal => egui::Color32::from_rgb(224, 244, 242),
            Self::Orange => egui::Color32::from_rgb(252, 236, 224),
        }
    }
}

/// User-facing length unit. Geometry and project files always remain in mm;
/// this enum only converts values at the UI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UiUnit {
    #[default]
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Foot,
}

impl UiUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Millimeter => "mm",
            Self::Centimeter => "cm",
            Self::Meter => "m",
            Self::Inch => "in",
            Self::Foot => "ft",
        }
    }

    pub fn name(self, language: UiLanguage) -> &'static str {
        match self {
            Self::Millimeter => language.text("Millimeter", "毫米"),
            Self::Centimeter => language.text("Centimeter", "厘米"),
            Self::Meter => language.text("Meter", "米"),
            Self::Inch => language.text("Inch", "英寸"),
            Self::Foot => language.text("Foot", "英尺"),
        }
    }

    pub fn millimeters_per_unit(self) -> f64 {
        match self {
            Self::Millimeter => 1.0,
            Self::Centimeter => 10.0,
            Self::Meter => 1_000.0,
            Self::Inch => 25.4,
            Self::Foot => 304.8,
        }
    }

    pub fn to_mm(self, value: f64) -> f64 {
        value * self.millimeters_per_unit()
    }

    pub fn from_mm(self, value_mm: f64) -> f64 {
        value_mm / self.millimeters_per_unit()
    }

    pub fn decimals(self) -> usize {
        match self {
            Self::Millimeter => 3,
            Self::Centimeter => 4,
            Self::Meter => 6,
            Self::Inch => 4,
            Self::Foot => 5,
        }
    }

    pub fn format_mm(self, value_mm: f64) -> String {
        format!(
            "{:.*} {}",
            self.decimals(),
            self.from_mm(value_mm),
            self.symbol()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub appearance: crate::appearance::UiAppearance,
    pub last_input_directory: Option<PathBuf>,
    pub last_output_directory: Option<PathBuf>,
    pub recent_projects: Vec<PathBuf>,
    pub language: UiLanguage,
    pub font: UiFontChoice,
    pub font_size: UiFontSize,
    pub color_theme: UiColorTheme,
    pub unit: UiUnit,
    pub show_first_use_tips: bool,
    pub show_tool_exit_confirmation: bool,
    pub beginner_mode: bool,
    pub agent_control_enabled: bool,
    pub agent_control_port: u16,
    pub calculator_plugin_enabled: bool,
    pub gear_plugin_enabled: bool,
    pub surface_texture_plugin_enabled: bool,
    pub tire_plugin_enabled: bool,
    pub wheel_rim_plugin_enabled: bool,
    pub screw_plugin_enabled: bool,
    pub nut_plugin_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: crate::appearance::UiAppearance::GraphiteStudio,
            last_input_directory: None,
            last_output_directory: None,
            recent_projects: Vec::new(),
            language: UiLanguage::English,
            font: UiFontChoice::Proportional,
            font_size: UiFontSize::Standard,
            color_theme: UiColorTheme::Blue,
            unit: UiUnit::Millimeter,
            show_first_use_tips: true,
            show_tool_exit_confirmation: true,
            beginner_mode: false,
            agent_control_enabled: true,
            agent_control_port: DEFAULT_AGENT_PORT,
            calculator_plugin_enabled: false,
            gear_plugin_enabled: false,
            surface_texture_plugin_enabled: false,
            tire_plugin_enabled: false,
            wheel_rim_plugin_enabled: false,
            screw_plugin_enabled: false,
            nut_plugin_enabled: false,
        }
    }
}

impl AppSettings {
    pub fn accent(&self) -> egui::Color32 {
        if self.appearance.is_original() {
            self.color_theme.accent()
        } else {
            self.appearance.palette().accent
        }
    }

    pub fn accent_soft(&self) -> egui::Color32 {
        if self.appearance.is_original() {
            self.color_theme.accent_soft()
        } else {
            self.appearance.palette().accent_soft
        }
    }
}

/// Agent applications that S33D can configure through their supported CLI.
/// Registration only adds or replaces the `s33d` MCP entry; unregistration
/// removes that entry and never touches either application itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentIntegration {
    Codex,
    ClaudeCode,
}

impl AgentIntegration {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
        }
    }

    fn cli_program(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentConfigAction {
    Inspect,
    Register,
    Unregister,
}

fn agent_cli_args(
    integration: AgentIntegration,
    action: AgentConfigAction,
    bridge: &Path,
) -> Vec<OsString> {
    match (integration, action) {
        (AgentIntegration::Codex, AgentConfigAction::Inspect) => ["mcp", "get", "s33d", "--json"]
            .into_iter()
            .map(Into::into)
            .collect(),
        (AgentIntegration::Codex, AgentConfigAction::Register) => vec![
            "mcp".into(),
            "add".into(),
            "s33d".into(),
            "--".into(),
            bridge.as_os_str().to_owned(),
            "mcp".into(),
        ],
        (AgentIntegration::Codex, AgentConfigAction::Unregister) => ["mcp", "remove", "s33d"]
            .into_iter()
            .map(Into::into)
            .collect(),
        (AgentIntegration::ClaudeCode, AgentConfigAction::Inspect) => {
            ["mcp", "get", "s33d"].into_iter().map(Into::into).collect()
        }
        (AgentIntegration::ClaudeCode, AgentConfigAction::Register) => vec![
            "mcp".into(),
            "add".into(),
            "--transport".into(),
            "stdio".into(),
            "s33d".into(),
            "--scope".into(),
            "user".into(),
            "--".into(),
            bridge.as_os_str().to_owned(),
            "mcp".into(),
        ],
        (AgentIntegration::ClaudeCode, AgentConfigAction::Unregister) => vec![
            "mcp".into(),
            "remove".into(),
            "s33d".into(),
            "--scope".into(),
            "user".into(),
        ],
    }
}

fn agent_bridge_executable() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate the S33D application: {error}"))?;
    let file_name = if cfg!(windows) {
        "s33d-agent.exe"
    } else {
        "s33d-agent"
    };
    Ok(executable.with_file_name(file_name))
}

fn run_agent_cli(
    integration: AgentIntegration,
    action: AgentConfigAction,
    bridge: &Path,
) -> std::io::Result<Output> {
    let args = agent_cli_args(integration, action, bridge);
    let result = Command::new(integration.cli_program()).args(&args).output();
    #[cfg(windows)]
    if integration == AgentIntegration::Codex
        && matches!(&result, Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    {
        if let Some(program) = codex_desktop_cli() {
            return Command::new(program).args(&args).output();
        }
    }
    result
}

// Desktop apps can inherit stale PATH values. Discover the installed package
// dynamically; never retry a command that actually ran and returned an error.
#[cfg(windows)]
fn codex_desktop_cli() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    let shell = PathBuf::from(std::env::var_os("SystemRoot")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let output = Command::new(shell)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
            "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); Get-AppxPackage -Name OpenAI.Codex | Sort-Object Version -Descending | ForEach-Object { Join-Path $_.InstallLocation 'app/resources/codex.exe' }"])
        .creation_flags(0x08000000)
        .output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| PathBuf::from(line.trim().trim_start_matches('\u{feff}')))
        .find(|path| path.is_absolute() && path.is_file())
}
fn command_output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}; {stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => "the Agent command returned no details".to_string(),
    }
}

fn output_reports_permission_problem(output: &Output) -> bool {
    let text = command_output_text(output).to_ascii_lowercase();
    [
        "permission denied",
        "access denied",
        "operation not permitted",
        "eacces",
        "eperm",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn output_reports_missing_registration(output: &Output) -> bool {
    let text = command_output_text(output).to_ascii_lowercase();
    [
        "not found",
        "no mcp server named",
        "does not exist",
        "not configured",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct UiStateFile {
    objects_panel_pos_v2: Option<[f32; 2]>,
    pub(crate) settings: AppSettings,
}

const DEBUG_LOG_PATH: &str = "debug/click-log.txt";

fn read_ui_state(path: &Path) -> Option<UiStateFile> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn legacy_ui_state_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        paths.push(current_dir.join(UI_STATE_FILE_NAME));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            for ancestor in parent.ancestors().take(4) {
                let candidate = ancestor.join(UI_STATE_FILE_NAME);
                if !paths.contains(&candidate) {
                    paths.push(candidate);
                }
            }
        }
    }
    paths
}

fn write_ui_state(path: &Path, state: &UiStateFile) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "cannot create settings directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|error| format!("cannot encode S33D settings: {error}"))?;
    std::fs::write(path, json)
        .map_err(|error| format!("cannot save settings to {}: {error}", path.display()))
}

pub(crate) fn load_ui_state() -> UiStateFile {
    if cfg!(test) {
        return UiStateFile::default();
    }
    let stable_path = user_ui_state_path();
    load_ui_state_from_paths(&stable_path, legacy_ui_state_paths())
}

fn load_ui_state_from_paths(
    stable_path: &Path,
    legacy_paths: impl IntoIterator<Item = PathBuf>,
) -> UiStateFile {
    if let Some(state) = read_ui_state(&stable_path) {
        return state;
    }
    for legacy_path in legacy_paths {
        if legacy_path == stable_path {
            continue;
        }
        if let Some(state) = read_ui_state(&legacy_path) {
            let _ = write_ui_state(&stable_path, &state);
            return state;
        }
    }
    UiStateFile::default()
}

fn save_ui_state_checked(pos: Option<egui::Pos2>, settings: &AppSettings) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let state = UiStateFile {
        objects_panel_pos_v2: pos.map(|pos| [pos.x, pos.y]),
        settings: settings.clone(),
    };
    write_ui_state(&user_ui_state_path(), &state)
}

fn save_ui_state(pos: Option<egui::Pos2>, settings: &AppSettings) {
    let _ = save_ui_state_checked(pos, settings);
}

fn append_debug_log(line: &str) {
    let path = std::path::Path::new(DEBUG_LOG_PATH);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
}

/// 布尔上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

impl BoolOp {
    pub fn name(&self) -> &'static str {
        match self {
            BoolOp::Union => "Union",
            BoolOp::Subtract => "Subtract",
            BoolOp::Intersect => "Intersect",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtrudeCtx {
    pub body_id: BodyId,
    /// Immutable source shape and native face; never reuse a display triangle index as topology.
    pub exact_face: Option<(ShapeId, u32)>,
    /// 建模轮廓来源；可以是真实 Sketch，也可以由选中平面 Surface 的边界临时生成。
    pub sketch: SketchFeature,
    pub regions: Vec<Region2D>,
    /// Indices in the persistent source Sketch's deterministic region list.
    pub region_indices: Vec<usize>,
    pub depth: f64,
    pub symmetric: bool,
    /// Preview shapes stay separate because selected closed regions can be
    /// disconnected; forcing preview geometry through a Boolean union can fail
    /// even when every individual extrusion is valid.
    pub ghost_shapes: Vec<ShapeId>,
    /// Exact preview construction error shown by the Properties panel.
    pub preview_error: Option<String>,
    /// Pocket（拉伸切除）：提交 = 从目标体上减掉，不新增 body。
    pub pocket: bool,
    /// Pocket 目标体（未定时可在 Pocket 模式下单击 solid 指定）。
    pub target_body: Option<BodyId>,
    /// 贯穿（Pocket）：深度自动取目标体沿挤出方向的尺寸。
    pub through: bool,
}

#[derive(Debug, Clone)]
pub struct RevolveCtx {
    pub body_id: BodyId,
    /// 旋转轮廓来源；Surface 直接建模时由其固定边界生成。
    pub sketch: SketchFeature,
    pub region: Region2D,
    pub region_index: usize,
    pub axis_point: [f64; 2],
    pub axis_dir: [f64; 2],
    pub angle_deg: f64,
}

#[derive(Debug, Clone)]
pub struct BooleanCtx {
    pub op: BoolOp,
    pub target: Option<BodyId>,
    pub tool: Option<BodyId>,
    /// Subtract / Intersect 当前等待重新拾取的输入槽；Union 不使用。
    pub picking: Option<BooleanOperandSlot>,
    /// 操作完成后是否保留源 objects。
    pub keep_originals: bool,
}

#[derive(Debug, Clone)]
pub struct MirrorCtx {
    /// 进入 Mirror 时锁定的源 objects。
    pub sources: Vec<BodyId>,
    /// 用户在视口中选定的平面 surface。
    pub plane: Option<FaceHit>,
    /// 实际用于内核镜像的无限平面（surface、默认基准面或线性方程）。
    pub plane_def: Option<PlaneDef>,
    pub plane_source: Option<MirrorPlaneSource>,
    /// ax + by + cz = d，依次保存 a、b、c、d。
    pub equation: [f64; 4],
    pub equation_enabled: bool,
    /// Apply 前的镜像预览：(source body, preview shape)。
    pub previews: Vec<(BodyId, ShapeId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorPlaneSource {
    XY,
    YZ,
    ZX,
    Surface,
    Equation,
}

impl MirrorPlaneSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::XY => "XY",
            Self::YZ => "YZ",
            Self::ZX => "ZX",
            Self::Surface => "Surface",
            Self::Equation => "Equation",
        }
    }
}

/// Split（平面切分）会话：一个 object + 一个切割平面。
#[derive(Debug, Clone)]
pub struct SplitCtx {
    /// 待切分的源 object。
    pub target: Option<BodyId>,
    /// true 时视口下一次单击用于重选 object（Properties 面板 Object 字段激活）。
    pub picking: bool,
    /// 用户在视口中选定的平面 surface。
    pub plane: Option<FaceHit>,
    /// 实际用于内核切分的无限平面（surface、默认基准面或线性方程）。
    pub plane_def: Option<PlaneDef>,
    pub plane_source: Option<MirrorPlaneSource>,
    /// ax + by + cz = d，依次保存 a、b、c、d。
    pub equation: [f64; 4],
    pub equation_enabled: bool,
    /// Apply 前的切分预览（每个连通件一个 shape；先法线正向侧后反向侧）。
    pub previews: Vec<ShapeId>,
    /// 提交后是否保留源 object。
    pub keep_originals: bool,
}

/// Section View（剖面视图）会话：沿平面剖切显示所有 visible solids。
#[derive(Debug, Clone)]
pub struct SectionViewCtx {
    pub drag_start: Option<(egui::Pos2, PlaneDef)>,
    /// 用户在视口中选定的平面 surface。
    pub plane: Option<FaceHit>,
    /// 实际用于剖切的无限平面（surface、默认基准面或线性方程）。
    pub plane_def: Option<PlaneDef>,
    pub plane_source: Option<MirrorPlaneSource>,
    /// ax + by + cz = d，依次保存 a、b、c、d。
    pub equation: [f64; 4],
    pub equation_enabled: bool,
    /// true = 显示法线反向侧，false = 显示法线正向侧。
    pub flip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Draw3DPlacementStage {
    ChoosePrimitive,
    AwaitBase,
    Sizing,
    Precision,
}

/// Direct-manipulation handles shown on a Draw 3D preview after its initial
/// two-point placement. The center handle moves the base point; the remaining
/// handles edit the same parameters shown in Properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Draw3DDragKind {
    Move,
    Width,
    Depth,
    PlanarSize,
    Height,
    Radius,
    MajorRadius,
    MinorRadius,
}

#[derive(Debug, Clone, Copy)]
pub struct Draw3DDirectDrag {
    pub kind: Draw3DDragKind,
    pub pointer_start: egui::Pos2,
    pub params_start: Primitive3DParams,
}

#[derive(Debug, Clone)]
pub struct Insert3DCtx {
    pub params: Option<Primitive3DParams>,
    /// Interactive preview; Sphere Apply replaces its lightweight mesh with
    /// an exact shape. Only accepted previews can become committed bodies.
    pub preview_shape: Option<ShapeId>,
    /// Some 时正在原位编辑一个既有 Draw 3D body；提交前不改写 Document。
    pub editing_body: Option<BodyId>,
    /// 新建基础体的两次点击流程：选类型 → 定基点 → 移动定尺寸 → 精确参数。
    pub stage: Draw3DPlacementStage,
    pub anchor: Option<[f64; 3]>,
    /// 参数修改后必须重新 Apply，Done 才能提交。
    pub parameters_applied: bool,
    /// Precision 阶段的工作区直接拖动；只改 preview，不提前写入 Document。
    pub direct_drag: Option<Draw3DDirectDrag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOperandSlot {
    Target,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectAllPolicy {
    SelectVisibleSolids,
    ClearWithNotice(&'static str),
}

fn select_all_policy(
    mode: Mode,
    boolean_op: Option<BoolOp>,
    _gizmo: Option<GizmoMode>,
    array_active: bool,
    chamfer_active: bool,
) -> SelectAllPolicy {
    match mode {
        Mode::Idle => {
            if array_active {
                return SelectAllPolicy::ClearWithNotice("Array accepts exactly one object");
            }
            if chamfer_active {
                return SelectAllPolicy::ClearWithNotice(
                    "Chamfer/Fillet uses one object's edge selection",
                );
            }
            SelectAllPolicy::SelectVisibleSolids
        }
        Mode::Boolean if boolean_op == Some(BoolOp::Union) => SelectAllPolicy::SelectVisibleSolids,
        Mode::Boolean => {
            SelectAllPolicy::ClearWithNotice("Subtract and Intersect use two ordered object fields")
        }
        Mode::Split => SelectAllPolicy::ClearWithNotice("Split uses one object field"),
        _ => SelectAllPolicy::ClearWithNotice(
            "finish or cancel the current modeling operation before selecting all objects",
        ),
    }
}

impl GizmoMode {
    pub fn name(&self) -> &'static str {
        match self {
            GizmoMode::Translate => "Move",
            GizmoMode::Rotate => "Rotate",
            GizmoMode::Scale => "Scale",
        }
    }
}

/// 3D 实体表面平面（面拾取结果）。
#[derive(Debug, Clone)]
pub struct FaceHit {
    pub body_id: BodyId,
    /// 面法线（世界空间，单位向量）。
    pub normal: [f32; 3],
    /// 面中心（世界空间，面上一点）。
    pub center: [f32; 3],
    /// patch 三角形索引（对应 body 网格）。
    pub triangles: Vec<u32>,
    /// Area measured in the same display-mesh index space as `triangles`.
    ///
    /// Imported meshes are welded and split at hard shading edges before
    /// picking, so these indices are deliberately not reused against the
    /// kernel's raw tessellation (the two vertex tables can differ).
    pub area_mm2: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationAxis {
    /// 轴线上一点（世界坐标）。
    pub point: [f32; 3],
    /// 单位方向（世界坐标）。
    pub direction: [f32; 3],
    /// 视口中用于反馈拾取结果的原始线段起点。
    pub display_start: [f32; 3],
    /// 视口中用于反馈拾取结果的原始线段终点。
    pub display_end: [f32; 3],
}
/// gizmo 拖拽中的单个 object 快照。
#[derive(Debug, Clone)]
struct GizmoDragItem {
    body_id: BodyId,
    /// 拖拽起点时的 shape（变换基准，B-rep 保持）。
    start_shape: ShapeId,
    /// 拖拽中的当前 shape（临时 mesh，拖动结束删除）。
    temp_shape: Option<ShapeId>,
}

/// gizmo 拖拽状态（左键拖拽整组选中 objects）。
#[derive(Debug, Clone)]
pub struct GizmoDrag {
    items: Vec<GizmoDragItem>,
    pivot: glam::Vec3,
    last_pos: egui::Pos2,
    /// 累积变换矩阵。
    mat: glam::Mat4,
}

/// 相机拖拽状态。
#[derive(Debug, Clone, Copy)]
pub enum CamDrag {
    Orbit { last: egui::Pos2 },
    Pan { last: egui::Pos2 },
}

#[derive(Debug, Clone)]
struct ClickSelectionSnapshot {
    operation: selection_gestures::ClickToolState,
    sidebar: selection_gestures::SidebarClickState,
    camera: Camera,
    bodies: Vec<BodyId>,
    regions: Vec<usize>,
    region_source: Option<region_selection::RegionSelectionSource>,
    primary_edge: Option<EdgeRef>,
    primary_face: Option<FaceHit>,
}

/// Section View 剖切缓存项：一个 shape 沿一个平面切出的两个闭合半体。
#[derive(Debug, Clone)]
pub struct CachedSectionSplit {
    intersects: bool,
    positive: MeshData,
    negative: MeshData,
}

pub struct S33DApp {
    pub(crate) background_modeling: bool,
    pub(crate) model_job: Option<model_progress::ModelJob>,
    pub(crate) export_job: Option<export_jobs::ExportJob>,
    model_work: Option<model_progress::WorkProgress>,
    // Exit requests survive asynchronous native geometry work.
    pending_tool_exit: Option<Option<tool_exit_prompt::WorkspaceExitClick>>,
    tool_exit_prompt: Option<tool_exit_prompt::ToolExitPrompt>,
    confirmed_tool_exit_click: bool,
    workspace_click_modifiers: Option<egui::Modifiers>,
    replaying_tool_exit: bool,
    array_exit_applied: Option<String>,
    exit_move_to_baseline: [f32; 3],
    exit_scale_size_baseline: [f32; 3],
    pub step_import_job: Option<exact_tools::StepImportJob>,
    pub face_extrude_job: Option<exact_tools::FaceExtrudeJob>,
    pub offset_ctx: Option<exact_tools::OffsetContext>,
    pub curve_ctx: Option<equation_curve::CurveContext>,
    pub surface_ctx: Option<equation_surface::SurfaceContext>,
    pub doc: Document,
    pub kernel: TruckKernel,
    pub camera: Camera,
    pub scene: Arc<Mutex<SceneData>>,
    pub renderer: Arc<Mutex<Option<crate::render3d::GpuRenderer>>>,
    pub mode: Mode,
    pub sketch: Option<SketchSession>,
    /// Persistent Sketch body currently being edited. Unlike `selected`, this
    /// remains stable while individual sketch entities are selected.
    pub sketch_edit_body: Option<BodyId>,
    /// Snapshot held across an Edit Sketch session. Live dependent rebuilds
    /// update the viewport before Done; Cancel restores this document, while
    /// Done converts it into one ordinary undo transaction.
    sketch_edit_original_doc: Option<Document>,
    sketch_edit_original_selection: Vec<BodyId>,
    pub selected: Vec<BodyId>,
    pub object_list_anchor: Option<BodyId>,
    pub thread_params: crate::threads::Params,
    pub thread_ui: crate::threads::config::UiState,
    pub thread_choices: Vec<crate::threads::Target>,
    pub thread_checked: Vec<bool>,
    pub open_thread_panel: bool,
    cylinder_pick_cache: HashMap<ShapeId, CylinderPickCacheEntry>,
    pub selected_regions: Vec<usize>,
    selected_region_source: Option<region_selection::RegionSelectionSource>,
    pub hover_body: Option<BodyId>,
    pub extrude_ctx: Option<ExtrudeCtx>,
    pub revolve_ctx: Option<RevolveCtx>,
    pub boolean_ctx: Option<BooleanCtx>,
    pub split_ctx: Option<SplitCtx>,
    pub section_view_ctx: Option<SectionViewCtx>,
    pub mirror_ctx: Option<MirrorCtx>,
    pub insert_3d_ctx: Option<Insert3DCtx>,
    pub gizmo: Option<GizmoMode>,
    pub gizmo_drag: Option<GizmoDrag>,
    /// 选中的 3D 表面（双击 solid 表面选中，可作为 sketch 平面）。
    pub face_pick: Option<FaceHit>,
    /// 悬停识别的 3D 表面（Idle 模式，非选中）。
    pub hover_face: Option<FaceHit>,
    /// sketch 区域缓存：body -> (实体指纹, 区域)，避免每帧重复平面分割。
    pub regions_cache: std::collections::HashMap<BodyId, (u64, Vec<Region2D>)>,
    region_mesh_cache: region_mesh_cache::RegionMeshCache,
    /// Align 面板开关。
    pub align_active: bool,
    pub axis_align: axis_align::AxisAlign,
    pub fastener_job: Option<crate::fasteners::Job>,
    /// Align 当前轴（0=X 1=Y 2=Z）。
    pub align_axis: usize,
    /// Align 当前方式。
    pub align_mode: AlignMode,
    /// Align 中保持不动、作为对齐基准的选中 object。
    pub align_reference: Option<BodyId>,
    /// Move 面板输入（world 轴位移）。
    pub move_input: [f32; 3],
    /// Move To 面板输入（目标绝对坐标）。
    pub move_to_input: [f32; 3],
    /// Move 面板方向键的单步位移（mm），默认 1 mm。
    pub move_step: f32,
    /// Rotate 面板输入（轴 0/1/2 + 角度）。
    pub rotate_input: (usize, f32),
    /// 从 sketch 直线或实体直棱拾取的世界空间旋转轴。
    pub rotate_custom_axis: Option<RotationAxis>,
    /// Rotate 子状态：下一次视口单击用于拾取轴线。
    pub rotate_axis_pick_active: bool,
    /// Scale 面板输入（百分比）。
    pub scale_input: [f32; 3],
    /// Scale by percent 默认联动 XYZ。
    pub scale_uniform: bool,
    /// Scale to size 面板输入（选中 object/组的目标世界包围盒尺寸，mm）。
    pub scale_size_input: [f32; 3],
    /// Scale to size 默认保持当前选中 object/组的长宽高比例。
    pub scale_size_uniform: bool,
    /// 剪贴板（Copy/Paste）。
    pub clipboard: Option<ClipboardItem>,
    /// 连续粘贴计数（偏移递增用）。
    pub paste_count: u32,
    /// Objects 漂浮窗开关（默认打开）。
    pub objects_panel_open: bool,
    /// Objects 漂浮窗位置（用户拖动后记忆，持久化到 ui_state.json）。
    pub objects_panel_pos: Option<egui::Pos2>,
    /// 可持久化的界面、引导与内置插件设置。
    pub settings: AppSettings,
    pub about_open: bool,
    pub help_search: String,
    pub first_use_help_open: bool,
    /// Standalone Agent Control manual window opened from Settings for Agents
    /// without a supported one-click registration command.
    pub agent_control_help_open: bool,
    pub plugins: PluginHost,
    /// Local loopback command listener used by the `s33d-agent` MCP/CLI bridge.
    pub agent_server: AgentServer,
    agent_desktop_lifetime: Arc<DesktopLifetime>,
    /// Editable Settings value. It is committed only after the new port binds,
    /// allowing a failed change to preserve the working listener.
    pub agent_control_port_input: u16,
    /// Last connection or Agent registration result, kept visible in Settings
    /// after the transient toast disappears.
    pub agent_control_status: Option<String>,
    /// Explicit confirmation dialog shown after a successful Agent
    /// registration so the result cannot be missed when the menu closes.
    pub agent_registration_notice: Option<String>,
    /// Objects 右键菜单正在编辑名称的 object 与暂存文本。
    pub rename_body_id: Option<BodyId>,
    pub rename_body_text: String,
    /// Persistent Objects context-menu color editor state (body + RGBA draft).
    pub object_color_editor: Option<(Vec<BodyId>, [f32; 4])>,
    /// Debug 开关（菜单栏开启后：click-log 文件 + History 诊断记录 + Export Debug）。
    pub debug_enabled: bool,
    /// 当前 sketch 会话创建于哪个 body 表面（None = 基准面）。
    pub sketch_on_body: Option<BodyId>,
    /// Chamfer/Fillet 工具激活。
    pub chamfer_active: bool,
    /// true = fillet（圆角），false = chamfer（倒角）。
    pub chamfer_fillet: bool,
    /// 倒角距离 / 圆角半径。
    pub chamfer_param: f32,
    /// 悬停的棱。
    pub hover_edge: Option<EdgeRef>,
    /// 选中的棱（普通选择预览；进入 Chamfer 后直接作为 Apply 目标）。
    pub selected_edge: Option<EdgeRef>,
    /// chamfer 目标体 → 基础 shape（重算用：改参数从基础重新差集）。
    pub chamfer_base_shapes: std::collections::HashMap<BodyId, ShapeId>,
    /// Done 后置位：下一次 Apply 产生新 object（而非更新当前）。
    pub chamfer_committed: bool,
    /// 上次单击（时间 + 位置），用于跨平台稳定识别双击/三击选择层级。
    pub last_click: Option<(std::time::Instant, egui::Pos2)>,
    /// 同一位置的连续左击次数：1=棱，2=面/区域，3=整个 object。
    pub click_count: u8,
    /// 连续点击序列开始前的选择集；二击/三击必须以它为基线，不能继承前一击的临时结果。
    click_selection_base: Option<ClickSelectionSnapshot>,
    /// Array 面板开关。
    pub array_active: bool,
    /// Array 启用的轴（多选：可同时沿多轴做网格阵列）。
    pub array_axes: [bool; 3],
    /// Array 每轴方向（true=正方向）。
    pub array_positive: [bool; 3],
    /// Array 每轴间距（各自独立）。
    pub array_spacing: [f32; 3],
    /// Array 每轴总数量（包含原件）。
    pub array_copies: [u32; 3],
    /// Sketch 中显示端点、中点、中心等红色关键点。
    pub sketch_points_visible: bool,
    /// Sketch 中吸附草图几何关键点（与底部网格 Snap 独立）。
    pub sketch_geometry_snap: bool,
    pub snap_enabled: bool,
    pub snap_step: f64,
    pub show_grid: bool,
    pub hint: String,
    pub toasts: Vec<(String, std::time::Instant)>,
    pub project_name: String,
    pub project_session: project_session::ProjectSession,
    pub references: crate::reference_assets::ReferenceLibrary,
    pub agent_clock: agent_visual::DocumentClock,
    pub sketch_tools_open: bool,
    pub drawing: crate::drawing2d::Drawing,
    pub draw3d_tools_open: bool,
    pub scene_version: u64,
    pub applied_version: u64,
    pub mesh_cache: HashMap<ShapeId, MeshData>,
    pick_geometry_cache: HashMap<ShapeId, Arc<PickGeometry>>,
    measurement_cache: HashMap<ShapeId, (glam::Vec3, glam::Vec3, f64, f64)>,
    last_hover_query: Option<(egui::Pos2, egui::Rect, u64, Mode, bool)>,
    /// Section View 剖切缓存：shape + 平面键 → 两半网格与法线。
    pub section_cache: HashMap<(ShapeId, [u64; 4]), CachedSectionSplit>,
    pub plane_pick_hover: Option<usize>,
    /// PickPlane 右栏中的自定义草图平面：ax + by + cz = d。
    pub sketch_plane_equation: [f64; 4],
    pub sketch_plane_equation_enabled: bool,
    pub saved_camera: Option<Camera>,
    pub viewport_rect: egui::Rect,
    pub cam_drag: Option<CamDrag>,
    /// 操作历史（最近 200 条，最新在前）。
    pub history: Vec<String>,
    /// Document 撤销/重做时与几何快照同步恢复的 object 选择。
    pub undo_selection_stack: Vec<Vec<BodyId>>,
    pub redo_selection_stack: Vec<Vec<BodyId>>,
    /// Sketch 编辑期间的独立实体时间线；退出 Sketch 后由 Document 快照接管。
    pub sketch_undo_stack: Vec<SketchSnapshot>,
    pub sketch_redo_stack: Vec<SketchSnapshot>,
    pub sketch_history_current: Option<SketchSnapshot>,
    /// 等待保存截图的 debug 时间戳。
    pub pending_debug_ts: Option<String>,
    /// Extrude 拖动状态（起始指针位置与深度）。
    pub extrude_drag_start: Option<(egui::Pos2, f64)>,
    pub numeric_focus_generation: u64,
    /// PickPlane 模式中 XZ 平面的半宽（随屏幕自适应）。
    pub plane_pick_size: f32,
    /// PickPlane 模式中 XY/YZ 小方块的边长（1/3 屏幕高度）。
    pub plane_block_size: f32,
    /// 当前要保存/打开的路径（文件对话框占位）。
    pub status_text: String,
    pub status_color: egui::Color32,
}

impl S33DApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_ui_fonts(&cc.egui_ctx);
        Self::from_ui_state(load_ui_state(), true)
    }

    /// Workers never read preferences, initialize fonts, or open an Agent listener.
    pub(crate) fn from_ui_state(ui_state: UiStateFile, desktop: bool) -> Self {
        let mut settings = ui_state.settings;
        settings.recent_projects = crate::recent_projects::normalize(settings.recent_projects);
        if settings.agent_control_port == 0 {
            settings.agent_control_port = DEFAULT_AGENT_PORT;
        }
        let agent_control_port_input = settings.agent_control_port;
        let default_step = settings.unit.millimeters_per_unit();
        let first_use_help_open = settings.show_first_use_tips;
        let plugins = if desktop {
            PluginHost::new(
                settings.calculator_plugin_enabled,
                settings.gear_plugin_enabled,
                settings.surface_texture_plugin_enabled,
                settings.tire_plugin_enabled,
                settings.wheel_rim_plugin_enabled,
                settings.screw_plugin_enabled,
                settings.nut_plugin_enabled,
            )
        } else {
            PluginHost::default()
        };
        let agent_desktop_lifetime = Arc::new(DesktopLifetime::default());
        let agent_server = if !desktop || cfg!(test) {
            AgentServer::disabled("disabled while running tests")
        } else if !settings.agent_control_enabled {
            AgentServer::disabled("disabled in Settings")
        } else {
            AgentServer::start_loopback(
                settings.agent_control_port,
                Arc::clone(&agent_desktop_lifetime),
            )
        };
        if desktop
            && !cfg!(test)
            && settings.agent_control_enabled
            && agent_server.address().is_none()
        {
            settings.agent_control_enabled = false;
        }
        let kernel = TruckKernel::new();
        let doc = Document::new("Untitled Project");
        let scene = SceneData {
            show_grid: true,
            ..Default::default()
        };
        Self {
            background_modeling: false,
            model_job: None,
            export_job: None,
            model_work: None,
            doc,
            offset_ctx: None,
            step_import_job: None,
            face_extrude_job: None,
            pending_tool_exit: None,
            tool_exit_prompt: None,
            confirmed_tool_exit_click: false,
            workspace_click_modifiers: None,
            replaying_tool_exit: false,
            array_exit_applied: None,
            exit_move_to_baseline: [0.; 3],
            exit_scale_size_baseline: [0.; 3],
            curve_ctx: None,
            surface_ctx: None,
            kernel,
            camera: Camera::default(),
            scene: Arc::new(Mutex::new(scene)),
            renderer: Arc::new(Mutex::new(None)),
            mode: Mode::Idle,
            sketch: None,
            sketch_edit_body: None,
            sketch_edit_original_doc: None,
            sketch_edit_original_selection: Vec::new(),
            selected: Vec::new(),
            object_list_anchor: None,
            thread_params: Default::default(),
            thread_ui: if desktop {
                crate::threads::config::UiState::load_user()
            } else {
                Default::default()
            },
            thread_choices: Vec::new(),
            thread_checked: Vec::new(),
            open_thread_panel: false,
            cylinder_pick_cache: HashMap::new(),
            selected_regions: Vec::new(),
            selected_region_source: None,
            hover_body: None,
            extrude_ctx: None,
            revolve_ctx: None,
            boolean_ctx: None,
            split_ctx: None,
            section_view_ctx: None,
            mirror_ctx: None,
            insert_3d_ctx: None,
            gizmo: None,
            gizmo_drag: None,
            face_pick: None,
            hover_face: None,
            regions_cache: Default::default(),
            region_mesh_cache: Default::default(),
            align_active: false,
            axis_align: axis_align::AxisAlign::default(),
            fastener_job: None,
            align_axis: 1,
            align_mode: AlignMode::Center,
            align_reference: None,
            move_input: [0.0; 3],
            move_to_input: [0.0; 3],
            move_step: default_step as f32,
            rotate_input: (1, 0.0),
            rotate_custom_axis: None,
            rotate_axis_pick_active: false,
            scale_input: [100.0; 3],
            scale_uniform: true,
            scale_size_input: [0.0; 3],
            scale_size_uniform: true,
            clipboard: None,
            paste_count: 0,
            objects_panel_open: true,
            objects_panel_pos: ui_state
                .objects_panel_pos_v2
                .map(|pos| egui::pos2(pos[0], pos[1])),
            settings,
            about_open: false,
            help_search: String::new(),
            first_use_help_open,
            agent_control_help_open: false,
            plugins,
            agent_server,
            agent_desktop_lifetime,
            agent_control_port_input,
            agent_control_status: None,
            agent_registration_notice: None,
            rename_body_id: None,
            rename_body_text: String::new(),
            object_color_editor: None,
            debug_enabled: false,
            last_click: None,
            click_count: 0,
            click_selection_base: None,
            sketch_on_body: None,
            chamfer_active: false,
            chamfer_fillet: false,
            chamfer_param: 2.0,
            hover_edge: None,
            chamfer_base_shapes: Default::default(),
            chamfer_committed: false,
            selected_edge: None,
            array_active: false,
            array_axes: [true, false, false],
            array_positive: [true; 3],
            array_spacing: [10.0; 3],
            array_copies: [3, 1, 1],
            sketch_points_visible: true,
            sketch_geometry_snap: true,
            snap_enabled: true,
            snap_step: default_step,
            show_grid: true,
            hint: "Ready. Press S to start a sketch, double-click a sketch region to select it."
                .into(),
            toasts: Vec::new(),
            project_name: "Untitled Project".into(),
            project_session: project_session::ProjectSession::default(),
            references: Default::default(),
            agent_clock: Default::default(),
            sketch_tools_open: false,
            drawing: crate::drawing2d::Drawing::default(),
            draw3d_tools_open: false,
            scene_version: 0,
            applied_version: u64::MAX,
            mesh_cache: HashMap::new(),
            pick_geometry_cache: HashMap::new(),
            measurement_cache: HashMap::new(),
            last_hover_query: None,
            section_cache: HashMap::new(),
            plane_pick_hover: None,
            sketch_plane_equation: [0.0, 0.0, 1.0, 0.0],
            sketch_plane_equation_enabled: false,
            saved_camera: None,
            viewport_rect: egui::Rect::NOTHING,
            cam_drag: None,
            history: Vec::new(),
            undo_selection_stack: Vec::new(),
            redo_selection_stack: Vec::new(),
            sketch_undo_stack: Vec::new(),
            sketch_redo_stack: Vec::new(),
            sketch_history_current: None,
            pending_debug_ts: None,
            extrude_drag_start: None,
            numeric_focus_generation: 0,
            plane_pick_size: 250.0,
            plane_block_size: 200.0,
            status_text: String::new(),
            status_color: egui::Color32::GRAY,
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push((msg.into(), std::time::Instant::now()));
    }

    pub fn set_hint(&mut self, text: impl Into<String>) {
        self.hint = text.into();
    }

    pub fn save_ui_preferences(&self) {
        save_ui_state(self.objects_panel_pos, &self.settings);
    }

    /// Visual preference only: retain the document, selections and camera.
    pub fn set_ui_appearance(&mut self, appearance: crate::appearance::UiAppearance) {
        if self.settings.appearance != appearance {
            self.settings.appearance = appearance;
            self.bump_scene();
        }
    }

    fn save_agent_control_preferences(&mut self) -> bool {
        match save_ui_state_checked(self.objects_panel_pos, &self.settings) {
            Ok(()) => true,
            Err(error) => {
                let message = match self.settings.language {
                    UiLanguage::English => format!(
                        "Agent Control changed for this run but Settings could not be saved: {error}. Grant S33D permission to write its user settings, then try again."
                    ),
                    UiLanguage::ChineseSimplified => format!(
                        "Agent 控制已在本次运行中改变，但设置无法保存：{error}。请授予 S33D 写入用户设置的权限后重试。"
                    ),
                };
                self.report_agent_control_status(message);
                false
            }
        }
    }

    /// Start or stop the local MCP/CLI listener immediately and persist the
    /// actual state. The bridge never owns model data, so replacing the
    /// listener does not affect the open document or its history.
    pub fn set_agent_control_enabled(&mut self, enabled: bool) -> bool {
        if !enabled {
            self.agent_server = AgentServer::disabled("disabled in Settings");
            self.settings.agent_control_enabled = false;
            if self.save_agent_control_preferences() {
                self.toast(
                    self.settings
                        .language
                        .text("Agent Control stopped", "Agent 控制已停止"),
                );
            }
            return false;
        }

        let port = self.settings.agent_control_port;
        let server = AgentServer::start_loopback(port, Arc::clone(&self.agent_desktop_lifetime));
        let address = server.address();
        if let Some(address) = address {
            self.agent_server = server;
            self.settings.agent_control_enabled = true;
            if !self.save_agent_control_preferences() {
                return false;
            }
            self.toast(match self.settings.language {
                UiLanguage::English => format!("Agent Control listening at {address}"),
                UiLanguage::ChineseSimplified => format!("Agent 控制正在监听 {address}"),
            });
            true
        } else {
            let error = server
                .startup_error()
                .unwrap_or("Agent Control could not start")
                .to_string();
            self.agent_server = server;
            self.settings.agent_control_enabled = false;
            self.save_agent_control_preferences();
            self.toast(match self.settings.language {
                UiLanguage::English => format!(
                    "Agent Control could not use port {port}: {error}. The port may be occupied; choose another port in Settings and try again."
                ),
                UiLanguage::ChineseSimplified => format!(
                    "Agent 控制无法使用端口 {port}：{error}。端口可能已被占用，请在设置中更换端口后重试。"
                ),
            });
            false
        }
    }

    /// Apply a new listener port transactionally. When Agent Control is
    /// running, the new port is bound before the old listener is replaced, so
    /// a conflict never breaks the working connection.
    pub fn apply_agent_control_port(&mut self, port: u16) -> bool {
        if port == 0 {
            self.toast(self.settings.language.text(
                "Port must be between 1 and 65535.",
                "端口必须在 1 到 65535 之间。",
            ));
            return false;
        }

        if !self.settings.agent_control_enabled {
            let previous_port = self.settings.agent_control_port;
            self.settings.agent_control_port = port;
            self.agent_control_port_input = port;
            if !self.save_agent_control_preferences() {
                self.settings.agent_control_port = previous_port;
                self.agent_control_port_input = previous_port;
                self.report_agent_control_status(match self.settings.language {
                    UiLanguage::English => format!(
                        "Port {port} was not applied because it could not be saved; the previous port {previous_port} remains selected."
                    ),
                    UiLanguage::ChineseSimplified => format!(
                        "端口 {port} 因无法保存而未应用；仍使用原端口 {previous_port}。"
                    ),
                });
                return false;
            }
            self.toast(match self.settings.language {
                UiLanguage::English => format!("Agent Control port saved as {port}"),
                UiLanguage::ChineseSimplified => format!("Agent 控制端口已保存为 {port}"),
            });
            return true;
        }

        if self.settings.agent_control_port == port && self.agent_server.address().is_some() {
            return self.save_agent_control_preferences();
        }

        let previous_port = self.settings.agent_control_port;
        let candidate = AgentServer::start_loopback(port, Arc::clone(&self.agent_desktop_lifetime));
        if let Some(address) = candidate.address() {
            self.agent_server = candidate;
            self.settings.agent_control_port = port;
            self.agent_control_port_input = port;
            if !self.save_agent_control_preferences() {
                let restored = AgentServer::start_loopback(
                    previous_port,
                    Arc::clone(&self.agent_desktop_lifetime),
                );
                if restored.address().is_some() {
                    self.agent_server = restored;
                    self.settings.agent_control_port = previous_port;
                    self.agent_control_port_input = previous_port;
                    self.report_agent_control_status(match self.settings.language {
                        UiLanguage::English => format!(
                            "Port {port} could not be saved, so Agent Control was restored to {previous_port}."
                        ),
                        UiLanguage::ChineseSimplified => format!(
                            "端口 {port} 无法保存，Agent 控制已恢复到原端口 {previous_port}。"
                        ),
                    });
                }
                return false;
            }
            self.toast(match self.settings.language {
                UiLanguage::English => format!("Agent Control switched to {address}"),
                UiLanguage::ChineseSimplified => format!("Agent 控制已切换到 {address}"),
            });
            true
        } else {
            let error = candidate
                .startup_error()
                .unwrap_or("the listener could not start");
            self.toast(match self.settings.language {
                UiLanguage::English => format!(
                    "Could not use port {port}: {error}. The port may be occupied; choose another port and try again. The previous listener is still active."
                ),
                UiLanguage::ChineseSimplified => format!(
                    "无法使用端口 {port}：{error}。端口可能已被占用，请更换端口后重试；原监听仍保持运行。"
                ),
            });
            false
        }
    }

    fn report_agent_control_status(&mut self, message: String) {
        self.agent_control_status = Some(message.clone());
        self.toast(message);
    }

    /// Apply the edited port, when needed, and verify that the S33D listener
    /// accepts a local TCP connection. A protocol request is intentionally not
    /// sent synchronously here because modeling requests are serviced by this
    /// same UI thread.
    pub fn test_agent_control_connection(&mut self) -> bool {
        if self.agent_control_port_input != self.settings.agent_control_port
            && !self.apply_agent_control_port(self.agent_control_port_input)
        {
            return false;
        }
        if !self.settings.agent_control_enabled {
            let message = self.settings.language.text(
                "Connection test failed: enable Agent Control, then try again.",
                "连接测试失败：请先启用 Agent 控制，然后重试。",
            );
            self.report_agent_control_status(message.to_string());
            return false;
        }
        let Some(address) = self.agent_server.address() else {
            let message = self.settings.language.text(
                "Connection test failed: the local listener is not running. Choose another port if it is occupied, then try again.",
                "连接测试失败：本机监听未运行。若端口被占用，请更换端口后重试。",
            );
            self.report_agent_control_status(message.to_string());
            return false;
        };
        match TcpStream::connect_timeout(&address, Duration::from_millis(600)) {
            Ok(_) => {
                let message = match self.settings.language {
                    UiLanguage::English => {
                        format!("Connection test passed: S33D is listening at {address}")
                    }
                    UiLanguage::ChineseSimplified => {
                        format!("连接测试通过：S33D 正在监听 {address}")
                    }
                };
                self.report_agent_control_status(message);
                true
            }
            Err(error) => {
                let message = match self.settings.language {
                    UiLanguage::English => format!(
                        "Connection test failed at {address}: {error}. Change the port and try again."
                    ),
                    UiLanguage::ChineseSimplified => format!(
                        "连接测试未通过（{address}）：{error}。请更换端口后重试。"
                    ),
                };
                self.report_agent_control_status(message);
                false
            }
        }
    }

    /// Register or update the user-level `s33d` MCP entry for a supported
    /// Agent. This operation never edits the S33D installation or document.
    pub fn register_agent_integration(&mut self, integration: AgentIntegration) -> bool {
        if !self.test_agent_control_connection() {
            return false;
        }
        let bridge = match agent_bridge_executable() {
            Ok(path) if path.is_file() => path,
            Ok(path) => {
                let message = match self.settings.language {
                    UiLanguage::English => format!(
                        "Cannot register {}: the matching bridge was not found at {}. Install or build s33d-agent beside S33D, then try again.",
                        integration.display_name(),
                        path.display()
                    ),
                    UiLanguage::ChineseSimplified => format!(
                        "无法注册 {}：在 {} 未找到配套桥接程序。请将 s33d-agent 安装或编译到 S33D 同目录后重试。",
                        integration.display_name(),
                        path.display()
                    ),
                };
                self.report_agent_control_status(message);
                return false;
            }
            Err(error) => {
                self.report_agent_control_status(error);
                return false;
            }
        };
        let address = format!("127.0.0.1:{}", self.settings.agent_control_port);

        let inspected = match run_agent_cli(integration, AgentConfigAction::Inspect, &bridge) {
            Ok(output) => output,
            Err(error) => {
                self.report_agent_cli_start_error(integration, error, true);
                return false;
            }
        };
        if output_reports_permission_problem(&inspected) {
            self.report_agent_permission_error(integration, &command_output_text(&inspected));
            return false;
        }
        if inspected.status.success() {
            let removed = match run_agent_cli(integration, AgentConfigAction::Unregister, &bridge) {
                Ok(output) => output,
                Err(error) => {
                    self.report_agent_cli_start_error(integration, error, true);
                    return false;
                }
            };
            if !removed.status.success() {
                if output_reports_permission_problem(&removed) {
                    self.report_agent_permission_error(integration, &command_output_text(&removed));
                } else {
                    self.report_agent_command_failure(
                        integration,
                        "update the existing registration",
                        "更新现有注册",
                        &removed,
                    );
                }
                return false;
            }
        } else if !output_reports_missing_registration(&inspected) {
            self.report_agent_command_failure(
                integration,
                "inspect the existing registration",
                "检查现有注册",
                &inspected,
            );
            return false;
        }

        let registered = match run_agent_cli(integration, AgentConfigAction::Register, &bridge) {
            Ok(output) => output,
            Err(error) => {
                self.report_agent_cli_start_error(integration, error, true);
                return false;
            }
        };
        if registered.status.success() {
            let message = match self.settings.language {
                UiLanguage::English => format!(
                    "{} registration complete at {address}. Restart or reopen the Agent to load S33D.",
                    integration.display_name()
                ),
                UiLanguage::ChineseSimplified => format!(
                    "{} 注册完成（{address}）。请重启或重新打开 Agent 以加载 S33D。",
                    integration.display_name()
                ),
            };
            self.agent_registration_notice = Some(message.clone());
            self.report_agent_control_status(message);
            true
        } else {
            if output_reports_permission_problem(&registered) {
                self.report_agent_permission_error(integration, &command_output_text(&registered));
            } else {
                self.report_agent_command_failure(
                    integration,
                    "write the registration",
                    "写入注册",
                    &registered,
                );
            }
            false
        }
    }

    /// Remove only the `s33d` MCP registration from the selected Agent's
    /// user configuration. The S33D application and project are untouched.
    pub fn unregister_agent_integration(&mut self, integration: AgentIntegration) -> bool {
        let bridge = agent_bridge_executable().unwrap_or_default();
        let removed = match run_agent_cli(integration, AgentConfigAction::Unregister, &bridge) {
            Ok(output) => output,
            Err(error) => {
                self.report_agent_cli_start_error(integration, error, false);
                return false;
            }
        };
        if removed.status.success() || output_reports_missing_registration(&removed) {
            let message = match self.settings.language {
                UiLanguage::English => format!(
                    "{}: the S33D registration is removed. S33D itself was not deleted.",
                    integration.display_name()
                ),
                UiLanguage::ChineseSimplified => format!(
                    "{}：已取消 S33D 注册；S33D 程序本身未被删除。",
                    integration.display_name()
                ),
            };
            self.report_agent_control_status(message);
            true
        } else {
            if output_reports_permission_problem(&removed) {
                self.report_agent_permission_error(integration, &command_output_text(&removed));
            } else {
                self.report_agent_command_failure(
                    integration,
                    "remove the registration",
                    "取消注册",
                    &removed,
                );
            }
            false
        }
    }

    fn report_agent_cli_start_error(
        &mut self,
        integration: AgentIntegration,
        error: std::io::Error,
        registering: bool,
    ) {
        let action_en = if registering {
            "register"
        } else {
            "unregister"
        };
        let action_zh = if registering {
            "注册"
        } else {
            "取消注册"
        };
        let message = if error.kind() == std::io::ErrorKind::PermissionDenied {
            match self.settings.language {
                UiLanguage::English => format!(
                    "S33D needs permission to {action_en} with {}. Allow access to its Agent configuration, then try again.",
                    integration.display_name()
                ),
                UiLanguage::ChineseSimplified => format!(
                    "S33D 需要权限才能在 {} 中{action_zh}。请允许访问该 Agent 的配置后重试。",
                    integration.display_name()
                ),
            }
        } else if error.kind() == std::io::ErrorKind::NotFound {
            match self.settings.language {
                UiLanguage::English => format!(
                    "{} CLI was not found. Install or open {}, then try again; Other Agent Manual shows the manual setup.",
                    integration.display_name(),
                    integration.display_name()
                ),
                UiLanguage::ChineseSimplified => format!(
                    "未找到 {} 命令行工具。请先安装或打开 {} 后重试；“其他 Agent 手册”提供手动配置方法。",
                    integration.display_name(),
                    integration.display_name()
                ),
            }
        } else {
            match self.settings.language {
                UiLanguage::English => format!(
                    "Could not start the {} configuration command: {error}",
                    integration.display_name()
                ),
                UiLanguage::ChineseSimplified => {
                    format!("无法启动 {} 配置命令：{error}", integration.display_name())
                }
            }
        };
        self.report_agent_control_status(message);
    }

    fn report_agent_permission_error(&mut self, integration: AgentIntegration, detail: &str) {
        let message = match self.settings.language {
            UiLanguage::English => format!(
                "S33D needs permission to update the {} Agent configuration. Grant configuration-file access and try again. {detail}",
                integration.display_name()
            ),
            UiLanguage::ChineseSimplified => format!(
                "S33D 需要权限才能更新 {} 的 Agent 配置。请授予配置文件访问权限后重试。{detail}",
                integration.display_name()
            ),
        };
        self.report_agent_control_status(message);
    }

    fn report_agent_command_failure(
        &mut self,
        integration: AgentIntegration,
        action_en: &str,
        action_zh: &str,
        output: &Output,
    ) {
        let detail = command_output_text(output);
        let message = match self.settings.language {
            UiLanguage::English => format!(
                "Could not {action_en} for {}: {detail}",
                integration.display_name()
            ),
            UiLanguage::ChineseSimplified => format!(
                "无法为 {} {action_zh}：{detail}",
                integration.display_name()
            ),
        };
        self.report_agent_control_status(message);
    }

    /// Change only the user-facing unit. Geometry remains in millimeters;
    /// Step, Move Step and the grid adopt one whole unit as their new default.
    pub fn set_ui_unit(&mut self, unit: UiUnit) -> bool {
        if self.settings.unit == unit {
            return false;
        }
        self.settings.unit = unit;
        self.snap_step = unit.millimeters_per_unit();
        self.move_step = self.snap_step as f32;
        self.save_ui_preferences();
        self.bump_scene();
        true
    }

    pub fn selected_gear_for_plugin(&self) -> Option<(BodyId, GearParams)> {
        if self.selected.len() != 1 {
            return None;
        }
        let body_id = self.selected.first().copied()?;
        let body = self.doc.body(body_id)?;
        match body.feature {
            Feature::Gear { params } => Some((body_id, params)),
            _ => None,
        }
    }

    pub fn selected_tire_for_plugin(&self) -> Option<(BodyId, TireParams)> {
        let body_id = (self.selected.len() == 1).then(|| self.selected[0])?;
        match self.doc.body(body_id)?.feature {
            Feature::Tire { params } => Some((body_id, params)),
            _ => None,
        }
    }

    pub fn selected_wheel_rim_for_plugin(&self) -> Option<(BodyId, WheelRimParams)> {
        let body_id = (self.selected.len() == 1).then(|| self.selected[0])?;
        match self.doc.body(body_id)?.feature {
            Feature::WheelRim { params } => Some((body_id, params)),
            _ => None,
        }
    }

    pub fn selected_surface_texture_for_plugin(
        &self,
    ) -> Option<(BodyId, BodyId, SurfaceTextureSurface, SurfaceTextureParams)> {
        let body_id = (self.selected.len() == 1).then(|| self.selected[0])?;
        match self.doc.body(body_id)?.feature {
            Feature::SurfaceTexture {
                source,
                surface,
                params,
            } => Some((body_id, source, surface, params)),
            _ => None,
        }
    }

    /// Convert the currently selected face into stable construction geometry
    /// for the Surface Texture plug-in. Parametric cylinders are promoted from
    /// the clicked side facet to their complete continuous lateral surface.
    pub fn surface_texture_target_for_plugin(&mut self) -> Option<(BodyId, SurfaceTextureSurface)> {
        let face = self.face_pick.clone()?;
        let body = self.doc.body(face.body_id)?.clone();
        let primitive = draw_3d_params_from_feature(&body.feature);
        if let Some(params) = primitive.filter(|params| params.kind == Primitive3DKind::Cylinder) {
            let frame = params.frame.unwrap_or_else(|| DatumPlane::Top.def());
            let axis = frame.normal().normalize();
            let face_normal = glam::DVec3::from_array(face.normal.map(f64::from)).normalize();
            if axis.dot(face_normal).abs() < 0.72 {
                let base = glam::DVec3::from_array(params.position);
                return Some((
                    face.body_id,
                    SurfaceTextureSurface::Cylinder {
                        center: (base + axis * (params.height * 0.5)).to_array(),
                        axis: axis.to_array(),
                        x_axis: frame.x_axis().to_array(),
                        radius: params.radius,
                        length: params.height,
                    },
                ));
            }
        }

        let shape = body.shape?;
        let mesh = self.mesh_for(shape);
        let normal = glam::DVec3::from_array(face.normal.map(f64::from)).normalize();
        let frame = PlaneDef::from_normal(normal.to_array());
        let x_axis = frame.x_axis();
        let y_axis = frame.y_axis();
        let reference = glam::DVec3::from_array(face.center.map(f64::from));
        let mut min = glam::DVec2::splat(f64::INFINITY);
        let mut max = glam::DVec2::splat(f64::NEG_INFINITY);
        for vertex_index in face.triangles {
            let point = mesh.positions.get(vertex_index as usize)?;
            let relative = glam::DVec3::from_array(point.map(f64::from)) - reference;
            let local = glam::DVec2::new(relative.dot(x_axis), relative.dot(y_axis));
            min = min.min(local);
            max = max.max(local);
        }
        let size = max - min;
        if !size.is_finite() || size.x <= 1.0e-5 || size.y <= 1.0e-5 {
            return None;
        }
        let origin =
            reference + x_axis * ((min.x + max.x) * 0.5) + y_axis * ((min.y + max.y) * 0.5);
        Some((
            face.body_id,
            SurfaceTextureSurface::Planar {
                origin: origin.to_array(),
                normal: normal.to_array(),
                x_axis: x_axis.to_array(),
                width: size.x,
                height: size.y,
            },
        ))
    }

    pub fn apply_plugin_command(&mut self, command: PluginCommand) {
        match command {
            PluginCommand::GenerateFastener {
                params,
                replace_body,
            } => self.generate_fastener(params, replace_body),
            PluginCommand::GenerateGear {
                params,
                replace_body,
            } => self.generate_gear(params, replace_body),
            PluginCommand::GenerateTire {
                params,
                replace_body,
            } => self.generate_tire(params, replace_body),
            PluginCommand::GenerateWheelRim {
                params,
                replace_body,
            } => self.generate_wheel_rim(params, replace_body),
            PluginCommand::GenerateSurfaceTexture {
                source_body,
                surface,
                params,
                replace_body,
            } => self.generate_surface_texture(source_body, surface, params, replace_body),
        }
    }

    fn generate_gear(&mut self, params: GearParams, replace_body: Option<BodyId>) {
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current modeling operation before generating a gear");
            return;
        }
        let shape = match crate::gear::build_gear_shape(&mut self.kernel, &params) {
            Ok(shape) => shape,
            Err(error) => {
                self.toast(format!("Gear generation failed: {error}"));
                return;
            }
        };

        let had_solids = self
            .doc
            .bodies
            .iter()
            .any(|body| body.kind == BodyKind::Solid && body.visible);
        if let Some(body_id) = replace_body {
            let editable = self
                .doc
                .body(body_id)
                .is_some_and(|body| matches!(body.feature, Feature::Gear { .. }));
            if !editable {
                self.kernel.delete(shape);
                self.toast("The selected gear is no longer available");
                return;
            }
            self.begin_document_change();
            if let Some(body) = self.doc.body_mut(body_id) {
                body.shape = Some(shape);
                body.feature = Feature::Gear { params };
                body.kind = BodyKind::Solid;
                body.visible = true;
            }
            self.selected = vec![body_id];
            self.record_history(format!(
                "Update {} {} (module {}, {} teeth)",
                params.kind.name(),
                body_id,
                params.module,
                params.teeth
            ));
            self.toast(format!("{} updated", params.kind.name()));
        } else {
            let body_id = self.doc.alloc_id();
            self.begin_document_change();
            self.doc.add_body(Body {
                id: body_id,
                name: format!("{} {}", params.kind.name(), body_id),
                kind: BodyKind::Solid,
                feature: Feature::Gear { params },
                shape: Some(shape),
                material: Material::default(),
                visible: true,
            });
            self.selected = vec![body_id];
            self.record_history(format!(
                "Generate {} {} (module {}, {} teeth)",
                params.kind.name(),
                body_id,
                params.module,
                params.teeth
            ));
            self.toast(format!("{} generated", params.kind.name()));
        }
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.bump_scene();
        if !had_solids {
            self.fit_view();
        }
    }

    fn generate_tire(&mut self, params: TireParams, replace_body: Option<BodyId>) {
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current modeling operation before generating a tire");
            return;
        }
        let shape = match crate::tire::build_tire_shape(&mut self.kernel, &params) {
            Ok(shape) => shape,
            Err(error) => {
                self.toast(format!("Tire generation failed: {error}"));
                return;
            }
        };
        let had_solids = self
            .doc
            .bodies
            .iter()
            .any(|body| body.kind == BodyKind::Solid && body.visible);
        if let Some(body_id) = replace_body {
            if !self
                .doc
                .body(body_id)
                .is_some_and(|body| matches!(body.feature, Feature::Tire { .. }))
            {
                self.kernel.delete(shape);
                self.toast("The selected tire is no longer available");
                return;
            }
            self.begin_document_change();
            if let Some(body) = self.doc.body_mut(body_id) {
                body.shape = Some(shape);
                body.feature = Feature::Tire { params };
                body.visible = true;
            }
            self.selected = vec![body_id];
            self.record_history(format!("Update Tire {body_id} ({})", params.pattern.name()));
            self.toast("Tire updated");
        } else {
            let body_id = self.doc.alloc_id();
            self.begin_document_change();
            self.doc.add_body(Body {
                id: body_id,
                name: format!("Tire {body_id}"),
                kind: BodyKind::Solid,
                feature: Feature::Tire { params },
                shape: Some(shape),
                material: Material {
                    color: [0.055, 0.062, 0.072, 1.0],
                    metalness: 0.0,
                    roughness: 0.82,
                },
                visible: true,
            });
            self.selected = vec![body_id];
            self.record_history(format!(
                "Generate Tire {body_id} ({})",
                params.pattern.name()
            ));
            self.toast("Tire generated");
        }
        self.finish_plugin_geometry_change(had_solids);
    }

    fn generate_wheel_rim(&mut self, params: WheelRimParams, replace_body: Option<BodyId>) {
        if self.mode != Mode::Idle {
            self.toast(
                "Finish or cancel the current modeling operation before generating a wheel rim",
            );
            return;
        }
        let shape = match crate::wheel_rim::build_wheel_rim_shape(&mut self.kernel, &params) {
            Ok(shape) => shape,
            Err(error) => {
                self.toast(format!("Wheel rim generation failed: {error}"));
                return;
            }
        };
        let had_solids = self
            .doc
            .bodies
            .iter()
            .any(|body| body.kind == BodyKind::Solid && body.visible);
        if let Some(body_id) = replace_body {
            if !self
                .doc
                .body(body_id)
                .is_some_and(|body| matches!(body.feature, Feature::WheelRim { .. }))
            {
                self.kernel.delete(shape);
                self.toast("The selected wheel rim is no longer available");
                return;
            }
            self.begin_document_change();
            if let Some(body) = self.doc.body_mut(body_id) {
                body.shape = Some(shape);
                body.feature = Feature::WheelRim { params };
                body.visible = true;
            }
            self.selected = vec![body_id];
            self.record_history(format!(
                "Update Wheel Rim {body_id} ({})",
                params.style.name()
            ));
            self.toast("Wheel rim updated");
        } else {
            let body_id = self.doc.alloc_id();
            self.begin_document_change();
            self.doc.add_body(Body {
                id: body_id,
                name: format!("Wheel Rim {body_id}"),
                kind: BodyKind::Solid,
                feature: Feature::WheelRim { params },
                shape: Some(shape),
                material: Material {
                    color: [0.68, 0.72, 0.78, 1.0],
                    metalness: 0.78,
                    roughness: 0.28,
                },
                visible: true,
            });
            self.selected = vec![body_id];
            self.record_history(format!(
                "Generate Wheel Rim {body_id} ({})",
                params.style.name()
            ));
            self.toast("Wheel rim generated");
        }
        self.finish_plugin_geometry_change(had_solids);
    }

    fn generate_surface_texture(
        &mut self,
        source_body: BodyId,
        surface: SurfaceTextureSurface,
        params: SurfaceTextureParams,
        replace_body: Option<BodyId>,
    ) {
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current modeling operation before applying texture");
            return;
        }
        let Some(source_shape) = self.doc.body(source_body).and_then(|body| body.shape) else {
            self.toast("The texture source object is no longer available");
            return;
        };
        let shape = match crate::surface_texture::build_surface_texture_shape(
            &mut self.kernel,
            source_shape,
            &surface,
            &params,
        ) {
            Ok(shape) => shape,
            Err(error) => {
                self.toast(format!("Surface texture failed: {error}"));
                return;
            }
        };
        let material = self
            .doc
            .body(source_body)
            .map(|body| body.material.clone())
            .unwrap_or_default();
        if let Some(body_id) = replace_body {
            let editable = self.doc.body(body_id).is_some_and(|body| {
                matches!(body.feature, Feature::SurfaceTexture { source, .. } if source == source_body)
            });
            if !editable {
                self.kernel.delete(shape);
                self.toast("The selected surface texture is no longer available");
                return;
            }
            self.begin_document_change();
            if let Some(body) = self.doc.body_mut(body_id) {
                body.shape = Some(shape);
                body.feature = Feature::SurfaceTexture {
                    source: source_body,
                    surface,
                    params,
                };
                body.visible = true;
            }
            if let Some(source) = self.doc.body_mut(source_body) {
                source.visible = false;
            }
            self.selected = vec![body_id];
            self.record_history(format!(
                "Update Surface Texture {body_id} ({})",
                params.pattern.name()
            ));
            self.toast("Surface texture updated");
        } else {
            let body_id = self.doc.alloc_id();
            self.begin_document_change();
            if let Some(source) = self.doc.body_mut(source_body) {
                source.visible = false;
            }
            self.doc.add_body(Body {
                id: body_id,
                name: format!("Surface Texture {body_id}"),
                kind: BodyKind::Solid,
                feature: Feature::SurfaceTexture {
                    source: source_body,
                    surface,
                    params,
                },
                shape: Some(shape),
                material,
                visible: true,
            });
            self.selected = vec![body_id];
            self.record_history(format!(
                "Generate Surface Texture {body_id} ({})",
                params.pattern.name()
            ));
            self.toast("Surface texture generated");
        }
        self.finish_plugin_geometry_change(true);
    }

    fn finish_plugin_geometry_change(&mut self, had_solids: bool) {
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.bump_scene();
        if !had_solids {
            self.fit_view();
        }
    }

    /// Every committed document mutation goes through this boundary so Edit
    /// Undo/Redo restores object selection together with the model snapshot.
    pub fn begin_document_change(&mut self) {
        self.undo_selection_stack.push(self.selected.clone());
        if self.undo_selection_stack.len() > Document::MAX_UNDO_STEPS {
            let excess = self.undo_selection_stack.len() - Document::MAX_UNDO_STEPS;
            self.undo_selection_stack.drain(0..excess);
        }
        self.redo_selection_stack.clear();
        self.doc.begin_change();
    }

    /// Reference-tracing kernel GC. Document current/undo/redo states are
    /// roots; active previews are temporary roots. Everything else is an
    /// unreachable immutable shape and can be reclaimed safely.
    pub fn collect_kernel_garbage(&mut self) -> usize {
        let mut live: HashSet<ShapeId> = self.doc.referenced_shape_ids();
        if let Some(context) = &self.extrude_ctx {
            live.extend(context.ghost_shapes.iter().copied());
        }
        if let Some(shape) = self
            .insert_3d_ctx
            .as_ref()
            .and_then(|context| context.preview_shape)
        {
            live.insert(shape);
        }
        if let Some(context) = &self.mirror_ctx {
            live.extend(context.previews.iter().map(|(_, shape)| *shape));
        }
        if let Some(context) = &self.split_ctx {
            live.extend(context.previews.iter().copied());
        }
        if let Some(drag) = &self.gizmo_drag {
            for item in &drag.items {
                live.insert(item.start_shape);
                if let Some(shape) = item.temp_shape {
                    live.insert(shape);
                }
            }
        }
        // Edit Sketch previews temporarily replace dependent shapes in the
        // current document. Keep the pre-edit shapes alive so Esc can restore
        // them and Done can record them as the single undo snapshot.
        if let Some(original) = &self.sketch_edit_original_doc {
            live.extend(original.referenced_shape_ids());
        }
        live.extend(self.chamfer_base_shapes.values().copied());
        // Clipboard geometry is an immutable snapshot, even when its source
        // object is subsequently edited or removed from the document.
        live.extend(self.clipboard.as_ref().and_then(|item| item.body.shape));

        self.mesh_cache.retain(|shape, _| live.contains(shape));
        self.pick_geometry_cache
            .retain(|shape, _| live.contains(shape));
        self.measurement_cache
            .retain(|shape, _| live.contains(shape));
        self.cylinder_pick_cache
            .retain(|shape, _| live.contains(shape));
        self.section_cache
            .retain(|(shape, _), _| live.contains(shape));
        self.kernel.retain_shapes(&live)
    }

    pub fn reset_edit_history_context(&mut self) {
        self.undo_selection_stack.clear();
        self.redo_selection_stack.clear();
        self.sketch_undo_stack.clear();
        self.sketch_redo_stack.clear();
        self.sketch_history_current = None;
    }

    /// 触发 debug 导出：立即写状态文本到 debug/ 目录（截图由 UI 层配合保存）。
    pub fn export_debug(&mut self) {
        std::fs::create_dir_all("debug").ok();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let text = self.debug_state_text();
        let path = format!("debug/debug-{ts}.txt");
        if std::fs::write(&path, &text).is_ok() {
            self.pending_debug_ts = Some(ts.to_string());
            self.toast(format!("Debug state saved to {path} - screenshot follows"));
        } else {
            self.toast("Debug export failed");
        }
    }

    /// 当前完整状态文本（诊断用）。
    pub fn debug_state_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "S33D debug state @ {}\n\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ));
        out.push_str(&format!("mode: {:?}\ngizmo: {:?}\n", self.mode, self.gizmo));
        out.push_str(&format!(
            "selected bodies: {:?}\nselected regions: {:?}\nface_pick: {}\n",
            self.selected,
            self.selected_regions,
            self.face_pick
                .as_ref()
                .map(|f| format!(
                    "body={} n=({:.2},{:.2},{:.2}) tris={}",
                    f.body_id,
                    f.normal[0],
                    f.normal[1],
                    f.normal[2],
                    f.triangles.len() / 3
                ))
                .unwrap_or_else(|| "none".into())
        ));
        out.push_str(&format!(
            "camera: eye=({:.1},{:.1},{:.1}) target=({:.1},{:.1},{:.1}) ortho={} ortho_half={:.1}\n",
            self.camera.eye.x, self.camera.eye.y, self.camera.eye.z,
            self.camera.target.x, self.camera.target.y, self.camera.target.z,
            self.camera.ortho, self.camera.ortho_half
        ));
        out.push_str("\n== Bodies ==\n");
        for b in &self.doc.bodies {
            let bbox = b
                .shape
                .and_then(|s| self.kernel.tessellate(s).ok())
                .and_then(|m| m.bounds())
                .map(|(mn, mx)| {
                    format!(
                        "({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1}) size=({:.1},{:.1},{:.1})",
                        mn.x,
                        mn.y,
                        mn.z,
                        mx.x,
                        mx.y,
                        mx.z,
                        mx.x - mn.x,
                        mx.y - mn.y,
                        mx.z - mn.z
                    )
                })
                .unwrap_or_else(|| "no-shape".into());
            out.push_str(&format!(
                "body {} \"{}\" kind={:?} shape={:?} visible={} bbox={}\n  feature: {}\n",
                b.id,
                b.name,
                b.kind,
                b.shape,
                b.visible,
                bbox,
                b.feature.describe()
            ));
            if let Some(sk) = b.sketch() {
                out.push_str(&format!(
                    "  sketch plane: o=({:.1},{:.1},{:.1}) n=({:.2},{:.2},{:.2}) entities={}\n",
                    sk.plane.origin[0],
                    sk.plane.origin[1],
                    sk.plane.origin[2],
                    sk.plane.normal().x,
                    sk.plane.normal().y,
                    sk.plane.normal().z,
                    sk.entities.len()
                ));
                let regions = s33d_model::compute_regions(&sk.entities);
                out.push_str(&format!(
                    "  sketch regions: {} (areas: {})\n",
                    regions.len(),
                    regions
                        .iter()
                        .map(|r| format!("{:.2}", r.signed_area()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                for (i, e) in sk.entities.iter().enumerate() {
                    out.push_str(&format!("    entity {i}: {:?}\n", e));
                }
            }
        }
        out.push_str("\n== History ==\n");
        for h in &self.history {
            out.push_str(&format!("  {h}\n"));
        }
        out
    }
    /// 记录一条操作历史（上限 200 条，最新的在末尾）。
    pub fn record_history(&mut self, msg: impl Into<String>) {
        self.history.push(msg.into());
        if self.history.len() > 200 {
            let extra = self.history.len() - 200;
            self.history.drain(0..extra);
        }
    }

    pub fn project_to_s33d(&self) -> Result<String, String> {
        if self.drawing.is_generating() {
            return Err("请等待工程图投影生成完成后再保存。".into());
        }
        let encoded = encode_s33d_project(
            &self.doc,
            &self.kernel,
            &self.history,
            &self.selected,
            &self.camera,
            self.show_grid,
            self.snap_enabled,
            self.snap_step,
        )?;
        let mut value: Value = serde_json::from_str(&encoded).map_err(|e| e.to_string())?;
        value["drawing"] = serde_json::to_value(&self.drawing).map_err(|e| e.to_string())?;
        value["references"] = serde_json::to_value(&self.references).map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }

    pub fn new_project(&mut self) {
        self.reset_document_tools();
        self.references = Default::default();
        self.agent_clock = Default::default();
        self.doc = Document::new("Untitled Project");
        self.kernel = TruckKernel::new();
        self.selected.clear();
        self.history.clear();
        self.project_name = "Untitled Project".into();
        self.mark_project_saved(None);
        self.clipboard = None;
        self.paste_count = 0;
        self.plugins.sync_selected_gear(None);
        self.plugins.sync_selected_tire(None);
        self.plugins.sync_selected_wheel_rim(None);
        self.plugins.sync_selected_fastener(None);
        self.plugins.sync_selected_surface_texture(None);
        self.plugins.sync_surface_texture_target(None);
        self.set_hint("New project");
        self.bump_scene();
    }

    /// Drop every document-owned draft and cached pick before IDs are reused.
    /// Do not commit or cancel against the replacement document/kernel.
    fn reset_document_tools(&mut self) {
        self.region_mesh_cache = Default::default();
        self.cancel_model_job();
        self.cancel_export_job();
        self.drawing = crate::drawing2d::Drawing::default();
        self.thread_choices.clear();
        self.thread_checked.clear();
        self.open_thread_panel = false;
        self.thread_ui.target_key = None;
        self.thread_ui.expanded = false;
        self.thread_ui.selected_preset = None;
        self.thread_ui.baseline = None;
        self.thread_ui.save_open = false;
        self.thread_ui.error = None;
        self.cylinder_pick_cache.clear();
        self.object_list_anchor = None;
        self.step_import_job = None;
        self.face_extrude_job = None;
        self.pending_tool_exit = None;
        self.tool_exit_prompt = None;
        self.confirmed_tool_exit_click = false;
        self.workspace_click_modifiers = None;
        self.replaying_tool_exit = false;
        self.array_exit_applied = None;
        self.fastener_job = None;
        self.axis_align = axis_align::AxisAlign::default();
        self.reset_edit_history_context();
        self.mode = Mode::Idle;
        self.sketch = None;
        self.sketch_edit_body = None;
        self.sketch_edit_original_doc = None;
        self.sketch_edit_original_selection.clear();
        self.sketch_on_body = None;
        self.saved_camera = None;
        self.extrude_ctx = None;
        self.extrude_drag_start = None;
        self.revolve_ctx = None;
        self.boolean_ctx = None;
        self.split_ctx = None;
        self.section_view_ctx = None;
        self.mirror_ctx = None;
        self.insert_3d_ctx = None;
        self.offset_ctx = None;
        self.curve_ctx = None;
        self.surface_ctx = None;
        self.gizmo = None;
        self.gizmo_drag = None;
        self.cam_drag = None;
        self.align_active = false;
        self.align_reference = None;
        self.array_active = false;
        self.rotate_axis_pick_active = false;
        self.rotate_custom_axis = None;
        self.chamfer_active = false;
        self.chamfer_base_shapes.clear();
        self.chamfer_committed = false;
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.hover_body = None;
        self.last_click = None;
        self.click_count = 0;
        self.click_selection_base = None;
        self.regions_cache.clear();
        self.mesh_cache.clear();
        self.pick_geometry_cache.clear();
        self.measurement_cache.clear();
        self.last_hover_query = None;
        self.section_cache.clear();
        self.rename_body_id = None;
        self.rename_body_text.clear();
        self.object_color_editor = None;
        self.plane_pick_hover = None;
        self.toasts.clear();
    }

    pub fn load_s33d_project(&mut self, text: &str) -> Result<(), String> {
        let (doc, kernel, history, selected, camera, show_grid, snap_enabled, snap_step) =
            decode_s33d_project(text)?;
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let references: crate::reference_assets::ReferenceLibrary = match value.get("references") {
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| e.to_string())?,
            None => Default::default(),
        };
        references.validate()?;
        let drawing = match value.get("drawing").filter(|v| !v.is_null()) {
            Some(v) => {
                serde_json::from_value(v.clone()).map_err(|e| format!("invalid drawing: {e}"))?
            }
            None => crate::drawing2d::Drawing::default(),
        };
        self.reset_document_tools();
        self.drawing = drawing;
        self.references = references;
        self.agent_clock = Default::default();
        self.doc = doc;
        self.kernel = kernel;
        self.history = history;
        self.selected = selected;
        self.reset_edit_history_context();
        self.camera = camera;
        self.show_grid = show_grid;
        self.snap_enabled = snap_enabled;
        self.snap_step = snap_step;
        self.mode = Mode::Idle;
        self.sketch = None;
        self.sketch_edit_body = None;
        self.sketch_edit_original_doc = None;
        self.sketch_edit_original_selection.clear();
        self.extrude_ctx = None;
        self.revolve_ctx = None;
        self.boolean_ctx = None;
        self.split_ctx = None;
        self.section_view_ctx = None;
        self.mirror_ctx = None;
        self.insert_3d_ctx = None;
        self.offset_ctx = None;
        self.curve_ctx = None;
        self.surface_ctx = None;
        self.gizmo = None;
        self.align_active = false;
        self.array_active = false;
        self.chamfer_active = false;
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.hover_body = None;
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.set_hint("Project opened");
        self.mark_project_saved(None);
        self.bump_scene();
        Ok(())
    }

    pub fn bump_scene(&mut self) {
        self.observe_agent_document();
        self.sanitize_region_selection();
        self.region_mesh_cache
            .retain(|id| self.doc.body(id).is_some());
        self.scene_version += 1;
    }

    // ================= Agent Control =================

    /// Drain a bounded number of loopback requests on the UI thread.  Kernel,
    /// Document, undo/redo, selection and scene invalidation therefore keep
    /// exactly the same ownership rules as interactive modeling.
    pub fn process_agent_requests(&mut self) {
        for _ in 0..32 {
            let Some(pending) = self.agent_server.try_recv() else {
                break;
            };
            if !pending.try_begin() {
                continue;
            }
            let mut queued_request = pending.request.clone();
            self.observe_agent_document();
            if self.tool_exit_prompt.is_none() {
                if let Err((code, message)) = self.check_agent_expected(&mut queued_request.params)
                {
                    let id = pending.request.id.clone();
                    pending.respond(AgentResponse::failure(id, code, message));
                    continue;
                }
            }
            if self.queue_agent_export_job(&queued_request) {
                self.export_job.as_mut().unwrap().pending_agent = Some(pending);
                continue;
            }
            if self.queue_agent_model_job(&queued_request) {
                self.model_job.as_mut().unwrap().pending_agent = Some(pending);
                continue;
            }
            let request_id = pending.request.id.clone();
            let enabled = std::mem::replace(&mut self.background_modeling, false);
            let result = self.execute_agent_request(pending.request.clone());
            self.background_modeling = enabled;
            let response = match result {
                Ok(result) => AgentResponse::success(request_id, result),
                Err((code, message)) => AgentResponse::failure(request_id, code, message),
            };
            pending.respond(response);
        }
    }

    fn execute_agent_request(&mut self, request: AgentRequest) -> AgentCommandResult {
        if !matches!(
            request.method.as_str(),
            "system.ping" | "system.describe" | "workflow.get"
        ) {
            if self.tool_exit_prompt.is_some() {
                return Err(Self::agent_fault(
                    "busy",
                    "Resolve the tool exit confirmation first",
                ));
            }
            if (self.progress_busy() && request.method != "document.summary")
                || (request.method == "body.export" && self.surface_preview_pending())
            {
                return Err(Self::agent_fault(
                    "busy",
                    "Wait for the current operation or cancel it first",
                ));
            }
        }
        self.observe_agent_document();
        let mut params = request.params;
        self.check_agent_expected(&mut params)?;
        if let Some(result) = self.execute_visual_command(&request.method, params.clone()) {
            return result;
        }
        match request.method.as_str() {
            "system.ping" => Ok(json!({
                "pong": true,
                "protocol_version": crate::agent_protocol::PROTOCOL_VERSION,
                "document": self.doc.name(),
                "mode": format!("{:?}", self.mode),
            })),
            "system.describe" => {
                let mut description = protocol_description();
                description["runtime"] = json!({
                    "address": self.agent_server.address().map(|address| address.to_string()),
                    "startup_error": self.agent_server.startup_error(),
                    "document": self.doc.name(),
                });
                Ok(description)
            }
            "workflow.get" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {
                    mode: Option<String>,
                }
                let input = Self::agent_parse::<Input>(params)?;
                crate::workflow_guide::guide(input.mode.as_deref().unwrap_or("auto"))
                    .map_err(|e| Self::agent_fault("invalid_params", e))
            }
            "feature.offset.start" => self.agent_offset_start(params),
            "surface.equation.preview" | "surface.equation.apply" => {
                self.agent_equation_surface(&request.method, params)
            }
            "curve.equation.preview" | "curve.equation.apply" => {
                self.agent_equation_curve(&request.method, params)
            }
            "feature.offset.status" | "feature.offset.commit" | "feature.offset.cancel" => {
                self.agent_offset_job(&request.method, params)
            }
            "document.get" => Ok(self.agent_document_value()),
            "thread.presets" => Ok(crate::threads::preset_values()),
            "thread.targets" => {
                let input = Self::agent_parse::<BodyIdsParams>(params)?;
                self.thread_targets(&input.body_ids)
                    .map(|v| json!(v))
                    .map_err(|e| Self::agent_fault("thread_error", e))
            }
            "thread.apply" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {
                    targets: Vec<crate::threads::Target>,
                    params: crate::threads::Params,
                }
                self.agent_require_idle()?;
                let input = Self::agent_parse::<Input>(params)?;
                self.apply_threads(input.targets, input.params)
                    .map(|ids| json!({"body_ids":ids}))
                    .map_err(|e| Self::agent_fault("thread_error", e))
            }
            "body.get" => {
                let params = Self::agent_parse::<BodyIdParams>(params)?;
                self.agent_body_value_by_id(params.body_id)
            }
            "selection.set" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<SelectionSetParams>(params)?;
                self.agent_set_selection(params.body_ids)
            }
            "primitive.create" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<PrimitiveCreateParams>(params)?;
                self.agent_create_primitive(params)
            }
            "sketch.create" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<SketchCreateParams>(params)?;
                self.agent_create_sketch(params)
            }
            "sketch.trim.preview" | "sketch.trim.apply" => {
                self.agent_trim_sketch(&request.method, Self::agent_parse(params)?)
            }
            "feature.extrude" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<ExtrudeFeatureParams>(params)?;
                self.agent_extrude_feature(params)
            }
            "feature.revolve" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<RevolveFeatureParams>(params)?;
                self.agent_revolve_feature(params)
            }
            "body.transform" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<TransformParams>(params)?;
                self.agent_transform_bodies(params)
            }
            "boolean.apply" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<BooleanApplyParams>(params)?;
                self.agent_boolean(params)
            }
            "body.rename" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<BodyRenameParams>(params)?;
                if self.doc.body(params.body_id).is_none() {
                    return Err(Self::agent_fault(
                        "not_found",
                        format!("body {} does not exist", params.body_id),
                    ));
                }
                if !self.rename_body(params.body_id, &params.name) {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "object name was rejected",
                    ));
                }
                self.agent_body_value_by_id(params.body_id)
            }
            "body.set_visibility" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<BodyVisibilityParams>(params)?;
                if self.doc.body(params.body_id).is_none() {
                    return Err(Self::agent_fault(
                        "not_found",
                        format!("body {} does not exist", params.body_id),
                    ));
                }
                self.set_body_visible(params.body_id, params.visible);
                self.agent_body_value_by_id(params.body_id)
            }
            "body.delete" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<BodyIdsParams>(params)?;
                self.agent_delete_bodies(params.body_ids)
            }
            "file.import" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<PathParams>(params)?;
                self.agent_import_file(&params.path)
            }
            "body.export" => {
                let params = Self::agent_parse::<ExportParams>(params)?;
                self.agent_export_body(params)
            }
            "project.save" => {
                self.agent_require_idle()?;
                let params = Self::agent_parse::<PathParams>(params)?;
                self.agent_save_project(&params.path)
            }
            "project.open" => {
                self.agent_prepare_mutation()?;
                let params = Self::agent_parse::<PathParams>(params)?;
                self.agent_open_project(&params.path)
            }
            "history.undo" => {
                self.agent_require_idle()?;
                if self.undo() {
                    Ok(self.agent_document_value())
                } else {
                    Err(Self::agent_fault(
                        "no_history",
                        "there is no operation to undo",
                    ))
                }
            }
            "history.redo" => {
                self.agent_require_idle()?;
                if self.redo() {
                    Ok(self.agent_document_value())
                } else {
                    Err(Self::agent_fault(
                        "no_history",
                        "there is no operation to redo",
                    ))
                }
            }
            "view.fit" => {
                self.fit_view();
                self.bump_scene();
                Ok(json!({"fitted": true}))
            }
            "view.recenter" => {
                self.agent_require_idle()?;
                self.recenter_view();
                Ok(json!({"recentered": true, "projection": "perspective"}))
            }
            method => Err(Self::agent_fault(
                "method_not_found",
                format!("unknown Agent Control method `{method}`; call system.describe"),
            )),
        }
    }

    fn agent_parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, (String, String)> {
        parse_params(params).map_err(|message| Self::agent_fault("invalid_params", message))
    }

    fn agent_fault(code: impl Into<String>, message: impl Into<String>) -> (String, String) {
        (code.into(), message.into())
    }

    fn agent_require_idle(&self) -> Result<(), (String, String)> {
        if self.mode == Mode::Idle
            && self.step_import_job.is_none()
            && self.fastener_job.is_none()
            && self.project_session.pending.is_none()
            && self.tool_exit_prompt.is_none()
            && !self.progress_busy()
        {
            Ok(())
        } else {
            Err(Self::agent_fault(
                "busy",
                format!(
                    "S33D is currently in {:?} mode; finish or cancel the interactive tool first",
                    self.mode
                ),
            ))
        }
    }

    fn agent_prepare_mutation(&mut self) -> Result<(), (String, String)> {
        self.agent_require_idle()?;
        self.deactivate_sidebar_tools();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        Ok(())
    }

    fn agent_document_value(&self) -> Value {
        json!({
            "name": self.doc.name(),
            "document_id": self.agent_clock.document_id, "revision": self.agent_clock.revision,
            "project_path": self.project_session.path,
            "unsaved_changes": self.project_has_changes(),
            "mode": format!("{:?}", self.mode),
            "unit": "mm",
            "selected_body_ids": self.selected,
            "body_count": self.doc.bodies.len(),
            "groups": self.doc.groups,
            "bodies": self.doc.bodies.iter().map(|body| self.agent_body_value(body)).collect::<Vec<_>>(),
            "can_undo": self.doc.can_undo(),
            "can_redo": self.doc.can_redo(),
            "history_tail": self.history.iter().rev().take(10).cloned().collect::<Vec<_>>(),
        })
    }

    fn agent_body_value_by_id(&self, body_id: BodyId) -> AgentCommandResult {
        let body = self.doc.body(body_id).ok_or_else(|| {
            Self::agent_fault("not_found", format!("body {body_id} does not exist"))
        })?;
        Ok(self.agent_body_value(body))
    }

    fn agent_body_value(&self, body: &Body) -> Value {
        let mesh = body
            .shape
            .and_then(|shape| self.kernel.tessellate(shape).ok());
        let bounds = mesh
            .as_ref()
            .and_then(MeshData::bounds)
            .or_else(|| equation_curve::bounds(&body.feature))
            .map(|(min, max)| {
                json!({
                    "min": min.to_array(),
                    "max": max.to_array(),
                    "size": (max - min).to_array(),
                    "center": ((min + max) * 0.5).to_array(),
                })
            });
        let sketch = body
            .sketch()
            .cloned()
            .or_else(|| self.profile_sketch(body.id))
            .map(|sketch| {
                let regions = s33d_model::compute_regions(&sketch.entities);
                json!({
                    "plane": sketch.plane,
                    "entities": sketch.entities,
                    "regions": regions.iter().enumerate().map(|(index, region)| json!({
                        "index": index,
                        "area": region.signed_area().abs(),
                        "outer_points": region.outer.len(),
                        "hole_count": region.holes.len(),
                    })).collect::<Vec<_>>()
                })
            });
        let feature_kind = match &body.feature {
            Feature::Fastener { .. } => "fastener",
            Feature::EquationCurve { .. } => "equation_curve",
            Feature::EquationSurface { .. } => "equation_surface",
            Feature::ExactOffset { .. } => "exact_offset",
            Feature::Sketch(_) => "sketch",
            Feature::Extrude { .. } => "extrude",
            Feature::Revolve { .. } => "revolve",
            Feature::Primitive3D { .. } => "primitive_3d",
            Feature::Gear { .. } => "gear",
            Feature::Tire { .. } => "tire",
            Feature::WheelRim { .. } => "wheel_rim",
            Feature::SurfaceTexture { .. } => "surface_texture",
            Feature::Derived { .. } => "derived",
            Feature::Imported { .. } => "imported",
            Feature::Chamfer { fillet, .. } => {
                if fillet.is_some() {
                    "fillet"
                } else {
                    "chamfer"
                }
            }
        };
        json!({
            "id": body.id,
            "name": body.name,
            "kind": match body.kind {BodyKind::Sketch=>"sketch",BodyKind::Curve=>"curve",BodyKind::Surface=>"surface",BodyKind::Solid=>"solid"},
            "feature_kind": feature_kind,
            "feature_description": body.feature.describe(),
            "feature": body.feature,
            "visible": body.visible,
            "selected": self.selected.contains(&body.id),
            "material": body.material,
            "bounds": bounds,
            "triangle_count": mesh.as_ref().map(MeshData::triangle_count),
            "exact_brep": body.shape.and_then(|id|self.kernel.exact_shape(id)).is_some(),
            "topology_face_count": body.shape.and_then(|id|self.kernel.exact_shape(id)).map(|shape|shape.face_count),
            "topology_faces": body.shape.and_then(|id|self.kernel.exact_shape(id)).map(|shape| {
                let mut bounds=vec![(glam::Vec3::splat(f32::INFINITY),glam::Vec3::splat(f32::NEG_INFINITY));shape.face_count as usize];
                for (triangle,&face) in shape.mesh.indices.chunks_exact(3).zip(&shape.triangle_faces) {
                    for &vertex in triangle { let p=glam::Vec3::from_array(shape.mesh.positions[vertex as usize]); bounds[face as usize].0=bounds[face as usize].0.min(p);bounds[face as usize].1=bounds[face as usize].1.max(p); }
                }
                bounds.into_iter().enumerate().map(|(index,(min,max))|json!({"index":index,"display_bounds":{"min":min.to_array(),"max":max.to_array()},"display_center":((min+max)*0.5).to_array()})).collect::<Vec<_>>()
            }),
            "sketch": sketch,
        })
    }

    fn agent_set_selection(&mut self, body_ids: Vec<BodyId>) -> AgentCommandResult {
        let mut unique = Vec::new();
        for body_id in body_ids {
            let body = self.doc.body(body_id).ok_or_else(|| {
                Self::agent_fault("not_found", format!("body {body_id} does not exist"))
            })?;
            if !body.visible {
                return Err(Self::agent_fault(
                    "invalid_selection",
                    format!("body {body_id} is hidden"),
                ));
            }
            if !unique.contains(&body_id) {
                unique.push(body_id);
            }
        }
        self.selected = unique;
        self.clear_region_selection();
        self.face_pick = None;
        self.selected_edge = None;
        self.bump_scene();
        Ok(json!({"selected_body_ids": self.selected}))
    }

    fn agent_create_primitive(&mut self, input: PrimitiveCreateParams) -> AgentCommandResult {
        let kind = match input.kind.trim().to_ascii_lowercase().as_str() {
            "box" => Primitive3DKind::Box,
            "cylinder" => Primitive3DKind::Cylinder,
            "sphere" => Primitive3DKind::Sphere,
            "cone" | "frustum" => Primitive3DKind::Cone,
            "torus" => Primitive3DKind::Torus,
            _ => {
                return Err(Self::agent_fault(
                    "invalid_params",
                    "kind must be box, cylinder, sphere, cone, or torus",
                ))
            }
        };
        let mut params = Primitive3DParams::defaults(kind);
        params.position = input.position;
        if let Some(value) = input.width {
            params.width = value;
        }
        if let Some(value) = input.depth {
            params.depth = value;
        }
        if let Some(value) = input.height {
            params.height = value;
        }
        if let Some(value) = input.radius {
            params.radius = value;
        }
        if let Some(value) = input.top_radius {
            params.top_radius = value;
        }
        if let Some(value) = input.major_radius {
            params.major_radius = value;
        }
        if let Some(value) = input.minor_radius {
            params.minor_radius = value;
        }
        let shape = Self::build_insert_3d_shape(&mut self.kernel, &params)
            .map_err(|message| Self::agent_fault("geometry_error", message))?;
        let had_visible_solid = self
            .doc
            .bodies
            .iter()
            .any(|body| body.kind == BodyKind::Solid && body.visible);
        let body_id = self.doc.alloc_id();
        let name = Self::agent_name(input.name, format!("{} {body_id}", kind.name()))?;
        let color_index = self.doc.bodies.len() + 1;
        self.begin_document_change();
        self.doc.add_body(Body {
            id: body_id,
            name,
            kind: BodyKind::Solid,
            feature: Feature::Primitive3D { params },
            shape: Some(shape),
            material: Material {
                color: palette_color(color_index),
                ..Material::default()
            },
            visible: true,
        });
        self.selected = vec![body_id];
        self.record_history(format!("Agent: create {} as object {body_id}", kind.name()));
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.bump_scene();
        if !had_visible_solid {
            self.fit_view();
        }
        self.agent_body_value_by_id(body_id)
    }

    fn agent_name(name: Option<String>, fallback: String) -> Result<String, (String, String)> {
        match name {
            Some(name) if name.trim().is_empty() => {
                Err(Self::agent_fault("invalid_params", "name cannot be empty"))
            }
            Some(name) => Ok(name.trim().to_string()),
            None => Ok(fallback),
        }
    }

    fn agent_create_sketch(&mut self, input: SketchCreateParams) -> AgentCommandResult {
        if input.entities.is_empty() {
            return Err(Self::agent_fault(
                "invalid_params",
                "sketch.create needs at least one entity",
            ));
        }
        if input.origin.iter().any(|value| !value.is_finite()) {
            return Err(Self::agent_fault("invalid_params", "origin must be finite"));
        }
        let plane_name = input.plane.trim().to_ascii_lowercase();
        let mut plane = match plane_name.as_str() {
            "xy" | "front" => DatumPlane::Front.def(),
            "yz" | "right" => DatumPlane::Right.def(),
            "zx" | "xz" | "top" => DatumPlane::Top.def(),
            "custom" => {
                let normal = input.normal.ok_or_else(|| {
                    Self::agent_fault("invalid_params", "custom plane requires normal")
                })?;
                let normal = glam::DVec3::from_array(normal);
                if !normal.is_finite() || normal.length_squared() < 1.0e-18 {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "custom plane normal must be finite and non-zero",
                    ));
                }
                PlaneDef::from_normal(normal.to_array())
            }
            _ => {
                return Err(Self::agent_fault(
                    "invalid_params",
                    "plane must be xy, yz, zx, or custom",
                ))
            }
        };
        plane.origin = input.origin;
        let entities = input
            .entities
            .into_iter()
            .map(Self::agent_sketch_entity)
            .collect::<Result<Vec<_>, _>>()?;
        let sketch = SketchFeature {
            vertex_locks: Default::default(),
            plane,
            entities,
            world_xyz_coordinates: plane_name == "custom",
            on_body: None,
        };
        let body_id = self.doc.alloc_id();
        let name = Self::agent_name(input.name, format!("Sketch {body_id}"))?;
        self.begin_document_change();
        self.doc.add_body(Body {
            id: body_id,
            name,
            kind: BodyKind::Sketch,
            feature: Feature::Sketch(sketch),
            shape: None,
            material: Material::default(),
            visible: true,
        });
        self.selected = vec![body_id];
        self.clear_region_selection();
        self.regions_cache.remove(&body_id);
        self.record_history(format!("Agent: create sketch object {body_id}"));
        self.bump_scene();
        self.agent_body_value_by_id(body_id)
    }

    fn agent_trim_sketch(&mut self, method: &str, input: SketchTrimParams) -> AgentCommandResult {
        use std::hash::{Hash, Hasher};
        // Never cancel or commit an in-progress desktop tool on behalf of an agent.
        self.agent_require_idle()?;
        if !input.point.iter().all(|v| v.is_finite())
            || !input.tolerance.is_finite()
            || input.tolerance <= 0.0
        {
            return Err(Self::agent_fault(
                "invalid_params",
                "point must be finite and tolerance must be positive finite millimeters",
            ));
        }
        let body = self
            .doc
            .body(input.sketch_id)
            .ok_or_else(|| Self::agent_fault("not_found", "sketch object does not exist"))?;
        let Feature::Sketch(mut sketch) = body.feature.clone() else {
            return Err(Self::agent_fault(
                "invalid_params",
                "Trim requires a committed Sketch, not a solid or Equation Curve",
            ));
        };
        // Include the plane and all entity metadata, not just tessellated geometry.
        // This is an opaque freshness token, not an authentication credential.
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&sketch)
            .expect("serializable sketch")
            .hash(&mut hash);
        input.sketch_id.hash(&mut hash);
        input.point.map(f64::to_bits).hash(&mut hash);
        input.tolerance.to_bits().hash(&mut hash);
        let token = format!("trim-v1-{:016x}", hash.finish());
        let apply = method == "sketch.trim.apply";
        if apply && input.preview_token.as_deref() != Some(token.as_str()) {
            return Err(Self::agent_fault("stale_preview", "Call sketch.trim.preview again and submit its token with the same point and tolerance"));
        }
        let plan = s33d_model::trim::plan_trim(&sketch.entities, input.point, input.tolerance)
            .ok_or_else(|| Self::agent_fault("no_hit", "No trimmable segment within tolerance"))?;
        sketch
            .entities
            .splice(plan.entity..=plan.entity, plan.replacement.clone());
        if !apply {
            let regions = s33d_model::compute_regions(&sketch.entities);
            return Ok(json!({
                "sketch_id": input.sketch_id, "preview_token": token,
                "entity_index": plan.entity, "removed": plan.removed,
                "replacement": plan.replacement, "entity_count_after": sketch.entities.len(),
                "regions_after": regions.iter().enumerate().map(|(index, r)| json!({
                    "index": index, "area": r.signed_area().abs(), "hole_count": r.holes.len()
                })).collect::<Vec<_>>(),
                "dependents_validated": false
            }));
        }
        // Build everything first. The existing dependency builder discards temporary
        // shapes on error; neither document history nor selection changes on failure.
        let rebuilt = self
            .rebuild_sketch_dependents(input.sketch_id, &sketch)
            .map_err(|error| Self::agent_fault("rebuild_failed", error))?;
        let rebuilt_ids: Vec<_> = rebuilt.iter().map(|(id, _, _)| *id).collect();
        self.agent_prepare_mutation()?;
        self.begin_document_change();
        self.doc.body_mut(input.sketch_id).unwrap().feature = Feature::Sketch(sketch);
        for (id, shape, feature) in rebuilt {
            let body = self.doc.body_mut(id).unwrap();
            body.shape = Some(shape);
            body.feature = feature;
        }
        self.selected = vec![input.sketch_id];
        self.clear_region_selection();
        self.regions_cache.clear();
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.collect_kernel_garbage();
        self.record_history(format!("Agent: trim sketch {}", input.sketch_id));
        self.bump_scene();
        Ok(
            json!({"body": self.agent_body_value_by_id(input.sketch_id)?, "rebuilt_body_ids": rebuilt_ids}),
        )
    }

    fn agent_sketch_entity(entity: AgentSketchEntity) -> Result<SketchEntity, (String, String)> {
        let finite2 = |point: [f64; 2]| point.iter().all(|value| value.is_finite());
        let positive = |value: f64, name: &str| {
            if value.is_finite() && value > 1.0e-6 {
                Ok(value)
            } else {
                Err(Self::agent_fault(
                    "invalid_params",
                    format!("{name} must be finite and greater than zero"),
                ))
            }
        };
        match entity {
            AgentSketchEntity::Line { start, end } => {
                if !finite2(start) || !finite2(end) || start == end {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "line endpoints must be finite and different",
                    ));
                }
                Ok(SketchEntity::Line { a: start, b: end })
            }
            AgentSketchEntity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
                counterclockwise,
            } => {
                if !finite2(center) || !start_deg.is_finite() || !end_deg.is_finite() {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "arc values must be finite",
                    ));
                }
                Ok(SketchEntity::Arc {
                    c: center,
                    r: positive(radius, "radius")?,
                    a0: start_deg.to_radians(),
                    a1: end_deg.to_radians(),
                    ccw: counterclockwise,
                })
            }
            AgentSketchEntity::Circle { center, radius } => {
                if !finite2(center) {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "circle center must be finite",
                    ));
                }
                Ok(SketchEntity::Circle {
                    c: center,
                    r: positive(radius, "radius")?,
                })
            }
            AgentSketchEntity::Rectangle { corner1, corner2 } => {
                if !finite2(corner1)
                    || !finite2(corner2)
                    || (corner1[0] - corner2[0]).abs() < 1.0e-9
                    || (corner1[1] - corner2[1]).abs() < 1.0e-9
                {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "rectangle corners must be finite and define non-zero width and height",
                    ));
                }
                Ok(SketchEntity::Rect {
                    c1: corner1,
                    c2: corner2,
                })
            }
            AgentSketchEntity::RotatedRectangle {
                center,
                width,
                height,
                rotation_deg,
            } => {
                if !finite2(center) || !rotation_deg.is_finite() {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "rotated rectangle values must be finite",
                    ));
                }
                Ok(SketchEntity::RotatedRect {
                    c: center,
                    width: positive(width, "width")?,
                    height: positive(height, "height")?,
                    rot: -rotation_deg.to_radians(),
                })
            }
            AgentSketchEntity::Polygon {
                center,
                radius,
                sides,
                rotation_deg,
            } => {
                if !finite2(center) || !rotation_deg.is_finite() || !(3..=1024).contains(&sides) {
                    return Err(Self::agent_fault(
                        "invalid_params",
                        "polygon needs a finite center/rotation and 3 to 1024 sides",
                    ));
                }
                Ok(SketchEntity::Polygon {
                    c: center,
                    r: positive(radius, "radius")?,
                    n: sides,
                    rot: rotation_deg.to_radians(),
                })
            }
        }
    }

    fn agent_extrude_feature(&mut self, input: ExtrudeFeatureParams) -> AgentCommandResult {
        if !input.depth.is_finite()
            || input.depth.abs() <= 1.0e-6
            || (input.symmetric && input.depth <= 0.0)
        {
            return Err(Self::agent_fault(
                "invalid_params",
                "depth must be finite and non-zero; symmetric depth must be positive",
            ));
        }
        let sketch = self.profile_sketch(input.sketch_id).ok_or_else(|| {
            Self::agent_fault(
                "not_found",
                format!("sketch body {} does not exist", input.sketch_id),
            )
        })?;
        let all_regions = s33d_model::compute_regions(&sketch.entities);
        let region_indices = Self::agent_region_indices(&input.region_indices, all_regions.len())?;
        let regions = region_indices
            .iter()
            .map(|index| all_regions[*index].clone())
            .collect::<Vec<_>>();
        let feature_params = ExtrudeParams {
            depth: input.depth,
            symmetric: input.symmetric,
            bevel: 0.0,
        };
        let shape = build_extrude_shape(&mut self.kernel, &sketch, &regions, &feature_params)
            .map_err(|message| Self::agent_fault("geometry_error", message))?;
        let body_id = self.doc.alloc_id();
        let name = Self::agent_name(input.name, format!("Extrude {body_id}"))?;
        let color_index = self.doc.bodies.len() + 1;
        self.begin_document_change();
        self.doc.add_body(Body {
            id: body_id,
            name,
            kind: BodyKind::Solid,
            feature: Feature::Extrude {
                sketch,
                params: feature_params,
                source_sketch: self.profile_source_link(input.sketch_id),
                pocket_source_sketches: Vec::new(),
                region_indices,
            },
            shape: Some(shape),
            material: Material {
                color: palette_color(color_index),
                ..Material::default()
            },
            visible: true,
        });
        self.selected = vec![body_id];
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.record_history(format!(
            "Agent: extrude sketch {} as object {body_id}",
            input.sketch_id
        ));
        self.bump_scene();
        self.agent_body_value_by_id(body_id)
    }

    fn agent_region_indices(
        requested: &[usize],
        region_count: usize,
    ) -> Result<Vec<usize>, (String, String)> {
        if requested.is_empty() {
            return Err(Self::agent_fault(
                "invalid_params",
                "region_indices cannot be empty",
            ));
        }
        let mut unique = Vec::new();
        for index in requested {
            if *index >= region_count {
                return Err(Self::agent_fault(
                    "invalid_params",
                    format!(
                        "region index {index} is out of range; sketch has {region_count} regions"
                    ),
                ));
            }
            if !unique.contains(index) {
                unique.push(*index);
            }
        }
        Ok(unique)
    }

    fn agent_revolve_feature(&mut self, input: RevolveFeatureParams) -> AgentCommandResult {
        if !input.angle_deg.is_finite() || input.angle_deg.abs() <= 1.0e-6 {
            return Err(Self::agent_fault(
                "invalid_params",
                "angle_deg must be finite and non-zero",
            ));
        }
        let axis = Axis2D {
            point: input.axis_point,
            dir: input.axis_direction,
        };
        if input
            .axis_point
            .iter()
            .chain(input.axis_direction.iter())
            .any(|value| !value.is_finite())
            || !axis.is_valid()
        {
            return Err(Self::agent_fault(
                "invalid_params",
                "revolve axis must be finite and have a non-zero direction",
            ));
        }
        let sketch = self.profile_sketch(input.sketch_id).ok_or_else(|| {
            Self::agent_fault(
                "not_found",
                format!("sketch body {} does not exist", input.sketch_id),
            )
        })?;
        let regions = s33d_model::compute_regions(&sketch.entities);
        let region = regions.get(input.region_index).cloned().ok_or_else(|| {
            Self::agent_fault(
                "invalid_params",
                format!(
                    "region index {} is out of range; sketch has {} regions",
                    input.region_index,
                    regions.len()
                ),
            )
        })?;
        s33d_model::triangulate_region(&region.outer, &region.holes).map_err(|error| {
            Self::agent_fault(
                "geometry_error",
                format!("Revolve region validation failed: {error}"),
            )
        })?;
        let shape = self
            .kernel
            .revolve_region(&region, &sketch.plane, &axis, input.angle_deg)
            .map_err(|error| Self::agent_fault("geometry_error", error.to_string()))?;
        let params = RevolveParams {
            axis_point: input.axis_point,
            axis_dir: input.axis_direction,
            angle_deg: input.angle_deg,
        };
        let body_id = self.doc.alloc_id();
        let name = Self::agent_name(input.name, format!("Revolve {body_id}"))?;
        let color_index = self.doc.bodies.len() + 1;
        self.begin_document_change();
        self.doc.add_body(Body {
            id: body_id,
            name,
            kind: BodyKind::Solid,
            feature: Feature::Revolve {
                sketch,
                params,
                source_sketch: self.profile_source_link(input.sketch_id),
                region_index: Some(input.region_index),
            },
            shape: Some(shape),
            material: Material {
                color: palette_color(color_index),
                ..Material::default()
            },
            visible: true,
        });
        self.selected = vec![body_id];
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.record_history(format!(
            "Agent: revolve sketch {} as object {body_id}",
            input.sketch_id
        ));
        self.bump_scene();
        self.agent_body_value_by_id(body_id)
    }

    fn agent_transform_bodies(&mut self, input: TransformParams) -> AgentCommandResult {
        let body_ids = self.agent_valid_solid_ids(input.body_ids, 1)?;
        if input
            .translation
            .iter()
            .chain(input.rotation_deg.iter())
            .chain(input.scale.iter())
            .any(|value| !value.is_finite())
            || input
                .pivot
                .is_some_and(|pivot| pivot.iter().any(|value| !value.is_finite()))
        {
            return Err(Self::agent_fault(
                "invalid_params",
                "transform values must be finite",
            ));
        }
        if input.scale.iter().any(|value| *value <= 1.0e-6) {
            return Err(Self::agent_fault(
                "invalid_params",
                "scale values must be greater than zero",
            ));
        }
        self.selected = body_ids.clone();
        let pivot = input
            .pivot
            .map(|pivot| glam::Vec3::from_array(pivot.map(|value| value as f32)))
            .or_else(|| self.selected_center())
            .unwrap_or(glam::Vec3::ZERO);
        let translation = glam::Vec3::from_array(input.translation.map(|value| value as f32));
        let rotation = glam::Quat::from_euler(
            glam::EulerRot::XYZ,
            (input.rotation_deg[0] as f32).to_radians(),
            (input.rotation_deg[1] as f32).to_radians(),
            (input.rotation_deg[2] as f32).to_radians(),
        );
        let scale = glam::Vec3::from_array(input.scale.map(|value| value as f32));
        let matrix = glam::Mat4::from_translation(translation)
            * glam::Mat4::from_translation(pivot)
            * glam::Mat4::from_quat(rotation)
            * glam::Mat4::from_scale(scale)
            * glam::Mat4::from_translation(-pivot);
        let mut transformed = Vec::with_capacity(body_ids.len());
        for body_id in &body_ids {
            let shape = self.doc.body(*body_id).and_then(|body| body.shape).unwrap();
            let new_shape = self
                .kernel
                .transform(shape, &matrix)
                .map_err(|error| Self::agent_fault("geometry_error", error.to_string()))?;
            transformed.push((*body_id, new_shape));
        }
        self.begin_document_change();
        for (body_id, shape) in transformed {
            let original = self.doc.body(body_id).unwrap().shape.unwrap();
            self.transform_thread_state(body_id, original, shape, &matrix);
            if let Some(body) = self.doc.body_mut(body_id) {
                body.shape = Some(shape);
                if let Feature::EquationSurface { params } = &mut body.feature {
                    params.transform = (matrix.as_dmat4()
                        * glam::DMat4::from_cols_array(&params.transform))
                    .to_cols_array();
                }
            }
        }
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.record_history(format!("Agent: transform objects {body_ids:?}"));
        self.bump_scene();
        Ok(json!({
            "transformed_body_ids": body_ids,
            "bodies": body_ids.iter().filter_map(|id| self.doc.body(*id)).map(|body| self.agent_body_value(body)).collect::<Vec<_>>()
        }))
    }

    fn agent_valid_solid_ids(
        &self,
        body_ids: Vec<BodyId>,
        minimum: usize,
    ) -> Result<Vec<BodyId>, (String, String)> {
        let mut unique = Vec::new();
        for body_id in body_ids {
            let body = self.doc.body(body_id).ok_or_else(|| {
                Self::agent_fault("not_found", format!("body {body_id} does not exist"))
            })?;
            if body.kind != BodyKind::Solid || body.shape.is_none() || !body.visible {
                return Err(Self::agent_fault(
                    "invalid_selection",
                    format!("body {body_id} must be a visible solid"),
                ));
            }
            if !unique.contains(&body_id) {
                unique.push(body_id);
            }
        }
        if unique.len() < minimum {
            return Err(Self::agent_fault(
                "invalid_selection",
                format!("this command needs at least {minimum} solid object(s)"),
            ));
        }
        Ok(unique)
    }

    fn agent_boolean(&mut self, input: BooleanApplyParams) -> AgentCommandResult {
        let operation = match input.operation.trim().to_ascii_lowercase().as_str() {
            "union" => DerivedBooleanOp::Union,
            "subtract" | "subtraction" => DerivedBooleanOp::Subtract,
            "intersect" | "intersection" => DerivedBooleanOp::Intersect,
            _ => {
                return Err(Self::agent_fault(
                    "invalid_params",
                    "operation must be union, subtract, or intersect",
                ))
            }
        };
        let body_ids = self.agent_valid_solid_ids(input.body_ids, 2)?;
        if operation != DerivedBooleanOp::Union && body_ids.len() != 2 {
            return Err(Self::agent_fault(
                "invalid_selection",
                "subtract and intersect require exactly two ordered body IDs",
            ));
        }
        let bodies = body_ids
            .iter()
            .filter_map(|body_id| self.doc.body(*body_id).cloned())
            .collect::<Vec<_>>();
        let first_shape = bodies[0].shape.unwrap();
        let mut result_shape = first_shape;
        let mut result_is_temporary = false;
        for body in bodies.iter().skip(1) {
            let tool_shape = body.shape.unwrap();
            let result = match operation {
                DerivedBooleanOp::Union => self.kernel.boolean_union(result_shape, tool_shape),
                DerivedBooleanOp::Subtract => {
                    self.kernel.boolean_subtract(result_shape, tool_shape)
                }
                DerivedBooleanOp::Intersect => {
                    self.kernel.boolean_intersect(result_shape, tool_shape)
                }
            };
            let new_shape = match result {
                Ok(shape) => shape,
                Err(error) => {
                    if result_is_temporary {
                        self.kernel.delete(result_shape);
                    }
                    return Err(Self::agent_fault("geometry_error", error.to_string()));
                }
            };
            if result_is_temporary {
                self.kernel.delete(result_shape);
            }
            result_shape = new_shape;
            result_is_temporary = true;
        }
        let body_id = self.doc.alloc_id();
        let operation_name = match operation {
            DerivedBooleanOp::Union => "Union",
            DerivedBooleanOp::Subtract => "Subtract",
            DerivedBooleanOp::Intersect => "Intersect",
        };
        let name = Self::agent_name(
            input.name,
            format!("Body {body_id} ({})", operation_name.to_ascii_lowercase()),
        )?;
        let material = bodies[0].material.clone();
        self.begin_document_change();
        if !input.keep_originals {
            for source_id in &body_ids {
                if let Some(body) = self.doc.body_mut(*source_id) {
                    body.visible = false;
                }
            }
        }
        self.doc.add_body(Body {
            id: body_id,
            name,
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: format!("{operation_name} of {body_ids:?}"),
                operation: Some(DerivedOperation::Boolean {
                    op: operation,
                    operands: body_ids.clone(),
                }),
            },
            shape: Some(result_shape),
            material,
            visible: true,
        });
        self.selected = vec![body_id];
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.record_history(format!(
            "Agent: {operation_name} {body_ids:?} -> object {body_id}"
        ));
        self.bump_scene();
        self.agent_body_value_by_id(body_id)
    }

    fn agent_delete_bodies(&mut self, body_ids: Vec<BodyId>) -> AgentCommandResult {
        if body_ids.is_empty() {
            return Err(Self::agent_fault(
                "invalid_selection",
                "body.delete needs at least one body ID",
            ));
        }
        let mut unique = Vec::new();
        for body_id in body_ids {
            if self.doc.body(body_id).is_none() {
                return Err(Self::agent_fault(
                    "not_found",
                    format!("body {body_id} does not exist"),
                ));
            }
            if !unique.contains(&body_id) {
                unique.push(body_id);
            }
        }
        self.selected = unique.clone();
        self.delete_selected();
        Ok(json!({
            "deleted_body_ids": unique,
            "document": self.agent_document_value()
        }))
    }

    fn agent_import_file(&mut self, path: &str) -> AgentCommandResult {
        let path = std::path::Path::new(path);
        let path_text = path.to_string_lossy().to_string();
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let shapes = match extension.as_str() {
            "stp" | "step" => self.kernel.import_step(&path_text),
            "igs" | "iges" => self.kernel.import_iges(&path_text),
            "stl" => self.kernel.import_stl(&path_text),
            "obj" => self.kernel.import_obj(&path_text),
            "ply" => self.kernel.import_ply(&path_text),
            "3mf" => self.kernel.import_3mf(&path_text),
            "glb" | "gltf" => self.kernel.import_glb(&path_text),
            "sldprt" | "sldasm" | "slddrw" | "x_t" | "x_b" => {
                return Err(Self::agent_fault(
                    "conversion_required",
                    crate::startup::native_cad_conversion_hint(self.settings.language),
                ))
            }
            _ => {
                return Err(Self::agent_fault(
                    "unsupported_format",
                    format!("unsupported import extension .{extension}"),
                ))
            }
        }
        .map_err(|error| Self::agent_fault("import_error", error.to_string()))?;
        if shapes.is_empty() {
            return Err(Self::agent_fault(
                "import_error",
                "the file contained no shapes",
            ));
        }
        self.begin_document_change();
        let start_index = self.doc.bodies.len() + 1;
        let mut body_ids = Vec::with_capacity(shapes.len());
        for (index, shape) in shapes.into_iter().enumerate() {
            let body_id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id: body_id,
                name: format!("Imported {}", start_index + index),
                kind: if self
                    .kernel
                    .exact_shape(shape)
                    .is_some_and(|s| s.solid_count == 0)
                {
                    BodyKind::Surface
                } else {
                    BodyKind::Solid
                },
                feature: Feature::Imported {
                    path: path_text.clone(),
                },
                shape: Some(shape),
                material: Material {
                    color: palette_color(start_index + index),
                    ..Material::default()
                },
                visible: true,
            });
            body_ids.push(body_id);
        }
        self.selected = body_ids.clone();
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.camera.set_view(crate::camera::ViewDir::Iso);
        self.fit_view();
        self.record_history(format!(
            "Agent: import {} object(s) from {path_text}",
            body_ids.len()
        ));
        self.bump_scene();
        Ok(json!({
            "imported_body_ids": body_ids,
            "bodies": body_ids.iter().filter_map(|id| self.doc.body(*id)).map(|body| self.agent_body_value(body)).collect::<Vec<_>>()
        }))
    }

    fn agent_export_body(&self, input: ExportParams) -> AgentCommandResult {
        let shape = self
            .doc
            .body(input.body_id)
            .and_then(|body| body.shape)
            .ok_or_else(|| {
                Self::agent_fault(
                    "not_found",
                    format!("solid body {} does not exist", input.body_id),
                )
            })?;
        let path = std::path::Path::new(&input.path);
        let format = input
            .format
            .map(|format| format.to_ascii_lowercase())
            .or_else(|| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(str::to_ascii_lowercase)
            })
            .ok_or_else(|| Self::agent_fault("invalid_params", "export format is missing"))?;
        let path_text = path.to_string_lossy().to_string();
        let result = match format.as_str() {
            "step" | "stp" => self.kernel.export_step(shape, &path_text),
            "stl" => self.kernel.export_stl(shape, &path_text),
            "obj" => self.kernel.export_obj(shape, &path_text),
            "ply" => self.kernel.export_ply(shape, &path_text),
            "glb" | "gltf" => self.kernel.export_glb(shape, &path_text),
            _ => {
                return Err(Self::agent_fault(
                    "unsupported_format",
                    format!("unsupported export format `{format}`"),
                ))
            }
        };
        result.map_err(|error| Self::agent_fault("export_error", error.to_string()))?;
        Ok(json!({
            "exported": true,
            "body_id": input.body_id,
            "format": format,
            "path": path_text
        }))
    }

    fn agent_save_project(&mut self, path: &str) -> AgentCommandResult {
        self.save_project_to_path(Path::new(path))
            .map_err(|error| Self::agent_fault("save_error", error))?;
        Ok(json!({"saved": true, "path": path}))
    }

    fn agent_open_project(&mut self, path: &str) -> AgentCommandResult {
        self.open_project_from_path(Path::new(path))
            .map_err(|message| Self::agent_fault("open_error", message))?;
        Ok(self.agent_document_value())
    }

    // ================= 选择 =================
    fn cylinder_targets_for_pick(
        &mut self,
        body_id: BodyId,
        shape: ShapeId,
    ) -> Vec<crate::threads::Target> {
        let mut provenance = Vec::new();
        let mut current = body_id;
        for _ in 0..32 {
            let Some(Feature::Derived {
                operation: Some(DerivedOperation::Copy { source, .. }),
                ..
            }) = self.doc.body(current).map(|body| &body.feature)
            else {
                break;
            };
            provenance.push((*source, self.doc.body(*source).and_then(|body| body.shape)));
            if *source == current {
                break;
            }
            current = *source;
        }
        if let Some((_, targets)) = self
            .cylinder_pick_cache
            .get(&shape)
            .filter(|(saved, _)| *saved == provenance)
        {
            return targets
                .iter()
                .cloned()
                .map(|mut t| {
                    t.body_id = body_id;
                    t
                })
                .collect();
        }
        let targets = self.thread_targets(&[body_id]).unwrap_or_default();
        if self.cylinder_pick_cache.len() >= 64 {
            self.cylinder_pick_cache.clear();
        }
        self.cylinder_pick_cache
            .insert(shape, (provenance, targets.clone()));
        targets
    }

    pub fn thread_target_for_face(&mut self, face: &FaceHit) -> Option<crate::threads::Target> {
        let shape = self.body(face.body_id)?.shape?;
        let targets = self.cylinder_targets_for_pick(face.body_id, shape);
        if targets.is_empty() {
            return None;
        }
        if !self.mesh_cache.contains_key(&shape) {
            self.mesh_for(shape);
        }
        let mesh = self.mesh_cache.get(&shape)?;
        let triangle = face.triangles.get(..3)?;
        targets
            .into_iter()
            .find(|t| t.contains_triangle(mesh, triangle))
    }

    /// Prepare parameters without activating a tool or changing selection.
    pub fn prepare_thread_draft(&mut self, target: &crate::threads::Target) {
        let expected = if target.internal {
            self.thread_params.diameter * 0.5 - self.thread_params.depth
        } else {
            self.thread_params.diameter * 0.5
                + self.thread_params.length * self.thread_params.taper * 0.5
        };
        if (expected - target.radius).abs() > 0.05 {
            let preset = crate::threads::presets()
                .into_iter()
                .filter_map(|(name, _, _, _)| crate::threads::preset_params(name, target.internal))
                .find(|p| {
                    p.taper == 0.
                        && ((if target.internal {
                            p.diameter * 0.5 - p.depth
                        } else {
                            p.diameter * 0.5
                        }) - target.radius)
                            .abs()
                            < 0.025
                });
            if let Some(mut p) = preset {
                p.length = self.thread_params.length;
                self.thread_params = p;
            } else {
                self.thread_params.taper = 0.;
                self.thread_params.depth = self.thread_params.depth.min(target.radius * 0.4);
                self.thread_params.diameter = 2.
                    * (target.radius
                        + if target.internal {
                            self.thread_params.depth
                        } else {
                            0.
                        });
            }
        }
        if self.thread_params.offset + self.thread_params.length > target.length {
            self.thread_params.offset = 0.;
        }
        self.thread_params.length = self.thread_params.length.min(target.length);
        // A previous very short lip may have clamped the draft below a pitch.
        // Selecting a longer shank should recover a usable default length.
        let available = target.length - self.thread_params.offset;
        if self.thread_params.length <= self.thread_params.pitch
            && available > self.thread_params.pitch
        {
            self.thread_params.length = available.min(8.0_f64.max(self.thread_params.pitch * 2.));
        }
    }

    /// Open a draft for precisely the selected wall; no geometry is cut here.
    pub fn begin_thread_on_face(&mut self, face: &FaceHit) -> bool {
        if self.mode != Mode::Idle {
            return false;
        }
        let Some(target) = self.thread_target_for_face(face) else {
            return false;
        };
        self.prepare_thread_draft(&target);
        self.deactivate_sidebar_tools();
        self.selected = vec![face.body_id];
        self.clear_region_selection();
        self.face_pick = Some(face.clone());
        self.thread_choices = vec![target];
        self.thread_checked = vec![true];
        self.open_thread_panel = true;
        self.bump_scene();
        true
    }

    pub fn thread_targets(&self, ids: &[BodyId]) -> Result<Vec<crate::threads::Target>, String> {
        let mut cached = HashMap::new();
        let mut visiting = HashSet::new();
        let mut targets = Vec::new();
        for &id in ids {
            self.doc
                .body(id)
                .filter(|b| b.visible && b.kind == BodyKind::Solid)
                .ok_or("Select visible solids")?;
            targets.extend(self.thread_targets_for_body(id, &mut cached, &mut visiting)?);
        }
        Ok(targets)
    }

    fn thread_targets_for_body(
        &self,
        id: BodyId,
        cached: &mut HashMap<BodyId, Vec<crate::threads::Target>>,
        visiting: &mut HashSet<BodyId>,
    ) -> Result<Vec<crate::threads::Target>, String> {
        use crate::threads::Target;
        if let Some(targets) = cached.get(&id) {
            return Ok(targets.clone());
        }
        // Copy provenance may include hidden sources; only the requested
        // destination must be visible. Bound recursion for malformed projects.
        if visiting.len() >= 32 || !visiting.insert(id) {
            return Ok(Vec::new());
        }
        let mut targets = Vec::new();
        if let Some(b) = self.doc.body(id).filter(|b| b.kind == BodyKind::Solid) {
            if let Feature::Derived { source, .. } = &b.feature {
                if let Some(text) = source.strip_prefix("Thread v1: ") {
                    let saved: crate::threads::Saved =
                        serde_json::from_str(text).map_err(|_| "Invalid saved thread surfaces")?;
                    let mesh = self
                        .kernel
                        .tessellate(b.shape.ok_or("Missing shape")?)
                        .map_err(|e| e.to_string())?;
                    if crate::threads::mesh_signature(&mesh) == saved.mesh_signature {
                        // Old saves dropped the entire selected wall after a
                        // partial thread. Recover uncut bands from annotations
                        // as well as the saved remaining-wall list.
                        targets.extend(crate::threads::remaining_targets(
                            id,
                            &saved.remaining,
                            &saved.annotations,
                            &mesh,
                        ));
                    } else {
                        // Older projects may have moved a threaded shape without
                        // updating its saved frame. Recover only complete walls
                        // from the actual geometry; stale coordinates are not authority.
                        targets.extend(crate::mesh_cylinders::targets(&mesh, id));
                    }
                    visiting.remove(&id);
                    cached.insert(id, targets.clone());
                    return Ok(targets);
                }
            }
            if let Some(exact) = b.shape.and_then(|s| self.kernel.exact_shape(s)) {
                for c in exact.cylinders().map_err(|e| e.to_string())? {
                    targets.push(Target {
                        body_id: id,
                        face_id: c[0] as u32,
                        origin: [c[1], c[2], c[3]],
                        axis: [c[4], c[5], c[6]],
                        radius: c[7],
                        length: c[8],
                        internal: c[9] != 0.,
                    });
                }
            }
            if targets.is_empty() {
                if let Some(p) = copy_primitive_snapshot(&b.feature, [0.; 3])
                    .filter(|p| p.kind == Primitive3DKind::Cylinder)
                {
                    let frame = p.frame.unwrap_or_else(|| DatumPlane::Top.def());
                    let seed = Target {
                        body_id: id,
                        face_id: u32::MAX,
                        origin: p.position,
                        axis: frame.normal().normalize().to_array(),
                        radius: p.radius,
                        length: p.height,
                        internal: false,
                    };
                    let mesh = self
                        .kernel
                        .tessellate(b.shape.ok_or("Missing shape")?)
                        .map_err(|e| e.to_string())?;
                    // Parameters alone are not authority after a transform.
                    // Require the current mesh to contain the complete wall.
                    if seed.matches_full_wall(&mesh) {
                        targets.push(seed);
                    } else if let Some(target) = seed.recover_primitive_cylinder(&mesh) {
                        // Only parameter-backed primitives reach this path.
                        // Certify the entire closed cylinder after transforms
                        // or re-tessellation; never fit arbitrary imported STL.
                        targets.push(target);
                    }
                }
            }
            if targets.is_empty() {
                if let Feature::Derived {
                    operation: Some(DerivedOperation::Copy { source, offset, .. }),
                    ..
                } = &b.feature
                {
                    let inherited = self.thread_targets_for_body(*source, cached, visiting)?;
                    if !inherited.is_empty() {
                        if let Some(source_shape) =
                            self.doc.body(*source).and_then(|source| source.shape)
                        {
                            let source_mesh = self
                                .kernel
                                .tessellate(source_shape)
                                .map_err(|e| e.to_string())?;
                            let mesh = self
                                .kernel
                                .tessellate(b.shape.ok_or("Missing shape")?)
                                .map_err(|e| e.to_string())?;
                            let fitted = crate::threads::copied_mesh_transform(&source_mesh, &mesh);
                            let translation =
                                glam::DMat4::from_translation(glam::DVec3::from_array(*offset));
                            targets.extend(inherited.iter().enumerate().filter_map(
                                |(index, seed)| {
                                    // Truck B-rep transforms can reorder display
                                    // vertices. A Copy's recorded translation is
                                    // accepted only when the whole wall agrees.
                                    let translated = seed
                                        .transformed(id, &translation)
                                        .filter(|target| target.matches_full_wall(&mesh));
                                    let mut target = translated.or_else(|| {
                                        seed.transformed(id, fitted.as_ref()?)
                                            .filter(|target| target.matches_full_wall(&mesh))
                                    })?;
                                    // Stable synthetic IDs distinguish the inner
                                    // and outer walls of old mesh-only copies.
                                    target.face_id = u32::MAX - index as u32;
                                    Some(target)
                                },
                            ));
                        }
                    }
                }
            }
        }
        // Exact Boolean/STEP shapes can mix analytic faces with faceted walls.
        // Add certified mesh bands as well, preserving native/provenance IDs
        // for duplicate geometry rather than dropping the other walls.
        if let Some(shape) = self.doc.body(id).and_then(|b| b.shape) {
            let mesh = self.kernel.tessellate(shape).map_err(|e| e.to_string())?;
            for candidate in crate::mesh_cylinders::targets(&mesh, id) {
                if !targets.iter().any(|t| t.same_wall(&candidate)) {
                    targets.push(candidate);
                }
            }
        }
        visiting.remove(&id);
        cached.insert(id, targets.clone());
        Ok(targets)
    }

    pub fn apply_threads(
        &mut self,
        targets: Vec<crate::threads::Target>,
        params: crate::threads::Params,
    ) -> Result<Vec<BodyId>, String> {
        if self.background_modeling {
            let ids = targets.iter().map(|t| t.body_id).collect();
            self.start_model_job(
                ("Generate thread", "生成螺纹"),
                true,
                None,
                move |worker| {
                    worker
                        .apply_threads(targets, params)
                        .map(|ids| json!(ids))
                        .map_err(|e| ("geometry".into(), e))
                },
            )?;
            return Ok(ids);
        }
        self.agent_require_idle().map_err(|(_, e)| e)?;
        if targets.is_empty() || targets.len() > 32 {
            return Err("Select 1 to 32 cylindrical surfaces".into());
        }
        let mut ids: Vec<_> = targets.iter().map(|t| t.body_id).collect();
        ids.sort_unstable();
        ids.dedup();
        self.report_model_progress(("Checking cylindrical surfaces…", "正在检查圆柱面…"), None)?;
        let current = self.thread_targets(&ids)?;
        let mut seen = HashSet::new();
        for t in &targets {
            if !seen.insert((t.body_id, t.face_id))
                || !current
                    .iter()
                    .any(|c| serde_json::to_value(c).ok() == serde_json::to_value(t).ok())
            {
                return Err(
                    "Surface selection changed or is duplicated; collect targets again".into(),
                );
            }
        }
        let mut outputs = Vec::new();
        // Compute all bodies before allocating document shapes or changing history.
        let total = targets.len();
        let mut completed = 0;
        for &id in &ids {
            let mut mesh = self
                .kernel
                .tessellate(self.doc.body(id).unwrap().shape.ok_or("Missing shape")?)
                .map_err(|e| e.to_string())?;
            let previous_signature = crate::threads::mesh_signature(&mesh);
            for t in targets.iter().filter(|t| t.body_id == id) {
                if !t.matches_full_wall(&mesh) {
                    return Err("A previous thread cut changed this cylindrical wall; collect targets again".into());
                }
                self.report_model_progress(
                    ("Cutting threads…", "正在生成螺纹…"),
                    Some((completed, total)),
                )?;
                mesh = crate::threads::cut(&mesh, t, &params)?;
                completed += 1;
            }
            let mut annotations = match &self.doc.body(id).unwrap().feature {
                Feature::Derived { source, .. } => source
                    .strip_prefix("Thread v1: ")
                    .and_then(|s| serde_json::from_str::<crate::threads::Saved>(s).ok())
                    .filter(|s| s.mesh_signature == previous_signature)
                    .map(|s| s.annotations)
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            annotations.extend(targets.iter().filter(|t| t.body_id == id).map(|t| {
                crate::threads::ThreadAnnotation {
                    target: t.clone(),
                    params: params.clone(),
                }
            }));
            let seeds: Vec<_> = current
                .iter()
                .filter(|t| t.body_id == id)
                .cloned()
                .collect();
            let remaining = crate::threads::remaining_targets(id, &seeds, &annotations, &mesh);
            let saved = crate::threads::Saved {
                pocket_sources: self.pocket_sources(id),
                annotations,
                remaining,
                params: params.clone(),
                mesh_signature: crate::threads::mesh_signature(&mesh),
            };
            outputs.push((
                id,
                mesh,
                serde_json::to_string(&saved).map_err(|e| e.to_string())?,
            ));
        }
        self.report_model_progress(
            ("Validating result…", "正在检查结果…"),
            Some((completed, total)),
        )?;
        self.agent_prepare_mutation().map_err(|(_, e)| e)?;
        self.begin_document_change();
        for (id, mesh, remaining) in outputs {
            let shape = self.kernel.insert_mesh(mesh);
            let b = self.doc.body_mut(id).unwrap();
            b.shape = Some(shape);
            b.feature = Feature::Derived {
                source: format!("Thread v1: {remaining}"),
                operation: None,
            };
        }
        self.selected = ids.clone();
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.record_history(format!(
            "Thread {} surfaces on {} objects",
            targets.len(),
            ids.len()
        ));
        self.bump_scene();
        Ok(ids)
    }

    /// Range selection follows document/list order, never numeric BodyId order.
    /// Visibility does not exclude rows from an Objects-list range.
    pub fn select_object_range(&mut self, target: BodyId, additive: bool) {
        let solids_only = self.align_active || self.mode == Mode::Boolean;
        let mut ids: Vec<_> = self
            .doc
            .bodies
            .iter()
            .filter(|b| !solids_only || (b.kind == BodyKind::Solid && b.shape.is_some()))
            .map(|b| b.id)
            .collect();
        ids.sort_by_key(|id| {
            self.doc
                .groups
                .iter()
                .position(|g| g.members.contains(id))
                .unwrap_or(self.doc.groups.len())
        });
        let Some(end) = ids.iter().position(|id| *id == target) else {
            return;
        };
        let anchor = self
            .object_list_anchor
            .filter(|id| ids.contains(id))
            .or_else(|| self.selected.iter().copied().find(|id| ids.contains(id)))
            .unwrap_or(target);
        let start = ids.iter().position(|id| *id == anchor).unwrap();
        self.object_list_anchor = Some(anchor);
        let mut next = if additive {
            self.selected.clone()
        } else {
            Vec::new()
        };
        for id in &ids[start.min(end)..=start.max(end)] {
            if !next.contains(id) {
                next.push(*id);
            }
        }
        if next.len() > 1 {
            if let Some(operation) = self.active_single_object_tool() {
                self.clear_object_selection_for_conflict(format!(
                    "{operation} accepts exactly one object; range selection cancelled"
                ));
                return;
            }
        }
        self.selected = next;
        self.clear_region_selection();
        self.selected_edge = None;
        self.hover_edge = None;
        self.face_pick = None;
        self.hover_face = None;
        if self.align_active {
            self.refresh_align_reference(Some(target));
        }
        self.bump_scene();
    }

    pub fn select(&mut self, id: BodyId, additive: bool) {
        self.select_object(id, additive, false);
    }

    /// List selection can manage hidden objects without making them visible.
    pub fn select_from_objects(&mut self, id: BodyId, additive: bool) {
        self.select_object(id, additive, true);
    }

    fn select_object(&mut self, id: BodyId, additive: bool, allow_hidden: bool) {
        if !self
            .doc
            .body(id)
            .is_some_and(|body| allow_hidden || body.visible)
        {
            return;
        }
        if additive || self.selected.as_slice() != [id] {
            self.clear_region_selection();
            self.selected_edge = None;
            self.hover_edge = None;
            self.face_pick = None;
            self.hover_face = None;
        }
        if additive {
            if let Some(pos) = self.selected.iter().position(|&s| s == id) {
                self.selected.remove(pos);
                self.record_history(format!("Deselect body {id}"));
            } else {
                self.selected.push(id);
            }
        } else {
            if self.selected.as_slice() != [id] {
                self.selected = vec![id];
                self.clear_region_selection();
                // 记录选择（含对象名与类型）
                let desc = self
                    .doc
                    .body(id)
                    .map(|b| {
                        format!(
                            "Select {} \"{}\" ({})",
                            id,
                            b.name,
                            if b.is_sketch() { "sketch" } else { "solid" }
                        )
                    })
                    .unwrap_or_else(|| format!("Select body {id}"));
                self.record_history(desc);
            }
            // 只有在普通选择状态下，选中 chamfer 结果体才进入特征编辑。
            // Align/Array/变换过程中，Objects 列表只是选择来源，不能抢走当前工具。
            let may_restore_chamfer = self.mode == Mode::Idle
                && !self.drawing.open
                && self.doc.body(id).is_some_and(|body| body.visible)
                && self.gizmo.is_none()
                && !self.align_active
                && !self.array_active;
            if let Some(b) = self.doc.body(id).cloned() {
                if may_restore_chamfer && matches!(b.feature, Feature::Chamfer { .. }) {
                    self.restore_chamfer_from_feature(&b);
                } else {
                    // 切换 object 后不能保留上一个 object 的子实体选择。
                    self.selected_edge = None;
                    self.hover_edge = None;
                }
            }
        }
        if self.selected.len() > 1 {
            if let Some(operation) = self.active_single_object_tool() {
                self.clear_object_selection_for_conflict(format!(
                    "{operation} accepts exactly one object; select one object and try again"
                ));
                return;
            }
        }
        if self.align_active {
            self.refresh_align_reference(Some(id));
        }
        self.bump_scene();
    }

    /// 工作区层级选择只更新 object 集合，不触发 Objects 列表的特征恢复逻辑。
    /// 返回目标 object 在本次操作后是否仍处于选中状态。
    fn select_workspace_owner(&mut self, id: BodyId, additive: bool) -> bool {
        if !self.doc.body(id).is_some_and(|body| body.visible) {
            return false;
        }
        let previous = self.selected.clone();
        let selected_now = apply_workspace_owner_selection(&mut self.selected, id, additive);
        if previous != self.selected {
            self.clear_region_selection();
            self.selected_edge = None;
            self.hover_edge = None;
            self.face_pick = None;
            self.hover_face = None;
        }
        if self.selected.len() > 1 {
            if let Some(operation) = self.active_single_object_tool() {
                self.clear_object_selection_for_conflict(format!(
                    "{operation} accepts exactly one object; select one object and try again"
                ));
                return false;
            }
        }
        if self.align_active {
            self.refresh_align_reference(Some(id));
        }
        selected_now
    }

    /// Align 选择变化后保留有效锁定体；若还没有参考体，
    /// 优先将本次新加入的 solid 作为默认锁定体。
    fn refresh_align_reference(&mut self, preferred: Option<BodyId>) {
        let eligible: Vec<BodyId> = self
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.doc.body(*id).is_some_and(|body| {
                    body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
                })
            })
            .collect();
        self.align_reference = align_reference_after_selection(
            self.align_reference,
            &self.selected,
            &eligible,
            preferred,
        );
    }

    fn capture_click_selection(&self) -> ClickSelectionSnapshot {
        ClickSelectionSnapshot {
            operation: selection_gestures::ClickToolState::capture(self),
            sidebar: selection_gestures::SidebarClickState::capture(self),
            camera: self.camera.clone(),
            bodies: self.selected.clone(),
            regions: self.selected_regions.clone(),
            region_source: self.selected_region_source,
            primary_edge: self.selected_edge.clone(),
            primary_face: self.face_pick.clone(),
        }
    }

    fn restore_click_selection(&mut self) {
        let Some(snapshot) = self.click_selection_base.clone() else {
            return;
        };
        self.selected = snapshot.bodies;
        self.selected_regions = snapshot.regions;
        self.selected_region_source = snapshot.region_source;
        self.sanitize_region_selection();
        self.selected_edge = snapshot.primary_edge;
        self.face_pick = snapshot.primary_face;
    }

    pub fn rename_body(&mut self, id: BodyId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            self.toast("Object name cannot be empty");
            return false;
        }
        let Some(previous) = self.doc.body(id).map(|body| body.name.clone()) else {
            self.toast("Object no longer exists");
            return false;
        };
        if previous == name {
            return true;
        }
        self.begin_document_change();
        if let Some(body) = self.doc.body_mut(id) {
            body.name = name.to_string();
        }
        self.record_history(format!("Rename object {id}: {previous} → {name}"));
        self.toast(format!("Renamed to {name}"));
        self.bump_scene();
        true
    }

    fn existing_object_ids(&self, ids: &[BodyId]) -> Vec<BodyId> {
        self.doc
            .bodies
            .iter()
            .filter(|b| ids.contains(&b.id))
            .map(|b| b.id)
            .collect()
    }

    /// A multi-selection is the context-menu target, even if the pointer is
    /// over another row. Single-object menus retain the clicked-row behavior.
    pub fn objects_context_targets(&self, clicked: BodyId) -> Vec<BodyId> {
        let selected = self.existing_object_ids(&self.selected);
        if selected.len() > 1 {
            selected
        } else {
            self.existing_object_ids(&[clicked])
        }
    }

    pub fn set_body_visible(&mut self, id: BodyId, visible: bool) {
        self.set_bodies_visible(&[id], visible);
    }

    pub fn set_bodies_visible(&mut self, ids: &[BodyId], visible: bool) {
        let ids = self.existing_object_ids(ids);
        if self.mode == Mode::Mirror
            && self
                .mirror_ctx
                .as_ref()
                .is_some_and(|ctx| ids.iter().any(|id| ctx.sources.contains(id)))
        {
            self.toast("Mirror source objects are locked; Cancel to change them");
            return;
        }
        if !ids
            .iter()
            .any(|&id| self.doc.body(id).is_some_and(|b| b.visible != visible))
        {
            return;
        }
        self.begin_document_change();
        for &id in &ids {
            if let Some(body) = self.doc.body_mut(id) {
                body.visible = visible;
            }
            if !visible {
                self.clear_unavailable_object_references(id);
            }
        }
        self.mesh_cache.clear();
        self.bump_scene();
        let action = if visible { "Show" } else { "Hide" };
        self.record_history(if ids.len() == 1 {
            format!("{action} object {}", ids[0])
        } else {
            format!("{action} {} objects", ids.len())
        });
    }

    fn clear_unavailable_object_references(&mut self, id: BodyId) {
        self.selected.retain(|selected| *selected != id);
        if self.hover_body == Some(id) {
            self.hover_body = None;
        }
        if self
            .face_pick
            .as_ref()
            .is_some_and(|face| face.body_id == id)
        {
            self.face_pick = None;
        }
        if self
            .hover_face
            .as_ref()
            .is_some_and(|face| face.body_id == id)
        {
            self.hover_face = None;
        }
        if self
            .selected_edge
            .as_ref()
            .is_some_and(|edge| edge.body_id == id)
        {
            self.selected_edge = None;
        }
        if self
            .hover_edge
            .as_ref()
            .is_some_and(|edge| edge.body_id == id)
        {
            self.hover_edge = None;
        }
        let reset_boolean = self.boolean_ctx.as_mut().and_then(|ctx| {
            if ctx.target == Some(id) {
                ctx.target = None;
                ctx.picking = Some(BooleanOperandSlot::Target);
                Some((ctx.op, "Original"))
            } else if ctx.tool == Some(id) {
                ctx.tool = None;
                ctx.picking = Some(BooleanOperandSlot::Tool);
                Some((ctx.op, "operation"))
            } else {
                None
            }
        });
        if let Some((op, operand)) = reset_boolean {
            self.set_hint(format!(
                "{}: {operand} object was hidden; select a visible replacement",
                op.name(),
            ));
        }
        if self.align_active {
            self.refresh_align_reference(None);
        }
    }

    /// Done：完成当前 chamfer 操作（定稿），保持工具激活，等待选下一条棱。
    pub fn chamfer_done(&mut self) {
        if self.selected_edge.is_none() {
            self.toast("No active chamfer to commit");
            return;
        }
        self.chamfer_committed = true;
        self.selected_edge = None;
        self.hover_edge = None;
        self.bump_scene();
        self.toast("Chamfer committed - select another edge for a new object, or Esc to exit");
    }

    /// Exit accepts the latest edge parameters and finishes the tool.
    pub fn exit_chamfer_tool(&mut self) {
        self.exit_tool_for_workspace_selection();
    }

    /// 选中 chamfer 结果体时：恢复棱选择与参数（点选后可改参数重算）。
    fn restore_chamfer_from_feature(&mut self, body: &Body) {
        if let Feature::Chamfer {
            target,
            edges,
            chamfer,
            fillet,
            ..
        } = &body.feature
        {
            self.selected_edge = Some(EdgeRef {
                body_id: *target,
                segments: edges
                    .iter()
                    .map(|s| EdgeSeg {
                        a: [s.a[0] as f32, s.a[1] as f32, s.a[2] as f32],
                        b: [s.b[0] as f32, s.b[1] as f32, s.b[2] as f32],
                        n1: [s.n1[0] as f32, s.n1[1] as f32, s.n1[2] as f32],
                        n2: [s.n2[0] as f32, s.n2[1] as f32, s.n2[2] as f32],
                    })
                    .collect(),
            });
            self.chamfer_fillet = fillet.is_some();
            self.chamfer_param = chamfer.unwrap_or_else(|| fillet.unwrap_or(2.0)) as f32;
            self.chamfer_active = true;
            // 从 Objects 恢复的是“编辑这个已有节点”，绝不能继承 Done 后的
            // 新建状态，否则会从历史节点分叉并丢掉其后的所有效果。
            self.chamfer_committed = false;
        }
    }

    pub fn clear_selection(&mut self) {
        if !self.selected.is_empty()
            || !self.selected_regions.is_empty()
            || self.selected_edge.is_some()
            || self.face_pick.is_some()
        {
            if self.debug_enabled {
                self.record_history(format!(
                    "[sel] clear_selection (was regions={:?})",
                    self.selected_regions
                ));
            }
            self.selected.clear();
            self.clear_region_selection();
            self.selected_edge = None;
            self.face_pick = None;
            self.bump_scene();
        }
    }

    pub fn selected_bodies(&self) -> Vec<BodyId> {
        self.selected.clone()
    }

    pub fn body(&self, id: BodyId) -> Option<&Body> {
        self.doc.body(id)
    }

    /// 关闭 Idle 状态下所有互斥的左栏操作工具。
    /// Objects 窗口和 Copy/Paste/Delete 是独立窗口/即时命令，不包含在这里。
    fn deactivate_sidebar_tools(&mut self) {
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.collapse_tool_menus();
        self.drawing.open = false;
        self.drawing.pending.clear();
        self.gizmo = None;
        self.gizmo_drag = None;
        self.rotate_custom_axis = None;
        self.rotate_axis_pick_active = false;
        self.align_active = false;
        self.align_reference = None;
        self.axis_align.picking = false;
        self.array_active = false;
        self.chamfer_active = false;
        self.selected_edge = None;
        self.hover_edge = None;
        self.last_click = None;
        self.click_count = 0;
        self.click_selection_base = None;
    }

    fn active_single_object_tool(&self) -> Option<&'static str> {
        if self.mode != Mode::Idle {
            return None;
        }
        if self.array_active {
            return Some("Array");
        }
        if self.chamfer_active {
            return Some("Chamfer/Fillet");
        }
        None
    }

    fn clear_object_selection_for_conflict(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        if self.mode == Mode::Idle {
            self.deactivate_sidebar_tools();
        }
        let boolean_op = self.boolean_ctx.as_mut().and_then(|ctx| {
            if ctx.op == BoolOp::Union {
                return None;
            }
            ctx.target = None;
            ctx.tool = None;
            ctx.picking = Some(BooleanOperandSlot::Target);
            Some(ctx.op)
        });
        self.selected.clear();
        self.clear_region_selection();
        self.face_pick = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.align_reference = None;
        let message = format!("Selection cleared: {reason}");
        self.toast(message.clone());
        self.record_history(format!("[sel] {message}"));
        if let Some(op) = boolean_op {
            self.set_boolean_hint(op);
        } else if self.mode == Mode::Idle {
            self.set_hint(message);
        }
        self.bump_scene();
    }

    pub fn set_body_color(&mut self, id: BodyId, color: [f32; 4]) {
        self.set_bodies_color(&[id], color);
    }

    pub fn set_bodies_color(&mut self, ids: &[BodyId], color: [f32; 4]) {
        let ids = self.existing_object_ids(ids);
        if !ids.iter().any(|&id| {
            self.doc.body(id).is_some_and(|body| {
                body.material
                    .color
                    .iter()
                    .zip(color.iter())
                    .any(|(a, b)| (a - b).abs() >= 1e-5)
            })
        }) {
            return;
        }
        self.begin_document_change();
        for &id in &ids {
            if let Some(body) = self.doc.body_mut(id) {
                body.material.color = color;
            }
        }
        self.record_history(if ids.len() == 1 {
            format!("Change object {} color", ids[0])
        } else {
            format!("Change {} objects color", ids.len())
        });
        self.bump_scene();
    }

    fn require_single_object(&mut self, operation: &'static str) -> Option<BodyId> {
        match self.selected.as_slice() {
            [id] => Some(*id),
            [] => {
                self.toast(format!("{operation} needs one selected object"));
                None
            }
            _ => {
                self.clear_object_selection_for_conflict(format!(
                    "{operation} accepts exactly one object; select one object and try again"
                ));
                None
            }
        }
    }

    pub fn toggle_gizmo_tool(&mut self, gizmo: GizmoMode) {
        if self.queue_tool_switch(move |worker| worker.toggle_gizmo_tool(gizmo)) {
            return;
        }
        if !self.finish_creation_for_tool_switch() || self.mode != Mode::Idle {
            return;
        }
        let enable = self.gizmo != Some(gizmo);
        let selected_size = (enable && gizmo == GizmoMode::Scale)
            .then(|| {
                self.selected_bounds()
                    .map(|(min, max)| (max - min).to_array())
            })
            .flatten();
        self.deactivate_sidebar_tools();
        if enable {
            self.gizmo = Some(gizmo);
            self.exit_move_to_baseline = self.move_to_input;
            if gizmo == GizmoMode::Scale {
                self.scale_input = [100.0; 3];
                self.scale_uniform = true;
                self.scale_size_input = selected_size.unwrap_or([0.0; 3]);
                self.scale_size_uniform = true;
            }
            self.exit_scale_size_baseline = self.scale_size_input;
        }
        self.bump_scene();
    }

    pub fn toggle_align_tool(&mut self) {
        if self.queue_tool_switch(Self::toggle_align_tool) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.mode != Mode::Idle {
            return;
        }
        let enable = !self.align_active;
        self.deactivate_sidebar_tools();
        if enable {
            self.align_active = true;
            // 默认以最后加入选择的 solid 为基准，面板中可改锁定对象。
            self.align_reference = self.selected.iter().rev().copied().find(|id| {
                self.doc
                    .body(*id)
                    .is_some_and(|b| b.kind == BodyKind::Solid && b.shape.is_some())
            });
            self.set_hint(
                "Align: add/remove objects in Objects or the viewport (Ctrl+A = all visible solids), choose Axis and Mode, then Apply",
            );
        } else {
            self.set_hint("Ready");
        }
        self.bump_scene();
    }

    pub fn toggle_array_tool(&mut self) {
        if self.queue_tool_switch(Self::toggle_array_tool) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.mode != Mode::Idle {
            return;
        }
        if self.require_single_object("Array").is_none() {
            return;
        }
        let enable = !self.array_active;
        self.deactivate_sidebar_tools();
        self.array_active = enable;
        self.array_exit_applied = None;
        self.bump_scene();
    }

    pub fn toggle_chamfer_tool(&mut self) {
        if self.queue_tool_switch(Self::toggle_chamfer_tool) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.mode != Mode::Idle {
            return;
        }
        let enable = !self.chamfer_active;
        let preselected_edge = self.selected_edge.clone();
        self.deactivate_sidebar_tools();
        if enable {
            self.chamfer_active = true;
            self.selected_edge = preselected_edge
                .filter(|edge| self.doc.body(edge.body_id).is_some_and(|body| body.visible));
        }
        self.bump_scene();
    }

    /// Esc 使用：退出当前 Idle 工具；返回是否确实关闭了一个工具。
    pub fn exit_sidebar_tool(&mut self) -> bool {
        let active =
            self.gizmo.is_some() || self.align_active || self.array_active || self.chamfer_active;
        if active {
            self.exit_tool_for_workspace_selection();
        }
        active
    }

    // ================= Draw 3D =================

    /// Commit a creation session before entering another top-level tool.
    /// Invalid geometry must leave the original session intact.
    pub fn finish_creation_for_tool_switch(&mut self) -> bool {
        self.drawing.open = false;
        self.drawing.pending.clear();
        let was_idle = self.mode == Mode::Idle;
        if !self.apply_tool_before_exit() {
            if self.tool_exit_waiting() {
                self.pending_tool_exit = Some(None);
            }
            self.toast("Finish the current operation before switching tools");
            return false;
        }
        if !was_idle {
            self.collapse_tool_menus();
        }
        true
    }

    pub fn start_insert_3d(&mut self) {
        if self.queue_tool_switch(Self::start_insert_3d) {
            return;
        }
        if self.mode == Mode::Insert3D {
            self.finish_creation_for_tool_switch();
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current operation before Draw 3D");
            return;
        }
        self.deactivate_sidebar_tools();
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        self.draw3d_tools_open = true;
        self.insert_3d_ctx = Some(Insert3DCtx {
            params: None,
            preview_shape: None,
            editing_body: None,
            stage: Draw3DPlacementStage::ChoosePrimitive,
            anchor: None,
            parameters_applied: false,
            direct_drag: None,
        });
        self.mode = Mode::Insert3D;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.set_hint("Draw 3D: choose Box, Cylinder, Sphere, Cone, or Torus");
        self.bump_scene();
    }

    pub fn choose_insert_3d_primitive(&mut self, kind: Primitive3DKind) {
        if self.mode != Mode::Insert3D {
            return;
        }
        if self.insert_3d_ctx.as_ref().is_some_and(|ctx| {
            matches!(
                ctx.stage,
                Draw3DPlacementStage::Sizing | Draw3DPlacementStage::Precision
            )
        }) {
            if !self.finish_creation_for_tool_switch() {
                return;
            }
            self.start_insert_3d();
        }
        let old_preview = self
            .insert_3d_ctx
            .as_mut()
            .and_then(|ctx| ctx.preview_shape.take());
        if let Some(shape) = old_preview {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        if let Some(ctx) = self.insert_3d_ctx.as_mut() {
            ctx.editing_body = None;
            ctx.params = Some(Primitive3DParams::defaults(kind));
            ctx.stage = Draw3DPlacementStage::AwaitBase;
            ctx.anchor = None;
            ctx.parameters_applied = false;
            ctx.direct_drag = None;
        }
        if kind == Primitive3DKind::Sphere {
            self.set_hint(self.settings.language.text(
                "Draw 3D Sphere: click the workspace to set its center",
                "绘制3D球体：单击工作区确定球心",
            ));
        } else {
            self.set_hint(format!(
                "Draw 3D {}: click the workspace to set its base point",
                kind.name()
            ));
        }
        self.bump_scene();
    }

    pub fn update_insert_3d_params(&mut self, params: Primitive3DParams) {
        if self.mode != Mode::Insert3D {
            return;
        }
        let old_preview = self.insert_3d_ctx.as_mut().and_then(|ctx| {
            ctx.params = Some(params);
            ctx.parameters_applied = false;
            ctx.preview_shape.take()
        });
        if let Some(shape) = old_preview {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        // Interactive sphere edits must not invoke the native revolve/sewing
        // pipeline for every keystroke. Apply replaces this display-only mesh
        // with the validated exact shape before Done can commit it.
        let preview = if params.kind == Primitive3DKind::Sphere {
            sphere_preview_mesh(&params).map(|mesh| self.kernel.insert_mesh(mesh))
        } else {
            Self::build_insert_3d_shape(&mut self.kernel, &params)
        };
        match preview {
            Ok(shape) => {
                if let Some(ctx) = self.insert_3d_ctx.as_mut() {
                    ctx.preview_shape = Some(shape);
                }
                self.set_hint(format!(
                    "Draw 3D {} preview · adjust parameters, then Done or Exit Draw 3D",
                    params.kind.name()
                ));
            }
            Err(error) => {
                self.set_hint(format!("Draw 3D: {error}"));
                self.toast(format!("Cannot create {}: {error}", params.kind.name()));
            }
        }
        self.bump_scene();
    }

    fn draw_3d_point_on_ground(&self, pos: egui::Pos2, rect: egui::Rect) -> [f64; 3] {
        let plane = DatumPlane::Top.def();
        let local = self.screen_to_plane(pos, rect, &plane);
        let mut world = plane.to_world(local[0], local[1]);
        if self.snap_enabled && self.snap_step.is_finite() && self.snap_step > 1e-9 {
            world[0] = (world[0] / self.snap_step).round() * self.snap_step;
            world[2] = (world[2] / self.snap_step).round() * self.snap_step;
        }
        world
    }

    fn resize_insert_3d_from_pointer(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        let Some((params, anchor)) = self.insert_3d_ctx.as_ref().and_then(|ctx| {
            (ctx.stage == Draw3DPlacementStage::Sizing).then_some((ctx.params?, ctx.anchor?))
        }) else {
            return;
        };
        let current = self.draw_3d_point_on_ground(pos, rect);
        let minimum = (self.snap_step.abs() * 0.1).clamp(0.1, 1_000.0);
        let resized = draw_3d_drag_params(params, anchor, current, minimum);
        if resized != params {
            self.update_insert_3d_params(resized);
        }
    }

    fn draw_3d_placement_click(&mut self, pos: egui::Pos2, rect: egui::Rect) -> bool {
        let stage = self
            .insert_3d_ctx
            .as_ref()
            .map(|ctx| ctx.stage)
            .unwrap_or(Draw3DPlacementStage::ChoosePrimitive);
        match stage {
            Draw3DPlacementStage::AwaitBase => {
                let point = self.draw_3d_point_on_ground(pos, rect);
                let Some(mut params) = self.insert_3d_ctx.as_ref().and_then(|ctx| ctx.params)
                else {
                    return false;
                };
                set_draw_3d_reference_point(&mut params, point);
                if let Some(ctx) = self.insert_3d_ctx.as_mut() {
                    ctx.anchor = Some(point);
                    ctx.stage = Draw3DPlacementStage::Sizing;
                }
                self.update_insert_3d_params(params);
                self.set_hint("Draw 3D: move the pointer to size the object, then click again");
                true
            }
            Draw3DPlacementStage::Sizing => {
                self.resize_insert_3d_from_pointer(pos, rect);
                if let Some(ctx) = self.insert_3d_ctx.as_mut() {
                    ctx.stage = Draw3DPlacementStage::Precision;
                    ctx.parameters_applied = false;
                    ctx.direct_drag = None;
                }
                self.set_hint("Draw 3D: refine parameters in Properties, click Apply, then Done");
                self.bump_scene();
                true
            }
            _ => false,
        }
    }

    fn world_to_viewport(&self, point: glam::Vec3, rect: egui::Rect) -> Option<egui::Pos2> {
        let view_proj = self
            .camera
            .proj_matrix(rect.width() / rect.height().max(1.0))
            * self.camera.view_matrix();
        let clip = view_proj * point.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        ))
    }

    fn draw_3d_axis_pointer_delta(
        &self,
        point: [f64; 3],
        axis: glam::DVec3,
        pointer_delta: egui::Vec2,
        rect: egui::Rect,
    ) -> f64 {
        let world = glam::Vec3::from_array(point.map(|value| value as f32));
        let axis = axis.normalize_or_zero();
        if axis.length_squared() <= 1e-12 {
            return 0.0;
        }
        let world_per_pixel = self.world_per_pixel_at(world).max(1e-6);
        let probe = (world_per_pixel * 48.0).max(0.01);
        let Some(a) = self.world_to_viewport(world, rect) else {
            return -(pointer_delta.y * world_per_pixel) as f64;
        };
        let Some(b) = self.world_to_viewport(
            world + glam::Vec3::from_array(axis.to_array().map(|value| value as f32)) * probe,
            rect,
        ) else {
            return -(pointer_delta.y * world_per_pixel) as f64;
        };
        let screen_axis = b - a;
        let pixels = screen_axis.length();
        if pixels <= 1e-3 {
            -(pointer_delta.y * world_per_pixel) as f64
        } else {
            (pointer_delta.dot(screen_axis / pixels) * probe / pixels) as f64
        }
    }

    fn begin_draw_3d_direct_drag(&mut self, pos: egui::Pos2, rect: egui::Rect) -> bool {
        let Some((params, preview_shape)) = self.insert_3d_ctx.as_ref().and_then(|ctx| {
            (ctx.stage == Draw3DPlacementStage::Precision)
                .then_some((ctx.params?, ctx.preview_shape?))
        }) else {
            return false;
        };

        let mut closest_handle: Option<(Draw3DDragKind, f32)> = None;
        for (kind, point) in draw_3d_control_points(&params) {
            let world = glam::Vec3::from_array(point.map(|value| value as f32));
            let Some(screen) = self.world_to_viewport(world, rect) else {
                continue;
            };
            let distance = screen.distance(pos);
            if distance <= 12.0
                && closest_handle
                    .as_ref()
                    .map(|(_, best)| distance < *best)
                    .unwrap_or(true)
            {
                closest_handle = Some((kind, distance));
            }
        }

        let kind = if let Some((kind, _)) = closest_handle {
            kind
        } else {
            // This is a translucent preview, not a document-body depth pick.
            // Test the projected triangles the user actually sees so preview
            // dragging follows its visible footprint in either projection.
            let hit_preview = self
                .kernel
                .tessellate(preview_shape)
                .ok()
                .is_some_and(|mesh| {
                    mesh.indices.chunks_exact(3).any(|triangle| {
                        let projected: Option<Vec<_>> = triangle
                            .iter()
                            .map(|&index| {
                                self.world_to_viewport(
                                    glam::Vec3::from_array(mesh.positions[index as usize]),
                                    rect,
                                )
                            })
                            .collect();
                        let Some(points) = projected else {
                            return false;
                        };
                        let cross = |a: egui::Vec2, b: egui::Vec2| a.x * b.y - a.y * b.x;
                        let area = cross(points[1] - points[0], points[2] - points[0]);
                        if area.abs() < 1e-4 {
                            return false;
                        }
                        let u = cross(points[1] - pos, points[2] - pos) / area;
                        let v = cross(points[2] - pos, points[0] - pos) / area;
                        u >= -1e-5 && v >= -1e-5 && u + v <= 1.0 + 1e-5
                    })
                });
            if !hit_preview {
                return false;
            }
            Draw3DDragKind::Move
        };

        if let Some(ctx) = self.insert_3d_ctx.as_mut() {
            ctx.direct_drag = Some(Draw3DDirectDrag {
                kind,
                pointer_start: pos,
                params_start: params,
            });
        }
        self.set_hint(match kind {
            Draw3DDragKind::Move => "Draw 3D: drag the preview to move it",
            _ => "Draw 3D: drag the red key point to resize",
        });
        self.bump_scene();
        true
    }

    fn update_draw_3d_direct_drag(&mut self, pos: egui::Pos2, rect: egui::Rect) -> bool {
        let Some(drag) = self.insert_3d_ctx.as_ref().and_then(|ctx| ctx.direct_drag) else {
            return false;
        };
        let frame = draw_3d_base_frame(&drag.params_start);
        let start_local = self.screen_to_plane(drag.pointer_start, rect, &frame);
        let current_local = self.screen_to_plane(pos, rect, &frame);
        let mut planar_delta = [
            current_local[0] - start_local[0],
            current_local[1] - start_local[1],
        ];
        let minimum = (self.snap_step.abs() * 0.1).clamp(0.1, 1_000.0);
        let mut params = drag.params_start;
        if drag.kind == Draw3DDragKind::Move {
            let delta = frame.x_axis() * planar_delta[0] + frame.y_axis() * planar_delta[1];
            params.position =
                (glam::DVec3::from_array(drag.params_start.position) + delta).to_array();
        } else {
            let handle = draw_3d_control_points(&drag.params_start)
                .into_iter()
                .find_map(|(kind, point)| (kind == drag.kind).then_some(point))
                .unwrap_or(drag.params_start.position);
            let height_delta = self.draw_3d_axis_pointer_delta(
                handle,
                frame.normal(),
                pos - drag.pointer_start,
                rect,
            );
            if drag.kind != Draw3DDragKind::PlanarSize {
                let axis = if drag.kind == Draw3DDragKind::Depth {
                    frame.y_axis()
                } else {
                    frame.x_axis()
                };
                let delta =
                    self.draw_3d_axis_pointer_delta(handle, axis, pos - drag.pointer_start, rect);
                if drag.kind == Draw3DDragKind::Depth {
                    planar_delta[1] = delta;
                } else {
                    planar_delta[0] = delta;
                }
            }
            params = draw_3d_dragged_params(
                drag.params_start,
                drag.kind,
                planar_delta,
                height_delta,
                minimum,
            );
        }
        let changed = self.insert_3d_ctx.as_ref().and_then(|ctx| ctx.params) != Some(params);
        if changed {
            self.update_insert_3d_params(params);
        }
        true
    }

    fn finish_draw_3d_direct_drag(&mut self) -> bool {
        let finished = self
            .insert_3d_ctx
            .as_mut()
            .and_then(|ctx| ctx.direct_drag.take())
            .is_some();
        if finished {
            self.set_hint("Draw 3D: drag again or refine Properties, then click Apply and Done");
            self.bump_scene();
        }
        finished
    }

    pub fn apply_insert_3d_params(&mut self) -> bool {
        let ready = self.insert_3d_ctx.as_ref().is_some_and(|ctx| {
            ctx.stage == Draw3DPlacementStage::Precision
                && ctx.params.is_some()
                && ctx.preview_shape.is_some()
        });
        if !ready {
            self.toast("Place and size the Draw 3D object first");
            return false;
        }
        let params = self.insert_3d_ctx.as_ref().unwrap().params.unwrap();
        if params.kind == Primitive3DKind::Sphere
            && !self.insert_3d_ctx.as_ref().unwrap().parameters_applied
        {
            match Self::build_insert_3d_shape(&mut self.kernel, &params) {
                Ok(shape) => {
                    if let Some(old) = self
                        .insert_3d_ctx
                        .as_mut()
                        .unwrap()
                        .preview_shape
                        .replace(shape)
                    {
                        self.kernel.delete(old);
                        self.mesh_cache.remove(&old);
                    }
                }
                Err(error) => {
                    self.toast(format!("Cannot apply Sphere: {error}"));
                    return false;
                }
            }
        }
        if let Some(ctx) = self.insert_3d_ctx.as_mut() {
            ctx.parameters_applied = true;
        }
        self.set_hint(
            "Draw 3D parameters applied. Done keeps drawing open. Esc commits and exits.",
        );
        self.bump_scene();
        true
    }

    fn build_insert_3d_shape(
        kernel: &mut TruckKernel,
        params: &Primitive3DParams,
    ) -> std::result::Result<ShapeId, String> {
        if params.position.iter().any(|value| !value.is_finite()) {
            return Err("position must be finite".to_string());
        }
        let mut top = params
            .frame
            .clone()
            .unwrap_or_else(|| DatumPlane::Top.def());
        top.origin = params.position;
        let mut front = params
            .frame
            .clone()
            .unwrap_or_else(|| DatumPlane::Front.def());
        front.origin = params.position;
        let axis = Axis2D {
            point: [0.0, 0.0],
            dir: [0.0, 1.0],
        };
        let positive = |value: f64, label: &str| -> std::result::Result<f64, String> {
            if value.is_finite() && value > 1e-4 {
                Ok(value)
            } else {
                Err(format!("{label} must be greater than zero"))
            }
        };

        let result = match params.kind {
            Primitive3DKind::Box => {
                let width = positive(params.width, "width")?;
                let depth = positive(params.depth, "depth")?;
                let height = positive(params.height, "height")?;
                let region = Region2D::new(vec![
                    [-width * 0.5, -depth * 0.5],
                    [width * 0.5, -depth * 0.5],
                    [width * 0.5, depth * 0.5],
                    [-width * 0.5, depth * 0.5],
                ]);
                kernel.extrude_region(&region, &top, height)
            }
            Primitive3DKind::Cylinder => {
                let radius = positive(params.radius, "radius")?;
                let height = positive(params.height, "height")?;
                let region = Region2D::new(insert_3d_circle([0.0, 0.0], radius, 96));
                kernel.extrude_region(&region, &top, height)
            }
            Primitive3DKind::Sphere => {
                let radius = positive(params.radius, "radius")?;
                let steps = 64;
                let mut profile = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let angle = -std::f64::consts::FRAC_PI_2
                        + std::f64::consts::PI * i as f64 / steps as f64;
                    profile.push([radius * angle.cos(), radius + radius * angle.sin()]);
                }
                kernel.revolve_region(&Region2D::new(profile), &front, &axis, 360.0)
            }
            Primitive3DKind::Cone => {
                let radius = positive(params.radius, "bottom radius")?;
                let height = positive(params.height, "height")?;
                if !params.top_radius.is_finite() || params.top_radius < 0.0 {
                    return Err("top radius must be zero or greater".to_string());
                }
                let mut profile = vec![[0.0, 0.0], [radius, 0.0]];
                if params.top_radius > 1e-6 {
                    profile.push([params.top_radius, height]);
                }
                profile.push([0.0, height]);
                kernel.revolve_region(&Region2D::new(profile), &front, &axis, 360.0)
            }
            Primitive3DKind::Torus => {
                let major = positive(params.major_radius, "major radius")?;
                let minor = positive(params.minor_radius, "minor radius")?;
                if major <= minor {
                    return Err("major radius must be greater than minor radius".to_string());
                }
                let profile = insert_3d_circle([major, minor], minor, 64);
                kernel.revolve_region(&Region2D::new(profile), &front, &axis, 360.0)
            }
        };
        result.map_err(|error| error.to_string())
    }

    /// Draw 3D 模式下从 Objects 或工作区重新进入既有基础体的参数编辑。
    pub fn edit_draw_3d_body(&mut self, id: BodyId) {
        if self.mode != Mode::Insert3D {
            return;
        }
        let Some(params) = self
            .doc
            .body(id)
            .and_then(|body| draw_3d_params_from_feature(&body.feature))
        else {
            self.toast(
                "Draw 3D recognizes its own objects and simple Box/Cylinder/Sphere/Cone/Frustum Extrude or Revolve features",
            );
            return;
        };
        if let Some(ctx) = self.insert_3d_ctx.as_mut() {
            ctx.editing_body = Some(id);
            ctx.stage = Draw3DPlacementStage::Precision;
            ctx.anchor = None;
            ctx.parameters_applied = false;
            ctx.direct_drag = None;
        }
        self.selected = vec![id];
        self.clear_region_selection();
        self.selected_edge = None;
        self.face_pick = None;
        self.update_insert_3d_params(params);
        let name = self
            .doc
            .body(id)
            .map(|body| body.name.clone())
            .unwrap_or_else(|| format!("Object {id}"));
        self.toast(format!("Editing {name}"));
    }

    pub fn done_insert_3d(&mut self) -> bool {
        if self.mode != Mode::Insert3D {
            return false;
        }
        if !self
            .insert_3d_ctx
            .as_ref()
            .is_some_and(|ctx| ctx.parameters_applied)
        {
            self.toast("Click Apply before Done");
            return false;
        }
        let ready = self
            .insert_3d_ctx
            .as_ref()
            .and_then(|ctx| Some((ctx.params?, ctx.preview_shape?, ctx.editing_body)));
        let Some((params, shape, editing_body)) = ready else {
            self.toast("Choose a 3D object with valid parameters first");
            return false;
        };
        if let Some(ctx) = self.insert_3d_ctx.as_mut() {
            ctx.params = None;
            ctx.preview_shape = None;
            ctx.editing_body = None;
            ctx.stage = Draw3DPlacementStage::ChoosePrimitive;
            ctx.anchor = None;
            ctx.parameters_applied = false;
            ctx.direct_drag = None;
        }

        let id = if let Some(id) = editing_body {
            if self.doc.body(id).is_none() {
                self.kernel.delete(shape);
                self.mesh_cache.remove(&shape);
                self.toast("The object being edited no longer exists");
                return false;
            }
            self.begin_document_change();
            if let Some(body) = self.doc.body_mut(id) {
                body.shape = Some(shape);
                body.feature = Feature::Primitive3D { params };
            }
            self.record_history(format!("Update Draw 3D {} object {id}", params.kind.name()));
            self.toast(format!("{} updated", params.kind.name()));
            id
        } else {
            let id = self.doc.alloc_id();
            let color_index = self.doc.bodies.len() + 1;
            self.begin_document_change();
            self.doc.add_body(Body {
                id,
                name: format!("{} {id}", params.kind.name()),
                kind: BodyKind::Solid,
                feature: Feature::Primitive3D { params },
                shape: Some(shape),
                material: Material {
                    color: palette_color(color_index),
                    ..Material::default()
                },
                visible: true,
            });
            self.record_history(format!("Draw 3D {} as object {id}", params.kind.name()));
            self.toast(format!("{} added", params.kind.name()));
            id
        };
        self.selected = vec![id];
        self.clear_region_selection();
        self.mesh_cache.remove(&shape);
        self.set_hint("Draw 3D: choose another object, reselect one to edit, or Exit Draw 3D");
        self.bump_scene();
        true
    }

    pub fn exit_insert_3d(&mut self) {
        if self.mode != Mode::Insert3D {
            return;
        }
        let has_current = self
            .insert_3d_ctx
            .as_ref()
            .map(|ctx| {
                // Choosing a primitive does not place an object. Only a
                // sized/precise draft or an existing preview can be applied.
                matches!(
                    ctx.stage,
                    Draw3DPlacementStage::Sizing | Draw3DPlacementStage::Precision
                ) && (ctx.params.is_some() || ctx.preview_shape.is_some())
            })
            .unwrap_or(false);
        if has_current {
            if let Some(ctx) = self.insert_3d_ctx.as_mut() {
                ctx.stage = Draw3DPlacementStage::Precision;
            }
            if !self.apply_insert_3d_params() || !self.done_insert_3d() {
                return;
            }
        }
        self.insert_3d_ctx = None;
        self.mode = Mode::Idle;
        self.collapse_tool_menus();
        self.set_hint("Draw 3D complete");
        self.bump_scene();
    }

    pub fn cancel_insert_3d(&mut self) {
        if let Some(shape) = self.insert_3d_ctx.take().and_then(|ctx| ctx.preview_shape) {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        self.mode = Mode::Idle;
        self.set_hint("Draw 3D cancelled");
        self.bump_scene();
    }

    pub fn escape_insert_3d(&mut self) {
        if self
            .insert_3d_ctx
            .as_ref()
            .is_some_and(|ctx| ctx.parameters_applied)
        {
            self.exit_insert_3d();
        } else {
            self.cancel_insert_3d();
            self.collapse_tool_menus();
        }
    }

    // ================= 草图 =================

    pub fn begin_plane_pick(&mut self) {
        if self.queue_tool_switch(Self::begin_plane_pick) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.sketch.is_some() {
            self.toast("Finish or exit the current sketch first");
            return;
        }
        if !matches!(self.mode, Mode::Idle | Mode::PickPlane) {
            self.toast("Finish or cancel the current operation first");
            return;
        }
        // Sketch is a top-level tool. Entering it must always leave edge
        // treatment, including the direct "sketch on selected face" path.
        self.deactivate_sidebar_tools();
        // 已选中 3D 表面时：直接在该面上启动 sketch（跳过 XY/YZ/ZX 平面选择界面）
        self.sketch_tools_open = true;
        if let Some(face) = self
            .face_pick
            .clone()
            .filter(|face| self.selected.contains(&face.body_id))
        {
            self.start_sketch_on_face(&face);
            return;
        }
        self.clear_selection();
        self.sketch_plane_equation = [0.0, 0.0, 1.0, 0.0];
        self.sketch_plane_equation_enabled = false;
        self.mode = Mode::PickPlane;
        self.set_hint("Choose a sketch plane in the panel or click one in the viewport: XY / YZ / ZX · Esc to cancel");
        // 世界原点 (0,0,0) 对准视口正中央：强制透视 + 斜上方 iso 视角
        self.camera.ortho = false;
        self.camera.target = glam::Vec3::ZERO;
        let dir = glam::Vec3::new(1.0, 0.82, 1.0).normalize();
        self.camera.eye = dir * 520.0;
        self.camera.up = glam::Vec3::Y;
        self.bump_scene();
    }

    /// 在指定基准面上开始草图（面板按钮 / 视口点击共用入口）。
    pub fn start_sketch_on(&mut self, plane: DatumPlane) {
        let def = plane.def();
        self.toast(format!("Sketch on {} plane", plane.name()));
        self.start_sketch(def);
    }

    /// Update the optional custom plane shown in the initial Sketch picker.
    pub fn set_sketch_plane_equation(&mut self, enabled: bool, equation: [f64; 4]) {
        if self.mode != Mode::PickPlane {
            return;
        }
        self.sketch_plane_equation_enabled = enabled;
        self.sketch_plane_equation = equation;
        if enabled {
            self.set_hint(if plane_from_equation(equation).is_ok() {
                "Custom sketch plane ready · click Start Sketch"
            } else {
                "Plane equation needs at least one non-zero finite coefficient"
            });
        } else {
            self.set_hint("Choose a sketch plane in the panel or click XY / YZ / ZX");
        }
        self.bump_scene();
    }

    pub fn sketch_plane_equation_is_valid(&self) -> bool {
        self.sketch_plane_equation_enabled
            && plane_from_equation(self.sketch_plane_equation).is_ok()
    }

    pub fn start_sketch_on_equation(&mut self) {
        if self.mode != Mode::PickPlane || !self.sketch_plane_equation_enabled {
            return;
        }
        match plane_from_equation(self.sketch_plane_equation) {
            Ok(plane) => {
                self.toast("Sketch on custom equation plane");
                self.start_sketch_with_coordinate_display(plane, true);
            }
            Err(error) => self.toast(error),
        }
    }

    /// 在某个基准面上开始草图。
    pub fn start_sketch(&mut self, plane: PlaneDef) {
        self.start_sketch_with_coordinate_display(plane, false);
    }

    fn start_sketch_with_coordinate_display(
        &mut self,
        plane: PlaneDef,
        world_xyz_coordinates: bool,
    ) {
        // Sketch 与所有 3D feature tools 互斥. Keep this cleanup here too,
        // because callers may enter directly from a selected surface.
        self.deactivate_sidebar_tools();
        self.sketch_edit_body = None;
        self.sketch_edit_original_doc = None;
        self.sketch_edit_original_selection.clear();
        self.sketch_on_body = None; // 基准面草图：无目标体
        self.saved_camera = Some(self.camera.clone());
        self.mode = Mode::Sketch;
        self.sketch_tools_open = true;
        // 正视平面（先取数据再 move）
        let n = plane.normal();
        let o = plane.origin();
        let x = plane.x_axis();
        let y = plane.y_axis();
        self.sketch = Some(SketchSession::new_with_coordinate_display(
            plane,
            Vec::new(),
            world_xyz_coordinates,
        ));
        self.sketch_undo_stack.clear();
        self.sketch_redo_stack.clear();
        self.sketch_history_current = Some(SketchSnapshot::default());
        self.camera
            .align_to_plane(v3(o), v3(n), v3(x), v3(y), 170.0);
        self.selected.clear();
        self.clear_region_selection();
        self.set_hint("Sketch mode: pick entities or draw with L/A/C/R/G - press Done to finish");
        self.toast("Sketch mode: draw a closed profile, press Done to finish");
        self.bump_scene();
    }

    /// 编辑已有 sketch body。
    pub fn edit_sketch(&mut self, body_id: BodyId) {
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current operation first");
            return;
        }
        let Some(sk) = self
            .doc
            .body(body_id)
            .and_then(|body| body.sketch().cloned())
        else {
            return;
        };
        // Sketch 与 gizmo/Align/Array 互斥
        self.deactivate_sidebar_tools();
        self.sketch_edit_body = Some(body_id);
        self.sketch_edit_original_doc = Some(self.doc.clone());
        self.sketch_edit_original_selection = self.selected.clone();
        self.saved_camera = Some(self.camera.clone());
        self.mode = Mode::Sketch;
        self.sketch_tools_open = true;
        self.sketch_on_body = sk.on_body;
        let mut session = SketchSession::new_with_coordinate_display(
            sk.plane,
            sk.entities,
            sk.world_xyz_coordinates,
        );
        session.vertex_locks = sk.vertex_locks;
        self.sketch_undo_stack.clear();
        self.sketch_redo_stack.clear();
        self.sketch_history_current = Some(session.snapshot());
        self.sketch = Some(session);
        let n = sk.plane.normal();
        let o = sk.plane.origin();
        self.camera.align_to_plane(
            v3(o),
            v3(n),
            v3(sk.plane.x_axis()),
            v3(sk.plane.y_axis()),
            170.0,
        );
        self.selected = vec![body_id];
        self.set_hint("Editing sketch: draw or pick entities, Enter to rebuild");
        self.bump_scene();
    }

    /// 完成草图：作为独立 sketch body 保留在平面上。
    pub fn finish_sketch(&mut self) {
        // Done can happen before the end-of-frame observer. Capture the final
        // drawing/edit state before Sketch mode is closed.
        self.sync_sketch_history();
        let Some(sk) = &mut self.sketch else { return };
        sk.finish_chain();
        if sk.entities.is_empty() && self.sketch_edit_body.is_none() {
            self.toast("Sketch is empty");
            return;
        }
        let plane = sk.plane.clone();
        let snapshot = sk.snapshot();
        let selected_texts: Vec<_> = sk
            .selected_entities()
            .into_iter()
            .filter_map(|i| snapshot.entities.get(i))
            .filter(|e| matches!(e, SketchEntity::Path { text: Some(_), .. }))
            .cloned()
            .collect();
        let world_xyz_coordinates = sk.world_xyz_coordinates;
        let updated_sketch = SketchFeature {
            vertex_locks: snapshot.vertex_locks.clone(),
            plane,
            entities: snapshot.entities.clone(),
            world_xyz_coordinates,
            on_body: self.sketch_on_body,
        };

        // 更新编辑中的 body，或新建 sketch body。During Edit Sketch the
        // current document may already contain live rebuilt shapes. Build the
        // final result first, restore the pre-edit document, then commit once
        // so the entire session is one normal Undo step.
        if let Some(id) = self.sketch_edit_body {
            let rebuilt = match self.rebuild_sketch_dependents(id, &updated_sketch) {
                Ok(rebuilt) => rebuilt,
                Err(error) => {
                    self.toast(format!("Sketch rebuild failed: {error}"));
                    self.set_hint("Fix the sketch profile, then try Done again");
                    return;
                }
            };
            if let Some(original) = self.sketch_edit_original_doc.take() {
                self.doc = original;
            }
            let original_selection = std::mem::take(&mut self.sketch_edit_original_selection);
            if !original_selection.is_empty() {
                self.selected = original_selection;
            }
            self.begin_document_change();
            if let Some(body) = self.doc.body_mut(id) {
                body.feature = Feature::Sketch(updated_sketch);
            }
            for (body_id, shape, feature) in rebuilt {
                if let Some(body) = self.doc.body_mut(body_id) {
                    body.shape = Some(shape);
                    body.feature = feature;
                }
            }
            self.sketch = None;
            self.sketch_edit_body = None;
            self.restore_view();
            self.mode = Mode::Idle;
            self.selected = vec![id];
            self.mesh_cache.clear();
            self.section_cache.clear();
            self.collect_kernel_garbage();
            self.bump_scene();
            let regions = self.regions_of(id);
            self.select_text_region_groups(id, &selected_texts, &regions);
            self.toast("Sketch and dependent bodies updated");
            self.record_history(format!("Edit sketch {} and rebuild dependents", id));
            return;
        }
        // Convert the live Sketch timeline into normal Document snapshots.
        // After Done/Exit, Undo therefore walks the drawing states of this one
        // Sketch object instead of removing the entire object in one step.
        let mut timeline = self.sketch_undo_stack.clone();
        if timeline.last() != Some(&snapshot) {
            timeline.push(snapshot);
        }
        timeline.dedup();
        timeline.retain(|state| !state.entities.is_empty());
        let drawing_steps = timeline.len();

        self.sketch = None;
        self.sketch_edit_body = None;
        self.sketch_edit_original_doc = None;
        self.sketch_edit_original_selection.clear();
        self.restore_view();
        self.mode = Mode::Idle;
        let mut sketch_id = None;
        for state in timeline {
            self.begin_document_change();
            let feature = SketchFeature {
                entities: state.entities,
                vertex_locks: state.vertex_locks,
                ..updated_sketch.clone()
            };
            let id = if let Some(id) = sketch_id {
                if let Some(body) = self.doc.body_mut(id) {
                    body.feature = Feature::Sketch(feature);
                }
                id
            } else {
                let id = self.doc.alloc_id();
                self.doc.add_body(Body {
                    id,
                    name: format!("Sketch {}", id),
                    kind: BodyKind::Sketch,
                    feature: Feature::Sketch(feature),
                    shape: None,
                    material: Material::default(),
                    visible: true,
                });
                sketch_id = Some(id);
                id
            };
            self.selected = vec![id];
        }
        let Some(id) = sketch_id else {
            // entities was checked above, so this is only a defensive guard.
            self.toast("Sketch has no committed drawing steps");
            return;
        };
        self.clear_region_selection();
        self.set_hint(
            "Sketch kept. Double-click a closed region, then press E for Extrude or P for Pocket.",
        );
        self.toast("Sketch kept on the plane");
        self.record_history(format!(
            "Sketch created ({id}, {drawing_steps} drawing steps)"
        ));
        self.bump_scene();
        let regions = self.regions_of(id);
        self.select_text_region_groups(id, &selected_texts, &regions);
    }

    /// Rebuild the complete supported dependency graph before committing an
    /// edited Sketch. Direct Extrude/Revolve nodes are followed by structured
    /// Boolean results and Chamfer/Fillet nodes in dependency order.
    ///
    /// New shapes are prepared transactionally: a failure deletes all
    /// temporary results and leaves both the document and edit session intact.
    fn rebuild_sketch_dependents(
        &mut self,
        source_id: BodyId,
        sketch: &SketchFeature,
    ) -> Result<Vec<(BodyId, ShapeId, Feature)>, String> {
        let regions = s33d_model::compute_regions(&sketch.entities);
        let dependents: Vec<(BodyId, Feature)> = self
            .doc
            .bodies
            .iter()
            .filter_map(|body| match &body.feature {
                Feature::Extrude {
                    source_sketch: Some(id),
                    ..
                }
                | Feature::Revolve {
                    source_sketch: Some(id),
                    ..
                } if *id == source_id => Some((body.id, body.feature.clone())),
                _ => None,
            })
            .collect();

        let mut rebuilt = Vec::with_capacity(dependents.len());
        for (body_id, feature) in dependents {
            let result = match feature {
                Feature::Extrude {
                    params,
                    region_indices,
                    ..
                } => {
                    let indices = if region_indices.is_empty() {
                        if regions.len() == 1 {
                            vec![0]
                        } else {
                            return self.discard_rebuilt_shapes(
                                rebuilt,
                                format!(
                                    "Body {body_id} has legacy profile data and the edited sketch now has {} regions",
                                    regions.len()
                                ),
                            );
                        }
                    } else {
                        region_indices
                    };
                    let selected: Result<Vec<_>, _> = indices
                        .iter()
                        .map(|&index| {
                            regions.get(index).cloned().ok_or_else(|| {
                                format!(
                                    "Body {body_id} needs sketch region {index}, but only {} remain",
                                    regions.len()
                                )
                            })
                        })
                        .collect();
                    let selected = match selected {
                        Ok(selected) => selected,
                        Err(error) => return self.discard_rebuilt_shapes(rebuilt, error),
                    };
                    build_extrude_shape(&mut self.kernel, sketch, &selected, &params).map(|shape| {
                        (
                            shape,
                            Feature::Extrude {
                                sketch: sketch.clone(),
                                params,
                                source_sketch: Some(source_id),
                                pocket_source_sketches: Vec::new(),
                                region_indices: indices,
                            },
                        )
                    })
                }
                Feature::Revolve {
                    params,
                    region_index,
                    ..
                } => {
                    let index = match region_index {
                        Some(index) => index,
                        None if regions.len() == 1 => 0,
                        None => {
                            return self.discard_rebuilt_shapes(
                                rebuilt,
                                format!(
                                    "Body {body_id} has legacy profile data and the edited sketch now has {} regions",
                                    regions.len()
                                ),
                            );
                        }
                    };
                    let Some(region) = regions.get(index) else {
                        return self.discard_rebuilt_shapes(
                            rebuilt,
                            format!(
                                "Body {body_id} needs sketch region {index}, but only {} remain",
                                regions.len()
                            ),
                        );
                    };
                    let axis = Axis2D {
                        point: params.axis_point,
                        dir: params.axis_dir,
                    };
                    if let Err(error) = s33d_model::triangulate_region(&region.outer, &region.holes)
                    {
                        return self.discard_rebuilt_shapes(
                            rebuilt,
                            format!("Revolve region {index}: {error}"),
                        );
                    }
                    self.kernel
                        .revolve_region(region, &sketch.plane, &axis, params.angle_deg)
                        .map(|shape| {
                            (
                                shape,
                                Feature::Revolve {
                                    sketch: sketch.clone(),
                                    params,
                                    source_sketch: Some(source_id),
                                    region_index: Some(index),
                                },
                            )
                        })
                        .map_err(|error| error.to_string())
                }
                _ => unreachable!(),
            };

            match result {
                Ok((shape, feature)) => rebuilt.push((body_id, shape, feature)),
                Err(error) => return self.discard_rebuilt_shapes(rebuilt, error),
            }
        }
        let mut rebuilt_shapes: HashMap<BodyId, ShapeId> = rebuilt
            .iter()
            .map(|(body_id, shape, _)| (*body_id, *shape))
            .collect();
        let mut affected: HashSet<BodyId> = rebuilt_shapes.keys().copied().collect();

        // First determine the full transitive closure. This lets the evaluator
        // wait for every affected parent instead of accidentally using its old
        // shape when document order is not topological.
        loop {
            let mut added = false;
            for body in &self.doc.bodies {
                if affected.contains(&body.id) {
                    continue;
                }
                let dependencies = rebuild_feature_dependencies(&body.feature);
                if dependencies
                    .iter()
                    .any(|dependency| affected.contains(dependency))
                {
                    affected.insert(body.id);
                    added = true;
                }
            }
            if !added {
                break;
            }
        }

        let mut pending: HashSet<BodyId> = affected
            .iter()
            .copied()
            .filter(|body_id| !rebuilt_shapes.contains_key(body_id))
            .collect();
        while !pending.is_empty() {
            let candidates: Vec<(BodyId, Feature)> = self
                .doc
                .bodies
                .iter()
                .filter(|body| pending.contains(&body.id))
                .map(|body| (body.id, body.feature.clone()))
                .collect();
            let mut progressed = false;

            for (body_id, feature) in candidates {
                let dependencies = rebuild_feature_dependencies(&feature);
                if dependencies.iter().any(|dependency| {
                    affected.contains(dependency) && !rebuilt_shapes.contains_key(dependency)
                }) {
                    continue;
                }

                let result = if let Some((op, operands)) = derived_boolean_recipe(&feature) {
                    self.rebuild_boolean_operation(op, &operands, &rebuilt_shapes)
                        .map(|shape| {
                            let mut rebuilt_feature = feature.clone();
                            if let Feature::Derived { operation, .. } = &mut rebuilt_feature {
                                *operation = Some(DerivedOperation::Boolean { op, operands });
                            }
                            (shape, rebuilt_feature)
                        })
                } else if let Feature::Derived {
                    operation: Some(operation),
                    ..
                } = &feature
                {
                    self.rebuild_derived_operation(operation, &rebuilt_shapes)
                        .map(|shape| (shape, feature.clone()))
                } else {
                    match &feature {
                        Feature::Chamfer {
                            target,
                            base,
                            edges,
                            reference_bounds,
                            chamfer,
                            fillet,
                        } => {
                            let base_id = base.unwrap_or(*target);
                            let base_shape = rebuilt_shapes
                                .get(&base_id)
                                .copied()
                                .or_else(|| self.doc.body(base_id).and_then(|body| body.shape))
                                .ok_or_else(|| format!("Base object {base_id} is missing"));
                            base_shape.and_then(|base_shape| {
                                let mesh = self
                                    .kernel
                                    .tessellate(base_shape)
                                    .map_err(|error| error.to_string())?;
                                let stored = stored_chamfer_segments(edges);
                                let segments = match reference_bounds {
                                    Some(reference_bounds) => remap_chamfer_segments(
                                        &mesh,
                                        &stored,
                                        *reference_bounds,
                                    )
                                    .ok_or_else(|| {
                                        "the selected edge no longer resolves on the rebuilt base"
                                            .to_string()
                                    })?,
                                    None => stored,
                                };
                                let bounds = mesh_reference_bounds(&mesh);
                                self.evaluate_chamfer_feature(
                                    base_shape, &segments, *chamfer, *fillet,
                                )
                                .map(|(shape, _)| {
                                    (
                                        shape,
                                        Feature::Chamfer {
                                            target: *target,
                                            base: *base,
                                            edges: model_chamfer_segments(&segments),
                                            reference_bounds: bounds,
                                            chamfer: *chamfer,
                                            fillet: *fillet,
                                        },
                                    )
                                })
                            })
                        }
                        _ => continue,
                    }
                };

                match result {
                    Ok((shape, rebuilt_feature)) => {
                        rebuilt_shapes.insert(body_id, shape);
                        rebuilt.push((body_id, shape, rebuilt_feature));
                        pending.remove(&body_id);
                        progressed = true;
                    }
                    Err(error) => {
                        return self.discard_rebuilt_shapes(
                            rebuilt,
                            format!("Body {body_id} cannot be rebuilt: {error}"),
                        );
                    }
                }
            }

            if !progressed {
                let mut ids: Vec<_> = pending.iter().copied().collect();
                ids.sort_unstable();
                return self.discard_rebuilt_shapes(
                    rebuilt,
                    format!(
                        "Dependency cycle or missing rebuild metadata affects object(s): {}",
                        ids.iter()
                            .map(u64::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }

        Ok(rebuilt)
    }

    fn rebuild_boolean_operation(
        &mut self,
        op: DerivedBooleanOp,
        operands: &[BodyId],
        rebuilt_shapes: &HashMap<BodyId, ShapeId>,
    ) -> Result<ShapeId, String> {
        if operands.len() < 2 {
            return Err("Boolean operation has fewer than two operands".into());
        }
        let mut shapes = Vec::with_capacity(operands.len());
        for body_id in operands {
            let shape = rebuilt_shapes
                .get(body_id)
                .copied()
                .or_else(|| self.doc.body(*body_id).and_then(|body| body.shape))
                .ok_or_else(|| format!("Boolean operand {body_id} has no shape"))?;
            shapes.push(shape);
        }

        let mut combined = shapes[0];
        let mut combined_is_temporary = false;
        for shape in shapes.into_iter().skip(1) {
            let next = match op {
                DerivedBooleanOp::Union => self.kernel.boolean_union(combined, shape),
                DerivedBooleanOp::Subtract => self.kernel.boolean_subtract(combined, shape),
                DerivedBooleanOp::Intersect => self.kernel.boolean_intersect(combined, shape),
            };
            match next {
                Ok(next) => {
                    if combined_is_temporary {
                        self.kernel.delete(combined);
                    }
                    combined = next;
                    combined_is_temporary = true;
                }
                Err(error) => {
                    if combined_is_temporary {
                        self.kernel.delete(combined);
                    }
                    return Err(error.to_string());
                }
            }
        }
        Ok(combined)
    }

    fn rebuild_derived_operation(
        &mut self,
        operation: &DerivedOperation,
        rebuilt_shapes: &HashMap<BodyId, ShapeId>,
    ) -> Result<ShapeId, String> {
        let source_shape = |source: BodyId| {
            rebuilt_shapes
                .get(&source)
                .copied()
                .or_else(|| self.doc.body(source).and_then(|body| body.shape))
                .ok_or_else(|| format!("Derived source object {source} has no shape"))
        };
        match operation {
            DerivedOperation::Boolean { .. } => {
                Err("Boolean recipe reached the single-source evaluator".into())
            }
            DerivedOperation::Mirror { source, plane } => self
                .kernel
                .mirror(source_shape(*source)?, plane)
                .map_err(|error| error.to_string()),
            DerivedOperation::Copy { source, offset, .. } => {
                let matrix = glam::Mat4::from_translation(glam::Vec3::new(
                    offset[0] as f32,
                    offset[1] as f32,
                    offset[2] as f32,
                ));
                self.kernel
                    .transform(source_shape(*source)?, &matrix)
                    .map_err(|error| error.to_string())
            }
            DerivedOperation::Split {
                source,
                plane,
                piece_index,
            } => {
                let mut pieces = self
                    .kernel
                    .split_by_plane(source_shape(*source)?, plane)
                    .map_err(|error| error.to_string())?;
                if *piece_index >= pieces.len() {
                    let piece_count = pieces.len();
                    for shape in pieces {
                        self.kernel.delete(shape);
                    }
                    return Err(format!(
                        "Split piece {} no longer exists ({} piece(s) remain)",
                        piece_index, piece_count
                    ));
                }
                let selected = pieces.remove(*piece_index);
                for shape in pieces {
                    self.kernel.delete(shape);
                }
                Ok(selected)
            }
        }
    }

    fn discard_rebuilt_shapes(
        &mut self,
        rebuilt: Vec<(BodyId, ShapeId, Feature)>,
        error: String,
    ) -> Result<Vec<(BodyId, ShapeId, Feature)>, String> {
        for (_, shape, _) in rebuilt {
            self.kernel.delete(shape);
        }
        Err(error)
    }

    /// Rebuild the persistent Sketch body and all supported dependents while
    /// Edit Sketch remains open. This is a transient document replacement:
    /// no document Undo entry is created until Done, and Cancel restores the
    /// snapshot captured by `edit_sketch`.
    pub fn sync_active_sketch_edit(&mut self) -> bool {
        let (Some(id), Some(sketch)) = (self.sketch_edit_body, self.sketch.as_ref()) else {
            return false;
        };
        let updated_sketch = SketchFeature {
            vertex_locks: sketch.vertex_locks.clone(),
            plane: sketch.plane,
            entities: sketch.entities.clone(),
            world_xyz_coordinates: sketch.world_xyz_coordinates,
            on_body: self.sketch_on_body,
        };
        let rebuilt = match self.rebuild_sketch_dependents(id, &updated_sketch) {
            Ok(rebuilt) => rebuilt,
            Err(error) => {
                self.toast(format!("Live Sketch rebuild failed: {error}"));
                self.set_hint("Adjust the sketch profile; the last valid solid is still shown");
                return false;
            }
        };
        if let Some(body) = self.doc.body_mut(id) {
            body.feature = Feature::Sketch(updated_sketch);
        }
        for (body_id, shape, feature) in rebuilt {
            if let Some(body) = self.doc.body_mut(body_id) {
                body.shape = Some(shape);
                body.feature = feature;
            }
        }
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.collect_kernel_garbage();
        self.bump_scene();
        true
    }

    pub fn cancel_sketch(&mut self) {
        if let Some(original) = self.sketch_edit_original_doc.take() {
            self.doc = original;
            self.selected = std::mem::take(&mut self.sketch_edit_original_selection);
            self.mesh_cache.clear();
            self.section_cache.clear();
        } else {
            self.sketch_edit_original_selection.clear();
        }
        self.sketch = None;
        self.sketch_edit_body = None;
        self.restore_view();
        self.mode = Mode::Idle;
        self.collect_kernel_garbage();
        self.bump_scene();
    }

    /// Empty new sketches leave without creating a body. Empty edited sketches
    /// must commit their deletions (or reject an invalid dependent rebuild),
    /// rather than silently restoring everything the user just trimmed away.
    pub fn exit_sketch(&mut self) {
        if !self.apply_sketch_before_tool_switch() {
            return;
        }
        self.finish_or_discard_empty_sketch();
    }

    pub fn escape_sketch(&mut self) {
        if self
            .sketch
            .as_mut()
            .is_some_and(SketchSession::finish_chain_if_any)
        {
            self.bump_scene();
            return;
        }
        self.finish_or_discard_empty_sketch();
    }

    fn finish_or_discard_empty_sketch(&mut self) {
        if self.sketch_edit_body.is_none()
            && self
                .sketch
                .as_ref()
                .is_some_and(|sketch| sketch.entities.is_empty())
        {
            self.cancel_sketch();
            self.toast("Exited empty Sketch");
        } else {
            self.finish_sketch();
        }
        if self.mode == Mode::Idle {
            self.collapse_tool_menus();
        }
    }

    fn restore_view(&mut self) {
        if let Some(cam) = self.saved_camera.take() {
            self.camera = cam;
        }
    }

    pub fn set_sketch_tool(&mut self, tool: SketchTool) {
        if !self.apply_sketch_before_tool_switch() {
            return;
        }
        if let Some(sk) = &mut self.sketch {
            sk.set_tool(tool);
            self.set_hint(tool.hint());
            self.bump_scene();
        }
    }

    fn apply_sketch_before_tool_switch(&mut self) -> bool {
        let result = self.sketch.as_mut().map(|sketch| {
            if !sketch.transform_selection.is_empty()
                && matches!(sketch.tool, SketchTool::Move | SketchTool::Rotate)
            {
                sketch.apply_transform().map(|_| ())
            } else {
                Ok(())
            }
        });
        if let Some(Err(error)) = result {
            self.toast(error);
            return false;
        }
        self.sync_sketch_history();
        true
    }

    // ================= 拉伸/旋转 =================

    /// 从 sketch body 的选中区域开始拉伸。
    pub fn start_extrude(&mut self) {
        if self.queue_tool_switch(Self::start_extrude) {
            return;
        }
        if self.surface_ctx.is_some() && !self.finish_equation_surface() {
            return;
        }
        if self.selected.len() == 1
            && self.body(self.selected[0]).is_some_and(|b| {
                b.kind == BodyKind::Surface && matches!(b.feature, Feature::EquationSurface { .. })
            })
        {
            self.start_equation_surface_extrude();
            return;
        }
        self.start_extrude_common(false);
    }

    /// Pocket（拉伸切除）：挤出选中区域并从目标体上减掉。
    /// 目标体优先取当前选中面所属的 body；未定时在 Pocket 模式下单击 solid。
    pub fn start_pocket(&mut self) {
        if self.queue_tool_switch(Self::start_pocket) {
            return;
        }
        self.start_extrude_common(true);
    }

    /// 将当前轮廓选择统一成 SketchFeature + Region2D。
    /// 真实 Sketch 沿用其实体；平面 Surface 则从共面 patch 的固定边界生成临时线框。
    fn text_region_group(&mut self, id: BodyId, hit: usize) -> Result<Option<Vec<usize>>, String> {
        let Some(entities) = self
            .body(id)
            .and_then(Body::sketch)
            .map(|sketch| sketch.entities.clone())
        else {
            return Ok(None);
        };
        let regions = self.regions_of(id);
        for entity in &entities {
            let group = crate::sketch_text::filled_regions(entity, &regions)
                .map_err(|error| format!("body={id}: {error}"))?;
            if group.contains(&hit) {
                return Ok(Some(group));
            }
        }
        Ok(None)
    }

    fn select_text_region_groups(
        &mut self,
        id: BodyId,
        entities: &[SketchEntity],
        regions: &[Region2D],
    ) {
        let result: Result<Vec<_>, _> = entities
            .iter()
            .map(|entity| crate::sketch_text::filled_regions(entity, regions))
            .collect();
        match result {
            Ok(groups) => {
                self.set_region_selection(id, groups.into_iter().flatten().collect());
            }
            Err(error) => {
                self.clear_region_selection();
                eprintln!("body={id}: {error}");
                self.toast(self.settings.language.text(
                    "Cannot fill this text; its outline is kept",
                    "无法填充此文字，已保留轮廓",
                ));
            }
        }
    }

    fn select_region_group(&mut self, id: BodyId, group: Vec<usize>, additive: bool) {
        self.sanitize_region_selection();
        let mut indices = self.selected_regions_for(id).to_vec();
        if additive && self.selected == vec![id] {
            if group.iter().all(|i| indices.contains(i)) {
                indices.retain(|i| !group.contains(i));
            } else {
                indices.extend(group);
            }
        } else {
            self.selected = vec![id];
            indices = group;
        }
        self.set_region_selection(id, indices);
        self.face_pick = None;
        self.selected_edge = None;
        self.bump_scene();
    }

    pub(crate) fn has_bounded_planar_selection(&mut self) -> bool {
        if let Some(face) = self.face_pick.clone() {
            let Some(shape) = self
                .body(face.body_id)
                .filter(|body| body.visible)
                .and_then(|body| body.shape)
            else {
                return false;
            };
            let mesh = self.mesh_for(shape);
            if let Some(exact) = self.kernel.exact_shape(shape) {
                let planar = exact_face_for_hit(exact, &mesh, &face)
                    .and_then(|index| exact.planar_faces.get(index as usize))
                    .copied()
                    .unwrap_or(false);
                if !planar {
                    return false;
                }
            }
            return bounded_planar_face_profile(&mesh, &face).is_ok_and(|(_, region)| {
                s33d_model::triangulate_region(&region.outer, &region.holes).is_ok()
            });
        }

        let [owner] = self.selected.as_slice() else {
            return false;
        };
        let owner = *owner;
        let regions = self.selected_regions_for(owner).to_vec();
        !regions.is_empty()
            && regions
                .into_iter()
                .all(|index| self.validated_region_mesh(owner, index).is_ok())
    }

    pub(crate) fn sketch_on_selected_profile(&mut self) {
        if let Some(face) = self.face_pick.clone() {
            self.start_sketch_on_face(&face);
            return;
        }
        match self.selected_profile_source("Sketch") {
            Ok((_, sketch, _, _)) => {
                self.start_sketch_with_coordinate_display(
                    sketch.plane,
                    sketch.world_xyz_coordinates,
                );
                self.sketch_on_body = sketch.on_body;
            }
            Err(error) => self.toast(error),
        }
    }

    pub(crate) fn deselect_surface(&mut self) {
        if self.face_pick.take().is_some() || !self.selected_regions.is_empty() {
            self.clear_region_selection();
            self.bump_scene();
        }
    }

    fn selected_profile_source(
        &mut self,
        action: &str,
    ) -> std::result::Result<(BodyId, SketchFeature, Vec<Region2D>, Vec<usize>), String> {
        if self.sanitize_region_selection() {
            return Err(
                "Sketch geometry or selection changed; select the closed region again".into(),
            );
        }
        if let Some(face) = self
            .face_pick
            .clone()
            .filter(|_| !matches!(self.mode, Mode::Split | Mode::SectionView))
        {
            let shape = self
                .body(face.body_id)
                .and_then(|body| body.shape)
                .ok_or_else(|| "Selected surface has no solid geometry".to_string())?;
            let mesh = self.mesh_for(shape);
            let (plane, region) = bounded_planar_face_profile(&mesh, &face)?;
            s33d_model::triangulate_region(&region.outer, &region.holes)
                .map_err(|error| format!("Surface region validation failed: {error}"))?;
            let sketch = sketch_from_surface_region(plane, &region, face.body_id);
            return Ok((face.body_id, sketch, vec![region], vec![0]));
        }

        if self.selected.len() != 1 {
            return Err(format!(
                "Select one closed Sketch region or one bounded planar surface to {action}"
            ));
        }
        let id = self.selected[0];
        let mut sketch = self.profile_sketch(id).ok_or_else(|| {
            format!("Select a closed Sketch region or bounded planar surface to {action}")
        })?;
        // Older projects can retain a surface link after its source was
        // removed/replaced (or even point back to the sketch itself). The
        // planar profile remains usable as a standalone extrusion.
        if sketch
            .on_body
            .is_some_and(|source| source == id || self.body(source).and_then(|b| b.shape).is_none())
        {
            sketch.on_body = None;
            self.toast(self.settings.language.text(
                "Original surface is unavailable; using the sketch as a standalone profile",
                "原表面已不存在；将草图作为独立轮廓使用",
            ));
        }
        let regions = self.regions_of(id);
        if regions.is_empty() {
            return Err(format!("Sketch has no closed profile to {action}"));
        }
        let picked_indices = if self.selected_regions.is_empty() {
            if sketch.entities.len() == 1
                && matches!(sketch.entities[0], SketchEntity::Path { text: Some(_), .. })
            {
                crate::sketch_text::filled_regions(&sketch.entities[0], &regions)?
            } else if regions.len() > 1 {
                return Err(format!(
                    "Double-click one closed region first, then {action}"
                ));
            } else {
                vec![0]
            }
        } else {
            let mut indices: Vec<_> = self
                .selected_regions
                .iter()
                .copied()
                .filter(|&index| index < regions.len())
                .collect();
            indices.sort_unstable();
            indices.dedup();
            indices
        };
        for &index in &picked_indices {
            self.validated_region_mesh(id, index)?;
        }
        let picked: Vec<_> = picked_indices
            .iter()
            .filter_map(|&index| regions.get(index).cloned())
            .collect();
        if picked.is_empty() {
            return Err("No closed region selected".to_string());
        }
        Ok((id, sketch, picked, picked_indices))
    }

    fn start_extrude_common(&mut self, pocket: bool) {
        self.extrude_drag_start = None;
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        let action = if pocket { "Pocket" } else { "Extrude" };
        let (id, sketch, picked, region_indices) = match self.selected_profile_source(action) {
            Ok(source) => source,
            Err(message) => {
                self.toast(message);
                return;
            }
        };
        let picked_count = picked.len();
        let picked_areas: Vec<f64> = picked.iter().map(|r| r.signed_area()).collect();
        let exact_face = if sketch.on_body == Some(id) {
            self.face_pick
                .clone()
                .filter(|face| face.body_id == id)
                .and_then(|face| {
                    let shape = self.body(id)?.shape?;
                    let display = self.mesh_for(shape);
                    let exact = self.kernel.exact_shape(shape)?;
                    exact_face_for_hit(exact, &display, &face).map(|index| (shape, index))
                })
        } else {
            None
        };
        const INITIAL_DEPTH: f64 = 20.0;
        let target = pocket
            .then(|| self.infer_pocket_target(id, &sketch, &region_indices, INITIAL_DEPTH))
            .flatten();
        self.deactivate_sidebar_tools();
        self.extrude_ctx = Some(ExtrudeCtx {
            body_id: id,
            exact_face,
            sketch,
            regions: picked,
            region_indices,
            depth: INITIAL_DEPTH,
            symmetric: false,
            ghost_shapes: Vec::new(),
            preview_error: None,
            pocket,
            target_body: target,
            through: false,
        });
        self.mode = Mode::Extrude;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.set_hint(if pocket {
            "Pocket: click a target body if needed, drag in the viewport to set depth - Done to commit"
        } else {
            "Extrude: drag in the viewport to set depth, or type it in the panel - Done to commit"
        });
        if self.debug_enabled {
            let line = format!(
                "[extrude-start] t={} selected={:?} regions={:?} picked={} areas={:?}\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0),
                self.selected,
                self.selected_regions,
                picked_count,
                picked_areas
            );
            append_debug_log(&line);
        }
        if self.debug_enabled {
            self.record_history(format!(
                "[extrude] start: picked={} region(s) idx={:?} areas={:?}",
                picked_count,
                self.selected_regions,
                picked_areas
                    .iter()
                    .map(|a| format!("{:.1}", a))
                    .collect::<Vec<_>>()
            ));
        }
        // 初始化 ghost 预览
        self.set_extrude_depth(INITIAL_DEPTH);
    }

    fn infer_pocket_target(
        &mut self,
        source: BodyId,
        sketch: &SketchFeature,
        regions: &[usize],
        depth: f64,
    ) -> Option<BodyId> {
        let is_visible_solid = |id| {
            self.body(id)
                .is_some_and(|b| b.visible && b.kind == BodyKind::Solid && b.shape.is_some())
        };
        if let Some(id) = self
            .face_pick
            .as_ref()
            .map(|face| face.body_id)
            .filter(|id| is_visible_solid(*id))
        {
            return Some(id);
        }
        let mut selected = self
            .selected
            .iter()
            .copied()
            .filter(|id| is_visible_solid(*id));
        if let Some(id) = selected.next() {
            return selected.next().is_none().then_some(id);
        }
        if let Some(id) = sketch.on_body.filter(|id| is_visible_solid(*id)) {
            return Some(id);
        }
        let candidates: Vec<_> = self
            .doc
            .bodies
            .iter()
            .filter(|body| is_visible_solid(body.id))
            .filter_map(|body| body.shape.map(|shape| (body.id, shape)))
            .collect();
        let mut linked = candidates.iter().map(|&(id, _)| id).filter(|&id| {
            self.pocket_sources(id).contains(&source)
                || matches!(self.body(id).map(|body| &body.feature),
                    Some(Feature::Extrude { source_sketch: Some(id), .. }) if *id == source)
        });
        if let Some(id) = linked.next() {
            return linked.next().is_none().then_some(id);
        }

        // Interior samples respect holes and concave outlines. This is only
        // target selection: the committed cutter is independently validated.
        let mut points = Vec::new();
        for &region in regions {
            let mesh = self.validated_region_mesh(source, region).ok()?;
            for triangle in mesh
                .indices
                .iter()
                .step_by(mesh.indices.len().div_ceil(24).max(1))
            {
                let [a, b, c] = triangle.map(|i| mesh.vertices[i as usize]);
                points.push(glam::DVec3::from_array(
                    sketch
                        .plane
                        .to_world((a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0),
                ));
            }
        }
        let normal = sketch.plane.normal();
        let mut matching = candidates.into_iter().filter_map(|(id, shape)| {
            let mesh = self.mesh_for(shape);
            points
                .iter()
                .any(|&point| {
                    [normal, -normal].into_iter().any(|direction| {
                        let start = point - direction * 1e-4;
                        ray_mesh_intersect(start.to_array(), direction.to_array(), &mesh)
                            .is_some_and(|distance| distance <= depth + 1e-4)
                    })
                })
                .then_some(id)
        });
        let id = matching.next()?;
        matching.next().is_none().then_some(id)
    }

    /// 更新拉伸深度并重建 ghost 预览形状。
    pub fn set_extrude_depth(&mut self, depth: f64) {
        let Some((plane, regions, old_ghosts, pocket, through, target_body)) =
            self.extrude_ctx.as_mut().map(|c| {
                c.depth = depth;
                (
                    c.sketch.plane.clone(),
                    c.regions.clone(),
                    std::mem::take(&mut c.ghost_shapes),
                    c.pocket,
                    c.through,
                    c.target_body,
                )
            })
        else {
            return;
        };
        for old in old_ghosts {
            self.kernel.delete(old);
        }
        // Display the requested removal range, without the Boolean cutter's
        // robustness padding. Commit constructs its own extended cutter.
        let through_spec = if pocket && through {
            target_body
                .and_then(|target| self.doc.body(target).and_then(|body| body.shape))
                .and_then(|shape| self.kernel.tessellate(shape).ok())
                .and_then(|mesh| pocket_target_span(&plane, &mesh).ok())
                .map(|(min, max)| {
                    let mut start = plane.clone();
                    start.origin = (plane.origin() + plane.normal() * max).to_array();
                    (start, min - max)
                })
        } else {
            None
        };
        let (preview_plane, preview_depth) = if let Some(spec) = through_spec {
            spec
        } else if pocket {
            (plane.clone(), -depth.max(0.1))
        } else {
            let mut p = plane.clone();
            if self.extrude_ctx.as_ref().is_some_and(|ctx| ctx.symmetric) {
                p.origin = (plane.origin() - plane.normal() * depth.max(0.1) / 2.0).to_array();
            }
            (p, depth.max(0.1))
        };
        let mut shapes = Vec::with_capacity(regions.len());
        let mut preview_error = None;
        let exact_face = self.extrude_ctx.as_ref().and_then(|ctx| ctx.exact_face);
        if let Some((source, face)) = exact_face {
            let start = (preview_plane.origin() - plane.origin()).dot(plane.normal());
            let preview = self
                .kernel
                .exact_shape(source)
                .ok_or_else(|| "Preview source is unavailable".to_owned())
                .and_then(|input| {
                    exact_tools::face_preview_mesh(
                        input,
                        face,
                        plane.normal().to_array(),
                        preview_depth,
                        start,
                    )
                });
            match preview {
                Ok(mesh) => shapes.push(self.kernel.insert_mesh(mesh)),
                Err(error) => preview_error = Some(error),
            }
        } else {
            for region in &regions {
                match self
                    .kernel
                    .extrude_region(region, &preview_plane, preview_depth)
                {
                    Ok(id) => shapes.push(id),
                    Err(error) => {
                        preview_error = Some(error.to_string());
                        break;
                    }
                }
            }
        }
        if preview_error.is_some() {
            for shape in shapes.drain(..) {
                self.kernel.delete(shape);
            }
        }
        if let Some(ctx) = &mut self.extrude_ctx {
            ctx.ghost_shapes = shapes;
            ctx.preview_error = preview_error;
        }
        self.mesh_cache.clear();
        self.bump_scene();
    }

    fn extrude_native_face_range(
        &mut self,
        source: ShapeId,
        face: u32,
        normal: [f64; 3],
        depth: f64,
        start: f64,
    ) -> s33d_kernel::Result<ShapeId> {
        let exact = self.kernel.exact_shape(source).ok_or_else(|| {
            s33d_kernel::KernelError::Geometry("Source exact shape no longer exists".into())
        })?;
        let prism = exact.extrude_planar_face_range(face, normal, depth, start)?;
        Ok(self.kernel.insert_exact(prism))
    }

    /// 提交拉伸，生成 solid body。
    pub fn commit_extrude(&mut self) {
        if self.model_work.is_none() && self.start_exact_face_commit() {
            return;
        }
        if self.queue_model_command(("Extrude / Pocket", "拉伸 / 挖孔"), Self::commit_extrude) {
            return;
        }
        let Some(ctx) = self.extrude_ctx.take() else {
            return;
        };
        for gs in ctx.ghost_shapes {
            self.kernel.delete(gs);
        }
        let sk = ctx.sketch.clone();
        let plane = sk.plane.clone();
        // Blind Pocket extends the Boolean cutter 2 mm outward to avoid
        // coplanar failures; its inward end remains exactly sketch plane - depth.
        // Through instead spans the complete target with padding at both ends.
        let (cutter_plane, depth) = if ctx.pocket && ctx.through {
            let spec = ctx
                .target_body
                .and_then(|target| self.doc.body(target).and_then(|body| body.shape))
                .ok_or_else(|| "Pocket needs a target body".to_string())
                .and_then(|shape| self.kernel.tessellate(shape).map_err(|e| e.to_string()))
                .and_then(|mesh| through_pocket_cutter_spec(&plane, &mesh));
            match spec {
                Ok(spec) => spec,
                Err(error) => {
                    self.toast(format!("Pocket through failed: {error}"));
                    self.mode = Mode::Idle;
                    self.bump_scene();
                    return;
                }
            }
        } else if ctx.pocket {
            let n = plane.normal();
            let mut p = plane.clone();
            p.origin = [
                plane.origin[0] + n.x * 2.0,
                plane.origin[1] + n.y * 2.0,
                plane.origin[2] + n.z * 2.0,
            ];
            // Pocket is inward; symmetric applies only to additive extrusion.
            // Its inward end matches the unpadded display preview.
            let d = ctx.depth + 2.0;
            (p, -d)
        } else if ctx.symmetric {
            let mut p = plane.clone();
            p.origin = (plane.origin() - plane.normal() * ctx.depth / 2.0).to_array();
            (p, ctx.depth)
        } else {
            (plane.clone(), ctx.depth)
        };
        let mut shapes = Vec::new();
        if let Some((source, face)) = ctx.exact_face {
            let start = (cutter_plane.origin() - plane.origin()).dot(plane.normal());
            match self.extrude_native_face_range(
                source,
                face,
                plane.normal().to_array(),
                depth,
                start,
            ) {
                Ok(id) => shapes.push(id),
                Err(error) => {
                    self.toast(format!("Surface Extrude failed: {error}"));
                    self.mode = Mode::Idle;
                    self.bump_scene();
                    return;
                }
            }
        } else {
            for region in &ctx.regions {
                match self.kernel.extrude_region(region, &cutter_plane, depth) {
                    Ok(id) => shapes.push(id),
                    Err(e) => {
                        for shape in shapes {
                            self.kernel.delete(shape);
                        }
                        self.toast(format!("Extrude failed: {e}"));
                        self.mode = Mode::Idle;
                        return;
                    }
                }
            }
        }
        // Surface Sketch extrusion extends the source object. Apply each
        // selected region to that shared base separately: disconnected regions
        // cannot be unioned with each other first, and adjacent coplanar regions
        // are also substantially more reliable in this order.
        if !ctx.pocket {
            if let Some(target) = sk.on_body {
                let Some(base_shape) = self.doc.body(target).and_then(|body| body.shape) else {
                    for extension in shapes {
                        self.kernel.delete(extension);
                    }
                    self.toast("Surface Extrude failed: source object has no shape");
                    self.mode = Mode::Idle;
                    return;
                };
                let result =
                    match union_extensions_into_base(&mut self.kernel, base_shape, shapes, None) {
                        Ok(result) => result,
                        Err(error) => {
                            self.toast(format!("Surface Extrude failed: {error}"));
                            self.mode = Mode::Idle;
                            self.bump_scene();
                            return;
                        }
                    };
                self.begin_document_change();
                let pocket_source_sketches = self.pocket_sources(target);
                if let Some(body) = self.doc.body_mut(target) {
                    body.shape = Some(result);
                    body.feature = Feature::Extrude {
                        sketch: sk.clone(),
                        params: ExtrudeParams {
                            depth: ctx.depth,
                            symmetric: ctx.symmetric,
                            bevel: 0.0,
                        },
                        source_sketch: None,
                        pocket_source_sketches,
                        region_indices: Vec::new(),
                    };
                }
                self.selected = vec![target];
                self.clear_region_selection();
                self.face_pick = None;
                self.hover_face = None;
                self.selected_edge = None;
                self.hover_edge = None;
                self.mode = Mode::Idle;
                self.mesh_cache.clear();
                let depth = self.settings.unit.format_mm(ctx.depth);
                self.toast(format!("Extended object {target} by {depth}"));
                self.record_history(format!("Surface Extrude object {target} by {depth}"));
                self.bump_scene();
                return;
            }
        }
        let shape = match merge_shapes_checked(&mut self.kernel, shapes) {
            Ok(shape) => Some(shape),
            Err(error) => {
                self.toast(format!("Extrude failed: {error}"));
                self.record_history(format!("Extrude failed: {error}"));
                self.mode = Mode::Idle;
                self.bump_scene();
                return;
            }
        };
        // Pocket：把挤出体从目标体上减掉（打洞/挖槽），不新增 body
        if ctx.pocket {
            let mut pocket_source_sketches = ctx
                .target_body
                .map(|id| self.pocket_sources(id))
                .unwrap_or_default();
            if let Some(id) = self.profile_source_link(ctx.body_id) {
                if !pocket_source_sketches.contains(&id) {
                    pocket_source_sketches.push(id);
                }
            }
            let Some(target) = ctx.target_body else {
                self.toast("Pocket needs a target body - click a solid in the viewport first");
                self.mode = Mode::Idle;
                return;
            };
            let Some(cutter) = shape else {
                self.toast("Pocket failed: no cutter shape");
                self.mode = Mode::Idle;
                return;
            };
            let Some(tshape) = self.doc.body(target).and_then(|b| b.shape) else {
                self.toast("Pocket target has no shape");
                self.mode = Mode::Idle;
                return;
            };
            // The same through range was already used to build the cutter.
            // Never rebuild it a second time from display-mesh boundaries.
            match self.kernel.boolean_subtract(tshape, cutter) {
                Ok(result) => {
                    self.kernel.delete(cutter);
                    // 注意：不删除 tshape——undo 快照仍引用它
                    self.begin_document_change();
                    if let Some(b) = self.doc.body_mut(target) {
                        b.shape = Some(result);
                        // 更新 feature 记录为 pocket
                        if let Feature::Extrude { params, .. } = &b.feature {
                            let _ = params;
                        }
                        b.feature = Feature::Extrude {
                            sketch: sk.clone(),
                            params: ExtrudeParams {
                                depth: ctx.depth,
                                symmetric: ctx.symmetric,
                                bevel: 0.0,
                            },
                            source_sketch: None,
                            pocket_source_sketches,
                            region_indices: Vec::new(),
                        };
                    }
                    self.selected = vec![target];
                    self.record_history(format!("[sel] select body {target} clears regions"));
                    self.clear_region_selection();
                    // 布尔后拓扑已改变，旧面/棱的三角形索引不再有效。
                    // 若继续渲染旧索引，会在孔内连出看似“漏模”的大三角面。
                    self.face_pick = None;
                    self.hover_face = None;
                    self.selected_edge = None;
                    self.hover_edge = None;
                    self.mode = Mode::Idle;
                    self.toast("Pocket cut done");
                    self.record_history(format!(
                        "Pocket {}{}",
                        self.settings.unit.format_mm(ctx.depth),
                        if ctx.through { " (through)" } else { "" }
                    ));
                    self.mesh_cache.clear();
                    self.bump_scene();
                }
                Err(e) => {
                    self.kernel.delete(cutter);
                    self.toast(format!("Pocket failed: {e}"));
                    self.mode = Mode::Idle;
                }
            }
            return;
        }
        if let Some(shape) = shape {
            let source_sketch = self.profile_source_link(ctx.body_id);
            let id = self.doc.alloc_id();
            let n = self.doc.bodies.len() + 1;
            let mat = Material {
                color: palette_color(n),
                ..Material::default()
            };
            self.begin_document_change();
            self.doc.add_body(Body {
                id,
                name: format!("Body {id}"),
                kind: BodyKind::Solid,
                feature: Feature::Extrude {
                    sketch: sk.clone(),
                    params: ExtrudeParams {
                        depth: ctx.depth,
                        symmetric: ctx.symmetric,
                        bevel: 0.0,
                    },
                    source_sketch,
                    pocket_source_sketches: Vec::new(),
                    region_indices: ctx.region_indices.clone(),
                },
                shape: Some(shape),
                material: mat,
                visible: true,
            });
            self.selected = vec![id];
            if self.debug_enabled {
                self.record_history(format!("[sel] select body {id} clears regions"));
            }
            self.clear_region_selection();
            self.mode = Mode::Idle;
            self.toast("Body created");
            self.record_history(format!(
                "Extrude {}",
                self.settings.unit.format_mm(ctx.depth)
            ));
            self.bump_scene();
        }
    }

    pub fn cancel_extrude(&mut self) {
        if let Some(ctx) = self.extrude_ctx.take() {
            for gs in ctx.ghost_shapes {
                self.kernel.delete(gs);
            }
        }
        self.mode = Mode::Idle;
        self.bump_scene();
    }

    /// 开始旋转：Sketch 区域或有固定边界的平面 Surface 均可直接作为轮廓。
    pub fn start_revolve(&mut self) {
        if self.queue_tool_switch(Self::start_revolve) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        let (body_id, sketch, mut regions, region_indices) =
            match self.selected_profile_source("Revolve") {
                Ok(source) => source,
                Err(message) => {
                    self.toast(message);
                    return;
                }
            };
        if regions.len() != 1 {
            self.toast("Revolve needs exactly one selected closed region");
            return;
        }
        self.revolve_ctx = Some(RevolveCtx {
            body_id,
            sketch,
            region: regions.remove(0),
            region_index: region_indices[0],
            axis_point: [0.0, 0.0],
            axis_dir: [0.0, 0.0],
            angle_deg: 360.0,
        });
        self.deactivate_sidebar_tools();
        self.mode = Mode::RevolveAxis;
        let axis_names = self
            .revolve_ctx
            .as_ref()
            .map(|ctx| {
                crate::sketch_session::world_axis_bindings_for_plane(&ctx.sketch.plane)
                    .map(|binding| binding.label())
                    .join("/")
            })
            .unwrap_or_else(|| "profile axes".to_string());
        self.set_hint(format!(
            "Revolve: click a straight in-plane edge, or choose {axis_names} in Properties"
        ));
        self.bump_scene();
    }

    /// 用轮廓平面内的局部线作为旋转轴。
    pub fn revolve_with_axis(&mut self, axis_point: [f64; 2], axis_dir: [f64; 2]) {
        let Some(ctx) = self.revolve_ctx.as_mut() else {
            return;
        };
        let length_sq = axis_dir[0] * axis_dir[0] + axis_dir[1] * axis_dir[1];
        if !length_sq.is_finite() || length_sq < 1e-12 {
            self.toast("Revolve axis has zero length");
            return;
        }
        ctx.axis_point = axis_point;
        ctx.axis_dir = axis_dir;
        self.mode = Mode::Revolve;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.set_hint("Revolve: set angle and confirm (Enter)");
        self.bump_scene();
    }

    /// 将拾取到的世界空间直棱投影为当前 Surface/Sketch 平面内的旋转轴。
    pub fn revolve_with_world_axis(&mut self, axis: RotationAxis) {
        let Some(plane) = self
            .revolve_ctx
            .as_ref()
            .map(|ctx| ctx.sketch.plane.clone())
        else {
            return;
        };
        let point = [
            axis.point[0] as f64,
            axis.point[1] as f64,
            axis.point[2] as f64,
        ];
        let direction = glam::DVec3::new(
            axis.direction[0] as f64,
            axis.direction[1] as f64,
            axis.direction[2] as f64,
        );
        let distance_to_plane = (glam::DVec3::from_array(point) - plane.origin())
            .dot(plane.normal())
            .abs();
        let normal_component = direction.dot(plane.normal()).abs();
        if distance_to_plane > 1e-2 || normal_component > 1e-3 {
            self.toast("Revolve axis must lie in the selected profile plane");
            return;
        }
        let q = (glam::DVec3::from_array(point) + direction).to_array();
        let p2 = plane.to_local(point);
        let q2 = plane.to_local(q);
        self.revolve_with_axis([p2.0, p2.1], [q2.0 - p2.0, q2.1 - p2.1]);
    }

    /// 使用基准面轴（X 或 Y）。
    pub fn revolve_with_plane_axis(&mut self, which: char) {
        let Some(region) = self.revolve_ctx.as_ref().map(|ctx| ctx.region.clone()) else {
            return;
        };
        let min_u = region
            .outer
            .iter()
            .map(|point| point[0])
            .fold(f64::INFINITY, f64::min);
        let min_v = region
            .outer
            .iter()
            .map(|point| point[1])
            .fold(f64::INFINITY, f64::min);
        let (p, d) = match which {
            'X' => ([0.0, min_v], [1.0, 0.0]),
            'Y' => ([min_u, 0.0], [0.0, 1.0]),
            _ => return,
        };
        self.revolve_with_axis(p, d);
    }

    pub fn commit_revolve(&mut self) {
        if self.queue_model_command(("Revolve", "旋转"), Self::commit_revolve) {
            return;
        }
        let Some(ctx) = self.revolve_ctx.take() else {
            return;
        };
        let sk = ctx.sketch.clone();
        let axis = s33d_kernel::Axis2D {
            point: ctx.axis_point,
            dir: ctx.axis_dir,
        };
        match self
            .kernel
            .revolve_region(&ctx.region, &sk.plane, &axis, ctx.angle_deg)
        {
            Ok(shape) => {
                let source_sketch = self.profile_source_link(ctx.body_id);
                let id = self.doc.alloc_id();
                let n = self.doc.bodies.len() + 1;
                self.begin_document_change();
                self.doc.add_body(Body {
                    id,
                    name: format!("Body {id}"),
                    kind: BodyKind::Solid,
                    feature: Feature::Revolve {
                        sketch: sk.clone(),
                        params: RevolveParams {
                            axis_point: ctx.axis_point,
                            axis_dir: ctx.axis_dir,
                            angle_deg: ctx.angle_deg,
                        },
                        source_sketch,
                        region_index: Some(ctx.region_index),
                    },
                    shape: Some(shape),
                    material: Material {
                        color: palette_color(n),
                        ..Material::default()
                    },
                    visible: true,
                });
                self.selected = vec![id];
                self.mode = Mode::Idle;
                self.toast("Body created by revolve");
                self.record_history(format!("Revolve {}°", ctx.angle_deg));
                self.bump_scene();
            }
            Err(e) => {
                self.toast(format!("Revolve failed: {e}"));
                self.mode = Mode::Idle;
            }
        }
    }

    pub fn cancel_revolve(&mut self) {
        self.revolve_ctx = None;
        self.mode = Mode::Idle;
        self.bump_scene();
    }

    // ================= 布尔 =================

    pub fn start_boolean(&mut self, op: BoolOp) {
        if self.queue_tool_switch(move |worker| worker.start_boolean(op)) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        let selected_solid_count = self
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.doc.body(*id).is_some_and(|body| {
                    body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
                })
            })
            .collect::<std::collections::HashSet<_>>()
            .len();
        if op != BoolOp::Union && selected_solid_count > 2 {
            self.boolean_ctx = None;
            self.mode = Mode::Idle;
            self.clear_object_selection_for_conflict(format!(
                "{} needs exactly two ordered objects; choose Original Object and {} Object separately",
                op.name(),
                if op == BoolOp::Subtract {
                    "Subtracting"
                } else {
                    "Intersecting"
                }
            ));
            return;
        }
        if self.mode == Mode::Boolean {
            if self.boolean_ctx.as_ref().is_some_and(|ctx| ctx.op == op) {
                self.cancel_boolean();
            } else {
                let ctx = self.prepare_boolean_context(op);
                self.boolean_ctx = Some(ctx);
                self.set_boolean_hint(op);
                self.bump_scene();
            }
            return;
        }
        if self.mode != Mode::Idle {
            return;
        }
        let solids = self
            .doc
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Solid && b.visible)
            .count();
        if solids < 2 {
            self.toast("Need at least two bodies for boolean");
            return;
        }
        self.deactivate_sidebar_tools();
        self.selected.retain(|id| {
            self.doc.body(*id).is_some_and(|body| {
                body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
            })
        });
        let ctx = self.prepare_boolean_context(op);
        self.boolean_ctx = Some(ctx);
        self.mode = Mode::Boolean;
        self.set_boolean_hint(op);
        self.bump_scene();
    }

    fn prepare_boolean_context(&mut self, op: BoolOp) -> BooleanCtx {
        let mut operands: Vec<BodyId> = self
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.doc.body(*id).is_some_and(|body| {
                    body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
                })
            })
            .collect();
        operands.dedup();
        if op == BoolOp::Union {
            self.selected = operands;
            return BooleanCtx {
                op,
                target: None,
                tool: None,
                picking: None,
                keep_originals: false,
            };
        }

        operands.truncate(2);
        let target = operands.first().copied();
        let tool = operands.get(1).copied();
        let picking = next_boolean_operand_slot(target, tool);
        self.selected = operands;
        BooleanCtx {
            op,
            target,
            tool,
            picking,
            keep_originals: false,
        }
    }

    fn set_boolean_hint(&mut self, op: BoolOp) {
        self.set_hint(match op {
            BoolOp::Union => {
                "Union: select 2 or more solid objects (Ctrl/Shift-click or Ctrl+A), then Apply"
                    .to_string()
            }
            BoolOp::Subtract | BoolOp::Intersect => {
                match self.boolean_ctx.as_ref().and_then(|ctx| ctx.picking) {
                    Some(BooleanOperandSlot::Target) => {
                        format!("{}: select the Original Object", op.name())
                    }
                    Some(BooleanOperandSlot::Tool) => format!(
                        "{}: select the {} Object",
                        op.name(),
                        if op == BoolOp::Subtract {
                            "Subtracting"
                        } else {
                            "Intersecting"
                        }
                    ),
                    None => format!("{}: operands ready; review and click Apply", op.name()),
                }
            }
        });
    }

    /// 布尔模式点击：Union 切换批量成员；其余操作填入右栏当前激活的输入槽。
    pub fn boolean_click(&mut self, body_id: BodyId, additive: bool) {
        let Some(mut ctx) = self.boolean_ctx.take() else {
            return;
        };
        let valid_operand = self.doc.body(body_id).is_some_and(|body| {
            body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
        });
        if !valid_operand {
            self.toast("Boolean operands must be visible solid objects");
            self.boolean_ctx = Some(ctx);
            return;
        }
        if ctx.op == BoolOp::Union {
            apply_workspace_owner_selection(&mut self.selected, body_id, additive);
            self.face_pick = None;
            self.selected_edge = None;
            self.set_boolean_hint(BoolOp::Union);
            self.boolean_ctx = Some(ctx);
            self.bump_scene();
            return;
        }
        let Some(slot) = ctx.picking else {
            self.toast("Click an object field in the Properties panel to replace it");
            self.boolean_ctx = Some(ctx);
            return;
        };
        let other = match slot {
            BooleanOperandSlot::Target => ctx.tool,
            BooleanOperandSlot::Tool => ctx.target,
        };
        if other == Some(body_id) {
            // Move the object to the explicitly requested slot. Never silently
            // reverse subtraction or leave the same object in both fields.
            match slot {
                BooleanOperandSlot::Target => ctx.tool = None,
                BooleanOperandSlot::Tool => ctx.target = None,
            }
        }
        match slot {
            BooleanOperandSlot::Target => ctx.target = Some(body_id),
            BooleanOperandSlot::Tool => ctx.tool = Some(body_id),
        }
        ctx.picking = next_boolean_operand_slot(ctx.target, ctx.tool);
        self.selected.clear();
        if let Some(target) = ctx.target {
            self.selected.push(target);
        }
        if let Some(tool) = ctx.tool {
            self.selected.push(tool);
        }
        let op = ctx.op;
        self.face_pick = None;
        self.selected_edge = None;
        self.boolean_ctx = Some(ctx);
        self.set_boolean_hint(op);
        self.bump_scene();
    }

    pub fn begin_boolean_operand_pick(&mut self, slot: BooleanOperandSlot) {
        let Some(ctx) = self.boolean_ctx.as_mut() else {
            return;
        };
        if ctx.op == BoolOp::Union {
            return;
        }
        ctx.picking = Some(slot);
        let op = ctx.op;
        self.set_boolean_hint(op);
        self.bump_scene();
    }

    pub fn swap_boolean_operands(&mut self) {
        let Some(ctx) = self.boolean_ctx.as_mut() else {
            return;
        };
        if self.mode != Mode::Boolean || ctx.op != BoolOp::Subtract {
            return;
        }
        std::mem::swap(&mut ctx.target, &mut ctx.tool);
        ctx.picking = next_boolean_operand_slot(ctx.target, ctx.tool);
        self.selected = ctx.target.into_iter().chain(ctx.tool).collect();
        self.face_pick = None;
        self.selected_edge = None;
        self.set_boolean_hint(BoolOp::Subtract);
        self.bump_scene();
    }

    pub fn apply_boolean_pair(&mut self) {
        if self.queue_model_command(("Boolean", "布尔运算"), Self::apply_boolean_pair) {
            return;
        }
        let Some(ctx) = self.boolean_ctx.clone() else {
            return;
        };
        if self.mode != Mode::Boolean || ctx.op == BoolOp::Union {
            return;
        }
        let (Some(target), Some(tool)) = (ctx.target, ctx.tool) else {
            self.toast("Select both Boolean objects before applying");
            return;
        };
        if self.run_boolean(ctx.op, target, tool, ctx.keep_originals) {
            self.mode = Mode::Idle;
            self.boolean_ctx = None;
            self.set_hint(format!("{} complete", ctx.op.name()));
        } else {
            self.set_hint(format!(
                "{} failed; operands kept for adjustment or retry",
                ctx.op.name()
            ));
        }
        self.bump_scene();
    }

    pub fn select_all_visible_solids(&mut self) {
        if self.mode == Mode::Mirror {
            self.toast("Mirror source objects are locked; Cancel to change them");
            return;
        }
        let boolean_op = self.boolean_ctx.as_ref().map(|ctx| ctx.op);
        let policy = select_all_policy(
            self.mode,
            boolean_op,
            self.gizmo,
            self.array_active,
            self.chamfer_active,
        );
        if let SelectAllPolicy::ClearWithNotice(reason) = policy {
            self.clear_object_selection_for_conflict(format!(
                "Ctrl+A is unavailable here because {reason}"
            ));
            return;
        }
        let align_active = self.mode == Mode::Idle && self.align_active;
        self.selected = self
            .doc
            .bodies
            .iter()
            .filter(|body| body.visible && body.kind == BodyKind::Solid && body.shape.is_some())
            .map(|body| body.id)
            .collect();
        self.face_pick = None;
        self.selected_edge = None;
        self.clear_region_selection();
        if align_active {
            // 尽量保留用户已选的 Lock；没有时以第一个可见 solid 为默认参考体。
            self.refresh_align_reference(None);
        }
        self.toast(format!("Selected {} solid objects", self.selected.len()));
        self.bump_scene();
    }

    pub fn apply_union_selected(&mut self) {
        if self.queue_model_command(("Union", "并集"), Self::apply_union_selected) {
            return;
        }
        let Some(ctx) = self.boolean_ctx.as_ref() else {
            return;
        };
        if self.mode != Mode::Boolean || ctx.op != BoolOp::Union {
            return;
        }
        let keep_originals = ctx.keep_originals;
        let mut operand_ids: Vec<BodyId> = self
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.doc.body(*id).is_some_and(|body| {
                    body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
                })
            })
            .collect();
        operand_ids.sort_unstable();
        operand_ids.dedup();
        if operand_ids.len() < 2 {
            self.toast("Select at least two visible solid objects to union");
            return;
        }

        let operand_bodies: Vec<Body> = operand_ids
            .iter()
            .filter_map(|id| self.doc.body(*id).cloned())
            .collect();
        let Some(first_shape) = operand_bodies.first().and_then(|body| body.shape) else {
            self.toast("Union: missing shape");
            return;
        };
        let mut combined = first_shape;
        for body in operand_bodies.iter().skip(1) {
            let Some(shape) = body.shape else {
                self.toast("Union: missing shape");
                return;
            };
            match self.kernel.boolean_union(combined, shape) {
                Ok(result) => combined = result,
                Err(error) => {
                    self.toast(format!("Union failed: {error}"));
                    self.set_hint(
                        "Union failed; selection kept. Adjust the objects or cancel with Esc",
                    );
                    return;
                }
            }
        }

        let result_id = self.doc.alloc_id();
        let names: Vec<String> = operand_bodies
            .iter()
            .map(|body| body.name.clone())
            .collect();
        let material = operand_bodies[0].material.clone();
        self.begin_document_change();
        if !keep_originals {
            for id in &operand_ids {
                if let Some(body) = self.doc.body_mut(*id) {
                    body.visible = false;
                }
            }
        }
        self.doc.add_body(Body {
            id: result_id,
            name: format!("Body {result_id} (union)"),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: format!(
                    "Union of {}",
                    operand_ids
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                operation: Some(DerivedOperation::Boolean {
                    op: DerivedBooleanOp::Union,
                    operands: operand_ids.clone(),
                }),
            },
            shape: Some(combined),
            material,
            visible: true,
        });
        self.selected = vec![result_id];
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.hover_body = None;
        self.boolean_ctx = None;
        self.mode = Mode::Idle;
        self.mesh_cache.clear();
        self.set_hint("Union complete");
        self.toast(format!(
            "Union complete: {} objects → 1{}",
            operand_ids.len(),
            if keep_originals {
                " (originals kept)"
            } else {
                ""
            }
        ));
        self.record_history(format!(
            "Union [{}] -> object {}{}",
            names.join(", "),
            result_id,
            if keep_originals {
                " (originals kept)"
            } else {
                ""
            }
        ));
        self.bump_scene();
    }

    fn run_boolean(
        &mut self,
        op: BoolOp,
        target: BodyId,
        tool: BodyId,
        keep_originals: bool,
    ) -> bool {
        let target_body = self.body(target).cloned();
        let tool_body = self.body(tool).cloned();
        let (Some(target_body), Some(tool_body)) = (target_body, tool_body) else {
            self.toast("Boolean: missing operand object");
            return false;
        };
        if !target_body.visible
            || !tool_body.visible
            || target_body.kind != BodyKind::Solid
            || tool_body.kind != BodyKind::Solid
        {
            self.toast("Boolean operands must be visible solids");
            return false;
        }
        let (Some(a), Some(b)) = (target_body.shape, tool_body.shape) else {
            self.toast("Boolean: missing shape");
            return false;
        };
        let result = match op {
            BoolOp::Union => self.kernel.boolean_union(a, b),
            BoolOp::Subtract => self.kernel.boolean_subtract(a, b),
            BoolOp::Intersect => self.kernel.boolean_intersect(a, b),
        };
        match result {
            Ok(shape) => {
                let id = self.doc.alloc_id();
                self.begin_document_change();
                if !keep_originals {
                    if let Some(body) = self.doc.body_mut(target) {
                        body.visible = false;
                    }
                    if let Some(body) = self.doc.body_mut(tool) {
                        body.visible = false;
                    }
                }
                self.doc.add_body(Body {
                    id,
                    name: format!("Body {id} ({})", op.name().to_lowercase()),
                    kind: BodyKind::Solid,
                    feature: Feature::Derived {
                        source: format!("{} of {} and {}", op.name(), target, tool),
                        operation: Some(DerivedOperation::Boolean {
                            op: match op {
                                BoolOp::Union => DerivedBooleanOp::Union,
                                BoolOp::Subtract => DerivedBooleanOp::Subtract,
                                BoolOp::Intersect => DerivedBooleanOp::Intersect,
                            },
                            operands: vec![target, tool],
                        }),
                    },
                    shape: Some(shape),
                    material: target_body.material,
                    visible: true,
                });
                self.selected = vec![id];
                self.clear_region_selection();
                self.face_pick = None;
                self.hover_face = None;
                self.selected_edge = None;
                self.hover_edge = None;
                self.hover_body = None;
                self.mesh_cache.clear();
                self.toast(format!(
                    "{} done{}",
                    op.name(),
                    if keep_originals {
                        " (originals kept)"
                    } else {
                        ""
                    }
                ));
                self.record_history(format!(
                    "{} \"{}\" + \"{}\" -> object {}{}",
                    op.name(),
                    target_body.name,
                    tool_body.name,
                    id,
                    if keep_originals {
                        " (originals kept)"
                    } else {
                        ""
                    }
                ));
                self.bump_scene();
                true
            }
            Err(e) => {
                self.toast(format!("Boolean failed: {e}"));
                false
            }
        }
    }

    pub fn cancel_boolean(&mut self) {
        self.boolean_ctx = None;
        self.mode = Mode::Idle;
        self.bump_scene();
    }

    // ================= 镜像 =================

    pub fn start_mirror(&mut self) {
        if self.queue_tool_switch(Self::start_mirror) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.mode == Mode::Mirror {
            self.cancel_mirror();
            return;
        }
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current operation before Mirror");
            return;
        }
        let sources: Vec<BodyId> = self
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.body(*id).is_some_and(|body| {
                    body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
                })
            })
            .collect();
        if sources.is_empty() || sources.len() != self.selected.len() {
            self.toast("Select one or more visible solid objects before Mirror");
            return;
        }

        self.deactivate_sidebar_tools();
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        self.mirror_ctx = Some(MirrorCtx {
            sources,
            plane: None,
            plane_def: None,
            plane_source: None,
            equation: [1.0, 0.0, 0.0, 0.0],
            equation_enabled: false,
            previews: Vec::new(),
        });
        self.mode = Mode::Mirror;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.set_hint("Mirror: click a planar surface to set the mirror plane");
        self.bump_scene();
    }

    pub fn begin_mirror_plane_pick(&mut self) {
        if self.mode != Mode::Mirror {
            return;
        }
        let previews = self
            .mirror_ctx
            .as_mut()
            .map(|ctx| {
                ctx.plane = None;
                ctx.plane_def = None;
                ctx.plane_source = None;
                ctx.equation_enabled = false;
                std::mem::take(&mut ctx.previews)
            })
            .unwrap_or_default();
        for (_, shape) in previews {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        self.face_pick = None;
        self.hover_face = None;
        self.set_hint("Mirror: click a planar surface to set the mirror plane");
        self.bump_scene();
    }

    pub fn set_mirror_plane(&mut self, face: FaceHit) {
        if self.mode != Mode::Mirror {
            return;
        }
        let normal = glam::DVec3::new(
            face.normal[0] as f64,
            face.normal[1] as f64,
            face.normal[2] as f64,
        );
        if !normal.is_finite() || normal.length_squared() <= 1e-12 {
            self.toast("The selected surface does not define a valid mirror plane");
            return;
        }
        let mut plane = PlaneDef::from_normal(normal.normalize().to_array());
        plane.origin = [
            face.center[0] as f64,
            face.center[1] as f64,
            face.center[2] as f64,
        ];

        self.rebuild_mirror_preview(plane, MirrorPlaneSource::Surface, Some(face));
    }

    pub fn set_mirror_datum_plane(&mut self, source: MirrorPlaneSource) {
        if self.mode != Mode::Mirror {
            return;
        }
        let normal = match source {
            MirrorPlaneSource::XY => [0.0, 0.0, 1.0],
            MirrorPlaneSource::YZ => [1.0, 0.0, 0.0],
            MirrorPlaneSource::ZX => [0.0, 1.0, 0.0],
            _ => return,
        };
        self.rebuild_mirror_preview(PlaneDef::from_normal(normal), source, None);
    }

    pub fn set_mirror_equation(&mut self, equation: [f64; 4]) {
        if self.mode != Mode::Mirror {
            return;
        }
        if let Some(ctx) = self.mirror_ctx.as_mut() {
            ctx.equation = equation;
            ctx.equation_enabled = true;
        }
        let plane = match plane_from_equation(equation) {
            Ok(plane) => plane,
            Err(error) => {
                self.clear_mirror_preview();
                if let Some(ctx) = self.mirror_ctx.as_mut() {
                    ctx.plane = None;
                    ctx.plane_def = None;
                    ctx.plane_source = None;
                    ctx.equation_enabled = true;
                }
                self.face_pick = None;
                self.set_hint(error);
                self.bump_scene();
                return;
            }
        };
        self.rebuild_mirror_preview(plane, MirrorPlaneSource::Equation, None);
        if let Some(ctx) = self.mirror_ctx.as_mut() {
            ctx.equation = equation;
            ctx.equation_enabled = true;
        }
    }

    pub fn disable_mirror_equation(&mut self) {
        if self.mode != Mode::Mirror {
            return;
        }
        let equation_was_active = self
            .mirror_ctx
            .as_ref()
            .is_some_and(|ctx| ctx.plane_source == Some(MirrorPlaneSource::Equation));
        if equation_was_active {
            self.clear_mirror_preview();
            if let Some(ctx) = self.mirror_ctx.as_mut() {
                ctx.plane = None;
                ctx.plane_def = None;
                ctx.plane_source = None;
            }
            self.face_pick = None;
        }
        if let Some(ctx) = self.mirror_ctx.as_mut() {
            ctx.equation_enabled = false;
        }
        self.set_hint("Mirror: choose a default plane or click a planar surface");
        self.bump_scene();
    }

    fn clear_mirror_preview(&mut self) {
        let previews = self
            .mirror_ctx
            .as_mut()
            .map(|ctx| std::mem::take(&mut ctx.previews))
            .unwrap_or_default();
        for (_, shape) in previews {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
    }

    fn rebuild_mirror_preview(
        &mut self,
        plane: PlaneDef,
        source: MirrorPlaneSource,
        face: Option<FaceHit>,
    ) {
        let sources = match self.mirror_ctx.as_ref() {
            Some(ctx) => ctx.sources.clone(),
            None => return,
        };
        self.clear_mirror_preview();
        if let Some(ctx) = self.mirror_ctx.as_mut() {
            ctx.plane = None;
            ctx.plane_def = None;
            ctx.plane_source = None;
            ctx.equation_enabled = source == MirrorPlaneSource::Equation;
        }
        self.face_pick = None;

        let mut previews = Vec::with_capacity(sources.len());
        for source in sources {
            let Some(shape) = self.body(source).and_then(|body| body.shape) else {
                for (_, preview) in previews {
                    self.kernel.delete(preview);
                    self.mesh_cache.remove(&preview);
                }
                self.toast("A Mirror source object is no longer available");
                return;
            };
            match self.kernel.mirror(shape, &plane) {
                Ok(preview) => previews.push((source, preview)),
                Err(error) => {
                    for (_, preview) in previews {
                        self.kernel.delete(preview);
                        self.mesh_cache.remove(&preview);
                    }
                    self.toast(format!("Mirror preview failed: {error}"));
                    return;
                }
            }
        }

        if let Some(ctx) = self.mirror_ctx.as_mut() {
            ctx.plane = face.clone();
            ctx.plane_def = Some(plane);
            ctx.plane_source = Some(source);
            ctx.equation_enabled = source == MirrorPlaneSource::Equation;
            ctx.previews = previews;
        }
        self.face_pick = face;
        self.hover_face = None;
        self.set_hint(format!(
            "Mirror {} preview ready · click Apply to create the mirrored object(s)",
            source.label()
        ));
        self.bump_scene();
    }

    pub fn apply_mirror(&mut self) {
        if self.queue_model_command(("Mirror", "镜像"), Self::apply_mirror) {
            return;
        }
        if self.mode != Mode::Mirror {
            return;
        }
        let Some(ctx) = self.mirror_ctx.take() else {
            return;
        };
        if ctx.plane_def.is_none()
            || ctx.previews.is_empty()
            || ctx.previews.len() != ctx.sources.len()
        {
            self.mirror_ctx = Some(ctx);
            self.toast("Select or define a mirror plane before applying");
            return;
        }

        let mirror_plane = ctx.plane_def.expect("validated mirror plane");
        let mirrored_count = ctx.previews.len();
        let plane_label = ctx
            .plane_source
            .map(MirrorPlaneSource::label)
            .unwrap_or("mirror plane");
        let mut new_ids = Vec::with_capacity(mirrored_count);
        self.begin_document_change();
        for (source_id, shape) in ctx.previews {
            let Some(source) = self.body(source_id).cloned() else {
                self.kernel.delete(shape);
                self.mesh_cache.remove(&shape);
                continue;
            };
            let id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id,
                name: format!("{} (mirror)", source.name),
                kind: BodyKind::Solid,
                feature: Feature::Derived {
                    source: format!("mirror of {source_id}"),
                    operation: Some(DerivedOperation::Mirror {
                        source: source_id,
                        plane: mirror_plane,
                    }),
                },
                shape: Some(shape),
                material: source.material,
                visible: true,
            });
            new_ids.push(id);
        }
        self.selected = new_ids;
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        self.mode = Mode::Idle;
        self.set_hint("Mirror complete");
        self.toast(format!(
            "Created {} mirrored object(s)",
            self.selected.len()
        ));
        self.record_history(format!(
            "Mirror {mirrored_count} object(s) across {plane_label}"
        ));
        self.bump_scene();
    }

    pub fn cancel_mirror(&mut self) {
        let previews = self
            .mirror_ctx
            .take()
            .map(|ctx| ctx.previews)
            .unwrap_or_default();
        for (_, shape) in previews {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        self.mode = Mode::Idle;
        self.set_hint("Mirror cancelled");
        self.bump_scene();
    }

    // ================= Split / Section View =================

    pub fn start_split(&mut self) {
        if self.queue_tool_switch(Self::start_split) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.mode == Mode::Split {
            self.cancel_split();
            return;
        }
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current operation before Split");
            return;
        }
        let solids: Vec<BodyId> = self
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.body(*id).is_some_and(|body| {
                    body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
                })
            })
            .collect();
        if solids.is_empty() {
            self.toast("Select one visible solid object before Split");
            return;
        }
        if solids.len() > 1 {
            self.toast("Split cuts one object; using the first selected solid");
        }
        self.deactivate_sidebar_tools();
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        let target = solids[0];
        self.selected = vec![target];
        self.split_ctx = Some(SplitCtx {
            target: Some(target),
            picking: false,
            plane: None,
            plane_def: None,
            plane_source: None,
            equation: [1.0, 0.0, 0.0, 0.0],
            equation_enabled: false,
            previews: Vec::new(),
            keep_originals: false,
        });
        self.mode = Mode::Split;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.set_hint("Split: click a planar surface to set the cutting plane");
        self.bump_scene();
    }

    pub fn begin_split_object_pick(&mut self) {
        if self.mode != Mode::Split {
            return;
        }
        if let Some(ctx) = self.split_ctx.as_mut() {
            ctx.picking = true;
        }
        self.set_hint("Split: click a solid object to split");
        self.bump_scene();
    }

    /// Split 模式点击 solid：Object 字段激活时替换目标 object（并清空旧预览）。
    pub fn split_click(&mut self, body_id: BodyId, _additive: bool) {
        let Some(mut ctx) = self.split_ctx.take() else {
            return;
        };
        if !self.body(body_id).is_some_and(|body| {
            body.visible && body.kind == BodyKind::Solid && body.shape.is_some()
        }) {
            self.toast("Split target must be a visible solid object");
            self.split_ctx = Some(ctx);
            return;
        }
        if ctx.target != Some(body_id) {
            for shape in std::mem::take(&mut ctx.previews) {
                self.kernel.delete(shape);
                self.mesh_cache.remove(&shape);
            }
            ctx.target = Some(body_id);
            ctx.plane = None;
            ctx.plane_def = None;
            ctx.plane_source = None;
            ctx.equation_enabled = false;
            ctx.previews = Vec::new();
            self.face_pick = None;
            self.selected = vec![body_id];
        }
        ctx.picking = false;
        self.split_ctx = Some(ctx);
        self.set_hint("Split: click a planar surface to set the cutting plane");
        self.bump_scene();
    }

    pub fn begin_split_plane_pick(&mut self) {
        if self.mode != Mode::Split {
            return;
        }
        let Some(mut ctx) = self.split_ctx.take() else {
            return;
        };
        ctx.plane = None;
        ctx.plane_def = None;
        ctx.plane_source = None;
        ctx.equation_enabled = false;
        for shape in std::mem::take(&mut ctx.previews) {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        self.face_pick = None;
        self.hover_face = None;
        self.split_ctx = Some(ctx);
        self.set_hint("Split: click a planar surface to set the cutting plane");
        self.bump_scene();
    }

    pub fn set_split_plane(&mut self, face: FaceHit) {
        if self.mode != Mode::Split {
            return;
        }
        let normal = glam::DVec3::new(
            face.normal[0] as f64,
            face.normal[1] as f64,
            face.normal[2] as f64,
        );
        if !normal.is_finite() || normal.length_squared() <= 1e-12 {
            self.toast("The selected surface does not define a valid cutting plane");
            return;
        }
        let mut plane = PlaneDef::from_normal(normal.normalize().to_array());
        plane.origin = [
            face.center[0] as f64,
            face.center[1] as f64,
            face.center[2] as f64,
        ];
        self.rebuild_split_preview(plane, MirrorPlaneSource::Surface, Some(face));
    }

    pub fn set_split_datum_plane(&mut self, source: MirrorPlaneSource) {
        if self.mode != Mode::Split {
            return;
        }
        let normal = match source {
            MirrorPlaneSource::XY => [0.0, 0.0, 1.0],
            MirrorPlaneSource::YZ => [1.0, 0.0, 0.0],
            MirrorPlaneSource::ZX => [0.0, 1.0, 0.0],
            _ => return,
        };
        self.rebuild_split_preview(PlaneDef::from_normal(normal), source, None);
    }

    pub fn set_split_equation(&mut self, equation: [f64; 4]) {
        if self.mode != Mode::Split {
            return;
        }
        let plane = match plane_from_equation(equation) {
            Ok(plane) => plane,
            Err(error) => {
                if let Some(ctx) = self.split_ctx.as_mut() {
                    ctx.equation = equation;
                    ctx.equation_enabled = true;
                }
                self.begin_split_plane_pick();
                if let Some(ctx) = self.split_ctx.as_mut() {
                    ctx.equation = equation;
                    ctx.equation_enabled = true;
                    ctx.picking = false;
                }
                self.set_hint(error);
                self.bump_scene();
                return;
            }
        };
        self.rebuild_split_preview(plane, MirrorPlaneSource::Equation, None);
        if let Some(ctx) = self.split_ctx.as_mut() {
            ctx.equation = equation;
            ctx.equation_enabled = true;
        }
    }

    pub fn disable_split_equation(&mut self) {
        if self.mode != Mode::Split {
            return;
        }
        let equation_was_active = self
            .split_ctx
            .as_ref()
            .is_some_and(|ctx| ctx.plane_source == Some(MirrorPlaneSource::Equation));
        if equation_was_active {
            self.begin_split_plane_pick();
        }
        if let Some(ctx) = self.split_ctx.as_mut() {
            ctx.equation_enabled = false;
        }
        self.set_hint("Split: choose a default plane or click a planar surface");
        self.bump_scene();
    }

    fn rebuild_split_preview(
        &mut self,
        plane: PlaneDef,
        source: MirrorPlaneSource,
        face: Option<FaceHit>,
    ) {
        let Some(mut ctx) = self.split_ctx.take() else {
            return;
        };
        for shape in std::mem::take(&mut ctx.previews) {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        ctx.plane = None;
        ctx.plane_def = None;
        ctx.plane_source = None;
        ctx.equation_enabled = source == MirrorPlaneSource::Equation;
        self.face_pick = None;
        self.hover_face = None;
        let Some(target) = ctx.target else {
            self.split_ctx = Some(ctx);
            self.toast("Select an object to split before choosing a plane");
            self.set_hint("Split: select an object, then choose the cutting plane");
            self.bump_scene();
            return;
        };
        let Some(shape) = self.body(target).and_then(|body| body.shape) else {
            self.split_ctx = Some(ctx);
            self.toast("The Split object is no longer available");
            self.bump_scene();
            return;
        };
        match self.kernel.split_by_plane(shape, &plane) {
            Ok(previews) => {
                let count = previews.len();
                ctx.plane = face.clone();
                ctx.plane_def = Some(plane);
                ctx.plane_source = Some(source);
                ctx.equation_enabled = source == MirrorPlaneSource::Equation;
                ctx.previews = previews;
                self.face_pick = face;
                self.split_ctx = Some(ctx);
                if count >= 2 {
                    self.set_hint(format!(
                        "Split {} preview ready · click Apply to create {count} object(s)",
                        source.label()
                    ));
                } else {
                    self.set_hint(format!(
                        "Split {} plane does not cut the object · adjust or cancel",
                        source.label()
                    ));
                }
            }
            Err(error) => {
                self.split_ctx = Some(ctx);
                self.toast(format!("Split preview failed: {error}"));
                self.set_hint("Split preview failed; adjust the cutting plane");
            }
        }
        self.bump_scene();
    }

    pub fn apply_split(&mut self) {
        if self.queue_model_command(("Split", "切分"), Self::apply_split) {
            return;
        }
        if self.mode != Mode::Split {
            return;
        }
        let Some(ctx) = self.split_ctx.take() else {
            return;
        };
        let Some(target) = ctx.target else {
            self.split_ctx = Some(ctx);
            self.toast("Select an object to split before applying");
            return;
        };
        if ctx.plane_def.is_none() || ctx.previews.is_empty() {
            self.split_ctx = Some(ctx);
            self.toast("Select or define a cutting plane before applying");
            return;
        }
        if ctx.previews.len() < 2 {
            self.split_ctx = Some(ctx);
            self.toast("The cutting plane does not cut the object into two parts");
            return;
        }
        let split_plane = ctx.plane_def.expect("validated split plane");
        let Some(source) = self.body(target).cloned() else {
            for shape in ctx.previews {
                self.kernel.delete(shape);
                self.mesh_cache.remove(&shape);
            }
            self.mode = Mode::Idle;
            self.toast("The Split object is no longer available");
            self.bump_scene();
            return;
        };
        let plane_label = ctx
            .plane_source
            .map(MirrorPlaneSource::label)
            .unwrap_or("cutting plane");
        let count = ctx.previews.len();
        let keep_originals = ctx.keep_originals;
        let mut new_ids = Vec::with_capacity(count);
        self.begin_document_change();
        if !keep_originals {
            if let Some(body) = self.doc.body_mut(target) {
                body.visible = false;
            }
        }
        for (i, shape) in ctx.previews.into_iter().enumerate() {
            let id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id,
                name: format!("{} (split {}/{})", source.name, i + 1, count),
                kind: BodyKind::Solid,
                feature: Feature::Derived {
                    source: format!("split of {target} by {plane_label}"),
                    operation: Some(DerivedOperation::Split {
                        source: target,
                        plane: split_plane,
                        piece_index: i,
                    }),
                },
                shape: Some(shape),
                material: source.material.clone(),
                visible: true,
            });
            new_ids.push(id);
        }
        self.selected = new_ids;
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.hover_body = None;
        self.mode = Mode::Idle;
        self.mesh_cache.clear();
        self.set_hint("Split complete");
        self.toast(format!(
            "Split complete: 1 object → {count} objects{}",
            if keep_originals {
                " (original kept)"
            } else {
                ""
            }
        ));
        self.record_history(format!(
            "Split \"{}\" by {plane_label} → {count} objects{}",
            source.name,
            if keep_originals {
                " (original kept)"
            } else {
                ""
            }
        ));
        self.bump_scene();
    }

    pub fn cancel_split(&mut self) {
        let previews = self
            .split_ctx
            .take()
            .map(|ctx| ctx.previews)
            .unwrap_or_default();
        for shape in previews {
            self.kernel.delete(shape);
            self.mesh_cache.remove(&shape);
        }
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        self.mode = Mode::Idle;
        self.set_hint("Split cancelled");
        self.bump_scene();
    }

    pub fn start_section_view(&mut self) {
        if self.queue_tool_switch(Self::start_section_view) {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        if self.mode == Mode::SectionView {
            self.cancel_section_view();
            return;
        }
        if self.mode != Mode::Idle {
            self.toast("Finish or cancel the current operation before Section View");
            return;
        }
        self.deactivate_sidebar_tools();
        // 默认剖面：法线 +Z 的平面穿过所有可见 solid 的世界包围盒中心，
        // 进入后立即能看到剖切效果。
        let center = self
            .visible_solid_bounds()
            .map(|(min, max)| (min + max) * 0.5)
            .unwrap_or(glam::Vec3::ZERO);
        let mut plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        plane.origin = [center.x as f64, center.y as f64, center.z as f64];
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        self.section_view_ctx = Some(SectionViewCtx {
            drag_start: None,
            plane: None,
            plane_def: Some(plane),
            plane_source: None,
            equation: [0.0, 0.0, 1.0, plane.origin[2]],
            equation_enabled: false,
            flip: false,
        });
        self.mode = Mode::SectionView;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.set_hint("Section View: cut intersecting objects; keep others whole");
        self.bump_scene();
    }

    pub fn begin_section_plane_pick(&mut self) {
        if self.mode != Mode::SectionView {
            return;
        }
        if let Some(ctx) = self.section_view_ctx.as_mut() {
            ctx.plane = None;
        }
        self.face_pick = None;
        self.set_hint("Section View: click a planar surface to set the section plane");
        self.bump_scene();
    }

    pub fn set_section_view_plane(&mut self, face: FaceHit) {
        if self.mode != Mode::SectionView {
            return;
        }
        let normal = glam::DVec3::new(
            face.normal[0] as f64,
            face.normal[1] as f64,
            face.normal[2] as f64,
        );
        if !normal.is_finite() || normal.length_squared() <= 1e-12 {
            self.toast("The selected surface does not define a valid section plane");
            return;
        }
        let mut plane = PlaneDef::from_normal(normal.normalize().to_array());
        plane.origin = [
            face.center[0] as f64,
            face.center[1] as f64,
            face.center[2] as f64,
        ];
        if let Some(ctx) = self.section_view_ctx.as_mut() {
            ctx.plane = Some(face.clone());
            ctx.plane_def = Some(plane);
            ctx.equation = section_interaction::equation(&plane);
            ctx.drag_start = None;
            ctx.plane_source = Some(MirrorPlaneSource::Surface);
            ctx.equation_enabled = false;
        }
        // Section View stores the source surface in its own context. Do not keep
        // the ordinary orange face selection overlay: it belongs to the original
        // uncut mesh and would cover the actual section result.
        self.face_pick = None;
        self.hover_face = None;
        self.set_hint("Section View: section plane set from the selected surface");
        self.bump_scene();
    }

    pub fn set_section_view_datum_plane(&mut self, source: MirrorPlaneSource) {
        if self.mode != Mode::SectionView {
            return;
        }
        let normal = match source {
            MirrorPlaneSource::XY => [0.0, 0.0, 1.0],
            MirrorPlaneSource::YZ => [1.0, 0.0, 0.0],
            MirrorPlaneSource::ZX => [0.0, 1.0, 0.0],
            _ => return,
        };
        if let Some(ctx) = self.section_view_ctx.as_mut() {
            ctx.plane = None;
            let plane = PlaneDef::from_normal(normal);
            ctx.plane_def = Some(plane);
            ctx.equation = section_interaction::equation(&plane);
            ctx.drag_start = None;
            ctx.plane_source = Some(source);
            ctx.equation_enabled = false;
        }
        self.face_pick = None;
        self.hover_face = None;
        self.set_hint("Section View: default section plane selected");
        self.bump_scene();
    }

    pub fn set_section_view_equation(&mut self, equation: [f64; 4]) {
        if self.mode != Mode::SectionView {
            return;
        }
        match plane_from_equation(equation) {
            Ok(plane) => {
                if let Some(ctx) = self.section_view_ctx.as_mut() {
                    ctx.plane = None;
                    ctx.plane_def = Some(plane);
                    ctx.plane_source = Some(MirrorPlaneSource::Equation);
                    ctx.equation = equation;
                    ctx.equation_enabled = true;
                }
                self.face_pick = None;
                self.hover_face = None;
                self.set_hint("Section View: equation plane applied");
            }
            Err(error) => {
                if let Some(ctx) = self.section_view_ctx.as_mut() {
                    ctx.equation = equation;
                    ctx.equation_enabled = true;
                    ctx.plane = None;
                    ctx.plane_def = None;
                    ctx.plane_source = None;
                }
                self.face_pick = None;
                self.set_hint(error);
            }
        }
        self.bump_scene();
    }

    pub fn disable_section_view_equation(&mut self) {
        if self.mode != Mode::SectionView {
            return;
        }
        let equation_was_active = self
            .section_view_ctx
            .as_ref()
            .is_some_and(|ctx| ctx.plane_source == Some(MirrorPlaneSource::Equation));
        if let Some(ctx) = self.section_view_ctx.as_mut() {
            ctx.equation_enabled = false;
            if equation_was_active {
                ctx.plane_def = None;
                ctx.plane_source = None;
                ctx.plane = None;
            }
        }
        self.face_pick = None;
        self.set_hint("Section View: choose a default plane or click a planar surface");
        self.bump_scene();
    }

    pub fn set_section_view_flip(&mut self, flip: bool) {
        if self.mode != Mode::SectionView {
            return;
        }
        if let Some(ctx) = self.section_view_ctx.as_mut() {
            ctx.flip = flip;
        }
        self.bump_scene();
    }

    pub fn cancel_section_view(&mut self) {
        self.section_view_ctx = None;
        self.face_pick = None;
        self.hover_face = None;
        self.hover_body = None;
        self.mode = Mode::Idle;
        self.set_hint("Section View exited");
        self.bump_scene();
    }

    // ================= 删除/复制 =================

    /// 删除一个参数化 chamfer/fillet 节点，并把它的所有后继重新接到前一基体。
    /// 几何全部成功重建后才修改 Document，避免删除失败留下半条特征链。
    fn delete_chamfer_operation(&mut self, id: BodyId) -> Result<usize, String> {
        let (target, replacement_base, rebuild_order) = chamfer_delete_plan(&self.doc, id)?;
        let replacement_shape = self
            .doc
            .body(replacement_base)
            .and_then(|body| body.shape)
            .ok_or_else(|| "Cannot restore the operation base".to_string())?;

        let updates = self
            .evaluate_chamfer_rebuilds(replacement_base, replacement_shape, &rebuild_order)
            .map_err(|error| format!("Cannot remove this operation: {error}"))?;

        self.begin_document_change();
        let _ = self.doc.remove_body(id);
        for (child_id, new_base, shape, edges, reference_bounds) in &updates {
            if let Some(body) = self.doc.body_mut(*child_id) {
                body.shape = Some(*shape);
                if let Feature::Chamfer {
                    base,
                    edges: stored_edges,
                    reference_bounds: stored_bounds,
                    ..
                } = &mut body.feature
                {
                    *base = Some(*new_base);
                    *stored_edges = edges.clone();
                    *stored_bounds = *reference_bounds;
                }
            }
        }

        let has_visible_result = self.doc.bodies.iter().any(|body| {
            body.visible
                && matches!(
                    &body.feature,
                    Feature::Chamfer { target: child_target, .. } if *child_target == target
                )
        });
        if !has_visible_result {
            if let Some(body) = self.doc.body_mut(replacement_base) {
                body.visible = true;
            }
        }
        let next_selection = active_chamfer_body(&self.doc, &[], target).or_else(|| {
            self.doc
                .body(replacement_base)
                .filter(|body| body.visible)
                .map(|body| body.id)
        });
        self.selected = next_selection.into_iter().collect();
        self.selected_edge = None;
        self.hover_edge = None;
        self.chamfer_committed = false;
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.bump_scene();
        Ok(updates.len())
    }

    fn delete_surface_texture_operation(&mut self, id: BodyId) -> Result<BodyId, String> {
        let source = match self.doc.body(id).map(|body| &body.feature) {
            Some(Feature::SurfaceTexture { source, .. }) => *source,
            _ => return Err("Selected object is not a surface texture operation".into()),
        };
        if self.doc.body(source).is_none() {
            return Err("Cannot restore the surface texture source".into());
        }
        self.begin_document_change();
        let _ = self.doc.remove_body(id);
        if let Some(source_body) = self.doc.body_mut(source) {
            source_body.visible = true;
        }
        self.selected = vec![source];
        self.face_pick = None;
        self.selected_edge = None;
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.bump_scene();
        Ok(source)
    }

    pub fn delete_selected(&mut self) {
        if self.mode == Mode::OffsetShell {
            self.toast("Finish or cancel Offset / Shell before deleting objects");
            return;
        }
        if self.mode == Mode::Mirror {
            self.toast("Mirror source objects are locked; Cancel to delete them");
            return;
        }
        if self.mode == Mode::Split {
            self.toast("Finish or cancel Split before deleting objects");
            return;
        }
        if self.mode == Mode::SectionView {
            self.toast("Section View does not modify objects; exit it before deleting");
            return;
        }
        if self.mode == Mode::Insert3D {
            self.toast("Finish or exit Draw 3D before deleting objects");
            return;
        }
        if self.mode == Mode::Sketch {
            if let Some(sk) = &mut self.sketch {
                if sk.delete_selected() {
                    self.bump_scene();
                }
            }
            return;
        }
        if self.selected.is_empty() {
            return;
        }
        if self.selected.len() == 1
            && self
                .doc
                .body(self.selected[0])
                .is_some_and(|body| matches!(body.feature, Feature::Chamfer { .. }))
        {
            let id = self.selected[0];
            match self.delete_chamfer_operation(id) {
                Ok(rebuilt) => {
                    self.toast(if rebuilt == 0 {
                        "Chamfer/Fillet operation removed".to_string()
                    } else {
                        format!(
                            "Chamfer/Fillet operation removed; {rebuilt} later operation(s) rebuilt"
                        )
                    });
                    self.record_history(format!("Delete chamfer operation {id}"));
                }
                Err(error) => self.toast(error),
            }
            return;
        }
        if self.selected.len() == 1
            && self
                .doc
                .body(self.selected[0])
                .is_some_and(|body| matches!(body.feature, Feature::SurfaceTexture { .. }))
        {
            let id = self.selected[0];
            match self.delete_surface_texture_operation(id) {
                Ok(source) => {
                    self.toast("Surface texture removed; source object restored");
                    self.record_history(format!("Delete surface texture {id}; restore {source}"));
                }
                Err(error) => self.toast(error),
            }
            return;
        }
        self.begin_document_change();
        for id in self.selected.clone() {
            // 注意：不删除 shape——undo 快照仍引用它（GC 后续优化）
            let _ = self.doc.remove_body(id);
        }
        self.selected.clear();
        self.bump_scene();
        self.toast("Deleted");
        self.record_history("Delete selection");
    }

    /// Apply the captured menu targets, rather than a later live selection.
    pub fn delete_bodies_from_objects(&mut self, ids: &[BodyId]) {
        let ids = self.existing_object_ids(ids);
        if ids.len() == 1 {
            self.delete_body_from_objects(ids[0]);
            return;
        }
        if ids.is_empty() {
            return;
        }
        if !matches!(self.mode, Mode::Idle | Mode::Boolean) {
            self.toast(self.settings.language.text(
                "Finish the current tool before deleting objects",
                "请先结束当前工具，再删除对象",
            ));
            return;
        }
        self.begin_document_change();
        for &id in &ids {
            let _ = self.doc.remove_body(id);
            self.clear_unavailable_object_references(id);
        }
        self.bump_scene();
        self.record_history(format!("Delete {} objects", ids.len()));
        self.toast(
            self.settings
                .language
                .text("Selected objects deleted", "所选对象已删除"),
        );
    }

    /// Single-object menu deletion retains feature-operation cancellation.
    pub fn delete_body_from_objects(&mut self, id: BodyId) {
        if self.mode == Mode::Mirror {
            self.toast("Mirror source objects are locked; Cancel to delete them");
            return;
        }
        if self.mode == Mode::Split {
            self.toast("Finish or cancel Split before deleting objects");
            return;
        }
        if self.mode == Mode::SectionView {
            self.toast("Exit Section View before deleting objects");
            return;
        }
        if self.mode == Mode::Insert3D {
            self.toast("Finish or exit Draw 3D before deleting objects");
            return;
        }
        if self.mode == Mode::Sketch {
            self.toast("Exit Sketch before deleting an object");
            return;
        }
        let Some(body) = self.doc.body(id).cloned() else {
            return;
        };
        if matches!(body.feature, Feature::Chamfer { .. }) {
            match self.delete_chamfer_operation(id) {
                Ok(rebuilt) => {
                    self.toast(if rebuilt == 0 {
                        "Chamfer/Fillet operation removed".to_string()
                    } else {
                        format!(
                            "Chamfer/Fillet operation removed; {rebuilt} later operation(s) rebuilt"
                        )
                    });
                    self.record_history(format!("Delete chamfer operation {id}"));
                }
                Err(error) => self.toast(error),
            }
            return;
        }
        if matches!(body.feature, Feature::SurfaceTexture { .. }) {
            match self.delete_surface_texture_operation(id) {
                Ok(source) => {
                    self.toast("Surface texture removed; source object restored");
                    self.record_history(format!("Delete surface texture {id}; restore {source}"));
                }
                Err(error) => self.toast(error),
            }
            return;
        }
        self.begin_document_change();
        let _ = self.doc.remove_body(id);
        self.selected.retain(|selected| *selected != id);
        if self.hover_body == Some(id) {
            self.hover_body = None;
        }
        if self
            .face_pick
            .as_ref()
            .is_some_and(|face| face.body_id == id)
        {
            self.face_pick = None;
        }
        if self
            .selected_edge
            .as_ref()
            .is_some_and(|edge| edge.body_id == id)
        {
            self.selected_edge = None;
        }
        self.bump_scene();
        self.toast(format!("Deleted {}", body.name));
        self.record_history(format!("Delete object {id}: {}", body.name));
    }

    pub fn duplicate_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let ids = self.selected.clone();
        let mut copies = Vec::new();
        for id in ids {
            let Some(body) = self.body(id).cloned() else {
                continue;
            };
            let Some(shape) = body.shape else { continue };
            let mat = glam::Mat4::from_translation(glam::Vec3::new(20.0, 0.0, 0.0));
            if let Ok(new_shape) = self.kernel.transform(shape, &mat) {
                copies.push((body, new_shape));
            }
        }
        if copies.is_empty() {
            self.toast("Duplicate failed");
            return;
        }
        // One user command, one history transaction, regardless of count.
        self.begin_document_change();
        let mut new_ids = Vec::new();
        for (body, new_shape) in copies {
            let source_id = body.id;
            let nid = self.doc.alloc_id();
            self.doc.add_body(Body {
                id: nid,
                name: format!("{} (copy)", body.name),
                kind: body.kind,
                feature: Feature::Derived {
                    source: format!("copy of {source_id}"),
                    operation: Some(DerivedOperation::Copy {
                        source: source_id,
                        offset: [20.0, 0.0, 0.0],
                        primitive_snapshot: copy_primitive_snapshot(&body.feature, [20., 0., 0.]),
                    }),
                },
                shape: Some(new_shape),
                material: body.material,
                visible: true,
            });
            new_ids.push(nid);
        }
        self.selected = new_ids;
        self.bump_scene();
        self.toast("Duplicated");
        self.record_history("Duplicate selection");
    }

    // ================= undo/redo =================

    pub fn sync_sketch_history(&mut self) {
        if self.mode != Mode::Sketch {
            self.sketch_history_current = None;
            self.sketch_undo_stack.clear();
            self.sketch_redo_stack.clear();
            return;
        }
        let Some(current) = self.sketch.as_ref().map(SketchSession::snapshot) else {
            return;
        };
        match self.sketch_history_current.replace(current.clone()) {
            None => {}
            Some(previous) if previous != current => {
                let history_message = if self
                    .sketch
                    .as_ref()
                    .is_some_and(|s| s.tool == SketchTool::Trim)
                {
                    Some("Sketch trim".to_string())
                } else if current.entities.len() > previous.entities.len() {
                    let names = current.entities[previous.entities.len()..]
                        .iter()
                        .map(|entity| entity.kind())
                        .collect::<Vec<_>>()
                        .join(", ");
                    Some(format!("Sketch draw: {names}"))
                } else if current.entities.len() < previous.entities.len() {
                    Some(format!(
                        "Sketch delete: {} object(s)",
                        previous.entities.len() - current.entities.len()
                    ))
                } else {
                    None
                };
                self.sketch_undo_stack.push(previous);
                if self.sketch_undo_stack.len() > Document::MAX_UNDO_STEPS {
                    let excess = self.sketch_undo_stack.len() - Document::MAX_UNDO_STEPS;
                    self.sketch_undo_stack.drain(0..excess);
                }
                self.sketch_redo_stack.clear();
                if let Some(message) = history_message {
                    self.record_history(message);
                }
            }
            Some(_) => {}
        }
    }

    fn reset_sketch_interaction_after_history(&mut self) {
        let Some(sketch) = self.sketch.as_mut() else {
            return;
        };
        sketch.reset_after_history_restore();
    }

    fn document_history_available(&self) -> bool {
        self.mode == Mode::Idle
            || (self.mode == Mode::Insert3D
                && self.insert_3d_ctx.as_ref().is_some_and(|context| {
                    context.params.is_none() && context.preview_shape.is_none()
                }))
    }

    pub fn can_undo(&self) -> bool {
        if self.mode == Mode::Sketch {
            return !self.sketch_undo_stack.is_empty()
                || self
                    .sketch
                    .as_ref()
                    .is_some_and(|sketch| !sketch.pending.is_empty());
        }
        self.document_history_available() && self.doc.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        if self.mode == Mode::Sketch {
            return !self.sketch_redo_stack.is_empty();
        }
        self.document_history_available() && self.doc.can_redo()
    }

    fn sanitize_after_document_history(&mut self) {
        self.selected
            .retain(|id| self.doc.body(*id).is_some_and(|body| body.visible));
        self.clear_region_selection();
        self.face_pick = None;
        self.hover_face = None;
        self.selected_edge = None;
        self.hover_edge = None;
        self.hover_body = None;
        self.gizmo_drag = None;
        self.rotate_custom_axis = None;
        self.rotate_axis_pick_active = false;
        self.chamfer_committed = false;
        self.regions_cache.clear();
        self.mesh_cache.clear();
        self.section_cache.clear();
    }

    pub fn undo(&mut self) -> bool {
        if self.mode == Mode::Sketch {
            // A first point is only an unfinished command, so Undo cancels it
            // without consuming a committed geometry history entry.
            if self
                .sketch
                .as_ref()
                .is_some_and(|sketch| !sketch.pending.is_empty())
            {
                self.reset_sketch_interaction_after_history();
                self.bump_scene();
                return true;
            }
            let Some(previous) = self.sketch_undo_stack.pop() else {
                return false;
            };
            if let Some(sketch) = self.sketch.as_mut() {
                self.sketch_redo_stack.push(sketch.snapshot());
                sketch.restore_snapshot(previous.clone());
                self.sketch_history_current = Some(previous);
                self.reset_sketch_interaction_after_history();
                self.record_history("Undo Sketch edit");
                if !self.sync_active_sketch_edit() {
                    self.bump_scene();
                }
                return true;
            }
            return false;
        }
        if !self.document_history_available() {
            self.toast("Finish or cancel the current tool before Undo");
            return false;
        }
        let current_selection = self.selected.clone();
        if self.doc.undo() {
            self.redo_selection_stack.push(current_selection);
            if let Some(previous_selection) = self.undo_selection_stack.pop() {
                self.selected = previous_selection;
            }
            self.sanitize_after_document_history();
            self.record_history("Undo");
            self.toast("Undo");
            self.bump_scene();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if self.mode == Mode::Sketch {
            let Some(next) = self.sketch_redo_stack.pop() else {
                return false;
            };
            if let Some(sketch) = self.sketch.as_mut() {
                self.sketch_undo_stack.push(sketch.snapshot());
                if self.sketch_undo_stack.len() > Document::MAX_UNDO_STEPS {
                    let excess = self.sketch_undo_stack.len() - Document::MAX_UNDO_STEPS;
                    self.sketch_undo_stack.drain(0..excess);
                }
                sketch.restore_snapshot(next.clone());
                self.sketch_history_current = Some(next);
                self.reset_sketch_interaction_after_history();
                self.record_history("Redo Sketch edit");
                if !self.sync_active_sketch_edit() {
                    self.bump_scene();
                }
                return true;
            }
            return false;
        }
        if !self.document_history_available() {
            self.toast("Finish or cancel the current tool before Redo");
            return false;
        }
        let current_selection = self.selected.clone();
        if self.doc.redo() {
            self.undo_selection_stack.push(current_selection);
            if let Some(next_selection) = self.redo_selection_stack.pop() {
                self.selected = next_selection;
            }
            self.sanitize_after_document_history();
            self.record_history("Redo");
            self.toast("Redo");
            self.bump_scene();
            true
        } else {
            false
        }
    }

    // ================= 视图 =================

    pub fn set_view(&mut self, view: crate::camera::ViewDir) {
        self.camera.set_view(view);
    }

    pub fn toggle_projection(&mut self) -> bool {
        self.camera.toggle_projection()
    }

    pub fn recenter_view(&mut self) {
        // Use document visibility, not the kernel arena (which also holds
        // hidden inputs, deleted objects and undo/preview geometry).
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        let mut include = |p: glam::Vec3| {
            if p.is_finite() {
                min = min.min(p);
                max = max.max(p);
            }
        };
        for body in self.doc.bodies.iter().filter(|b| b.visible) {
            if let Some((a, z)) = body
                .shape
                .and_then(|shape| self.kernel.tessellate(shape).ok())
                .and_then(|mesh| mesh.bounds())
                .or_else(|| equation_curve::bounds(&body.feature))
            {
                include(a);
                include(z);
            }
            if let Feature::Sketch(sketch) = &body.feature {
                for entity in &sketch.entities {
                    for (points, _) in entity.to_chains() {
                        for p in points {
                            include(glam::Vec3::from_array(
                                sketch.plane.to_world(p[0], p[1]).map(|v| v as f32),
                            ));
                        }
                    }
                }
            }
        }
        if !min.is_finite() {
            min = glam::Vec3::splat(-50.0);
            max = glam::Vec3::splat(50.0);
        }
        let aspect = self.viewport_rect.width() / self.viewport_rect.height();
        self.camera.recenter(min, max, aspect);
        self.bump_scene();
    }

    pub fn fit_view(&mut self) {
        let mut bounds = self.kernel.bounds();
        for b in self.doc.bodies.iter().filter(|b| b.visible) {
            if let Some((a, z)) = equation_curve::bounds(&b.feature) {
                bounds = Some(bounds.map_or((a, z), |(min, max)| (min.min(a), max.max(z))));
            }
        }
        let (min, max) = bounds.unwrap_or((glam::Vec3::ZERO, glam::Vec3::splat(200.0)));
        let center = (min + max) / 2.0;
        let radius = (max - min).length() / 2.0;
        // 保证从斜上方观察：若视线朝上（forward.y > 0，即从下方仰视），
        // 则把视线翻转到上方。注意：正常俯视时 forward.y 为负。
        let fwd = self.camera.forward();
        if fwd.y > 0.01 {
            let mirrored = glam::Vec3::new(fwd.x, -fwd.y, fwd.z).normalize();
            let dist = self.camera.distance().max(radius * 2.0);
            // eye 在视线反方向
            self.camera.eye = center - mirrored * dist;
            self.camera.target = center;
            self.camera.up = glam::Vec3::Y;
        }
        self.camera.fit(center, radius);
    }

    // ================= 场景组装 =================

    /// 每帧：若场景版本变化，重建 CPU 侧 SceneData。
    pub fn refresh_scene(&mut self) {
        if self.scene_version == self.applied_version {
            return;
        }
        let appearance = self.settings.appearance;
        let palette = appearance.palette();
        let theme_line = |original: [f32; 3], replacement: egui::Color32| {
            if appearance.is_original() {
                original
            } else {
                crate::appearance::rgb(replacement)
            }
        };
        let mut scene = SceneData::default();
        scene.show_grid = self.show_grid;
        scene.camera = Some(self.camera.clone());
        scene.version = self.scene_version;

        // Layered CAD grid based on Super3D: normal modeling uses the world XZ
        // plane; sketching switches to a local grid on the active sketch plane.
        // PickPlane mode still hides it so the three datum choices stay clear.
        if self.show_grid && self.mode != Mode::PickPlane {
            let (origin, u, v, normal, sketch_grid) = if let Some(sk) = &self.sketch {
                (
                    v3(sk.plane.origin()),
                    v3(sk.plane.x_axis()),
                    v3(sk.plane.y_axis()),
                    v3(sk.plane.normal()),
                    true,
                )
            } else {
                (
                    glam::Vec3::ZERO,
                    glam::Vec3::X,
                    glam::Vec3::Z,
                    glam::Vec3::Y,
                    false,
                )
            };
            // A tiny camera-facing lift avoids z-fighting on sketch-on-face.
            let grid_origin =
                origin + normal.normalize_or_zero() * if sketch_grid { 0.025 } else { 0.0 };
            // The grid is based on the active Step. Switching Unit resets Step
            // to one of that unit, while zoom only promotes it by a readable
            // 1/2/5 multiple. Thus grid, snapping and displayed units agree.
            let base_step = self.snap_step.max(1e-6) as f32;
            let target_world = (self.world_per_pixel_at(grid_origin) * 28.0).max(base_step);
            let ratio = (target_world / base_step).max(1.0);
            let magnitude = 10.0_f32.powf(ratio.log10().floor());
            let normalized = ratio / magnitude;
            let nice = if normalized <= 1.0 {
                1.0
            } else if normalized <= 2.0 {
                2.0
            } else if normalized <= 5.0 {
                5.0
            } else {
                10.0
            };
            let grid_step = base_step * magnitude * nice;
            let view_half = self.world_per_pixel_at(grid_origin)
                * self.viewport_rect.width().max(self.viewport_rect.height())
                * 0.85;
            let half_size = view_half.max(grid_step * 20.0).min(grid_step * 500.0);
            let (minor, major) = make_grid_lines(
                grid_origin.to_array(),
                u.to_array(),
                v.to_array(),
                half_size,
                grid_step,
                5,
            );
            scene.grid_minor_lines = SceneLines {
                positions: minor,
                color: theme_line(
                    if sketch_grid {
                        [0.18, 0.20, 0.23]
                    } else {
                        [0.17, 0.18, 0.20]
                    },
                    palette.grid_minor,
                ),
            };
            scene.grid_major_lines = SceneLines {
                positions: major,
                color: theme_line(
                    if sketch_grid {
                        [0.24, 0.26, 0.31]
                    } else {
                        [0.25, 0.26, 0.30]
                    },
                    palette.grid_major,
                ),
            };
            if sketch_grid {
                // Use the same signed XYZ bindings as the Sketch Align panel.
                // Plane-local U/V stay internal; positive colored directions
                // always agree with the world Gizmo, including U=-Z/V=-Z.
                if let Some(sketch) = self.sketch.as_ref() {
                    let colors = [[0.88, 0.42, 0.46], [0.48, 0.79, 0.44], [0.38, 0.63, 0.94]];
                    for displayed_axis in sketch.displayed_world_axes() {
                        let local_direction = u * displayed_axis.local_direction[0] as f32
                            + v * displayed_axis.local_direction[1] as f32;
                        scene.grid_axis_lines.push(SceneLines {
                            positions: vec![
                                (grid_origin - local_direction * half_size).to_array(),
                                grid_origin.to_array(),
                            ],
                            color: theme_line([0.28, 0.30, 0.35], palette.grid_major),
                        });
                        scene.grid_axis_lines.push(SceneLines {
                            positions: vec![
                                grid_origin.to_array(),
                                (grid_origin + local_direction * half_size).to_array(),
                            ],
                            color: colors[displayed_axis.world_axis],
                        });
                    }
                } else {
                    scene.grid_axis_lines.push(SceneLines {
                        positions: vec![
                            (grid_origin - u * half_size).to_array(),
                            (grid_origin + u * half_size).to_array(),
                            (grid_origin - v * half_size).to_array(),
                            (grid_origin + v * half_size).to_array(),
                        ],
                        color: theme_line([0.39, 0.44, 0.54], palette.grid_major),
                    });
                }
            } else {
                // A fitted view of a small first object can produce a grid
                // half-size below 120 mm. `f32::clamp` panics when min > max,
                // so collapse the lower bound to the available grid extent.
                let axis_min = 120.0_f32.min(half_size);
                let axis_len = (grid_step * 6.0).clamp(axis_min, half_size);
                for (dir, color) in [
                    (glam::Vec3::X, [0.88, 0.42, 0.46]),
                    (glam::Vec3::Y, [0.48, 0.79, 0.44]),
                    (glam::Vec3::Z, [0.38, 0.63, 0.94]),
                ] {
                    scene.grid_axis_lines.push(SceneLines {
                        positions: vec![origin.to_array(), (origin + dir * axis_len).to_array()],
                        color,
                    });
                }
            }
        }
        // 基准面（PickPlane 模式）：三个同尺寸的半透明平面在原点组成清晰的
        // 三维参考角。配色对应平面法线轴：YZ→红(X)、ZX→绿(Y)、XY→蓝(Z)。
        if self.mode == Mode::PickPlane {
            let wpp = self.world_per_pixel_at(glam::Vec3::ZERO);
            // 约占视口高度 30%，既易点击，也不会压迫顶部提示与视角控件。
            let h = ((self.viewport_rect.height() * 0.30 * wpp).clamp(80.0, 560.0)) as f64;
            self.plane_pick_size = h as f32;
            self.plane_block_size = h as f32;

            // 顺序与面板按钮一致：XY(0) / YZ(1) / ZX(2)
            // 三个 h×h 方块在原点相接，构成"墙角"：绿（地面 y=0）、蓝（z=0）、红（x=0）
            let specs: [(DatumPlane, [f32; 3], f64); 3] = [
                (DatumPlane::Front, [0.38, 0.63, 0.94], h), // XY 蓝方块：z=0，x∈[0,h]，y∈[0,h]
                (DatumPlane::Right, [0.88, 0.42, 0.46], h), // YZ 红方块：x=0，z∈[0,h]，y∈[0,h]
                (DatumPlane::Top, [0.48, 0.79, 0.44], h),   // XZ 绿方块：y=0，x∈[0,h]，z∈[0,h]
            ];
            for (i, (plane, color, half)) in specs.iter().enumerate() {
                let hovered = self.plane_pick_hover == Some(i);
                let def = plane.def();
                let half = *half;
                let corners = match plane {
                    // XY 蓝方块：u=x∈[0,h]，v=y∈[0,h]
                    DatumPlane::Front => [
                        def.to_world(0.0, 0.0),
                        def.to_world(half, 0.0),
                        def.to_world(half, half),
                        def.to_world(0.0, half),
                    ],
                    // YZ 红方块：u=-z∈[-h,0]，v=y∈[0,h]
                    DatumPlane::Right => [
                        def.to_world(-half, 0.0),
                        def.to_world(0.0, 0.0),
                        def.to_world(0.0, half),
                        def.to_world(-half, half),
                    ],
                    // XZ 绿方块：u=x∈[0,h]，v=-z∈[-h,0]
                    DatumPlane::Top => [
                        def.to_world(0.0, 0.0),
                        def.to_world(half, 0.0),
                        def.to_world(half, -half),
                        def.to_world(0.0, -half),
                    ],
                };
                let positions: Vec<[f32; 3]> = corners
                    .iter()
                    .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
                    .collect();
                scene.planes.push(ScenePlane {
                    positions,
                    indices: vec![0, 1, 2, 0, 2, 3],
                    color: *color,
                    opacity: if hovered { 0.24 } else { 0.09 },
                });
                // 外框：非悬停状态压低亮度，悬停时恢复对应轴色。
                let mut border = SceneLines {
                    positions: Vec::new(),
                    color: if hovered {
                        *color
                    } else {
                        [color[0] * 0.68, color[1] * 0.68, color[2] * 0.68]
                    },
                };
                for k in 0..4 {
                    let a = corners[k];
                    let b = corners[(k + 1) % 4];
                    border
                        .positions
                        .push([a[0] as f32, a[1] as f32, a[2] as f32]);
                    border
                        .positions
                        .push([b[0] as f32, b[1] as f32, b[2] as f32]);
                }
                scene.plane_borders.push(border);

                // 轻微内缩的第二道框线让平面看起来像可点击的设计卡片。
                let center = [
                    corners.iter().map(|p| p[0]).sum::<f64>() * 0.25,
                    corners.iter().map(|p| p[1]).sum::<f64>() * 0.25,
                    corners.iter().map(|p| p[2]).sum::<f64>() * 0.25,
                ];
                let inset: Vec<[f32; 3]> = corners
                    .iter()
                    .map(|p| {
                        [
                            (p[0] + (center[0] - p[0]) * 0.055) as f32,
                            (p[1] + (center[1] - p[1]) * 0.055) as f32,
                            (p[2] + (center[2] - p[2]) * 0.055) as f32,
                        ]
                    })
                    .collect();
                let mut inner = SceneLines {
                    positions: Vec::new(),
                    color: if hovered {
                        [
                            (color[0] * 1.08).min(1.0),
                            (color[1] * 1.08).min(1.0),
                            (color[2] * 1.08).min(1.0),
                        ]
                    } else {
                        [color[0] * 0.46, color[1] * 0.46, color[2] * 0.46]
                    },
                };
                for k in 0..4 {
                    inner.positions.push(inset[k]);
                    inner.positions.push(inset[(k + 1) % 4]);
                }
                scene.plane_borders.push(inner);
            }

            // 原点处的 XYZ 轴与右下角 gizmo 使用同一配色，强化空间方向感。
            scene.plane_lines.positions.clear();
            let axis_len = h as f32 * 0.34;
            for (end, color) in [
                ([axis_len, 0.0, 0.0], [0.88, 0.42, 0.46]),
                ([0.0, axis_len, 0.0], [0.48, 0.79, 0.44]),
                ([0.0, 0.0, axis_len], [0.38, 0.63, 0.94]),
            ] {
                scene.plane_borders.push(SceneLines {
                    positions: vec![[0.0, 0.0, 0.0], end],
                    color,
                });
            }
        }

        // bodies：先收集 solid 数据（避免循环内可变借用）
        let boolean_preview = (self.mode == Mode::Boolean)
            .then_some(self.boolean_ctx.as_ref())
            .flatten();
        let split_preview = (self.mode == Mode::Split)
            .then_some(self.split_ctx.as_ref())
            .flatten();
        let split_preview_replaces_target = split_preview
            .filter(|ctx| ctx.previews.len() >= 2)
            .and_then(|ctx| ctx.target);
        // Section View 剖切平面（None = 剖面未定，按普通方式渲染）。
        let section_plane = if self.mode == Mode::SectionView {
            self.section_view_ctx.as_ref().and_then(|ctx| ctx.plane_def)
        } else {
            None
        };
        let section_flip = self.section_view_ctx.as_ref().is_some_and(|ctx| ctx.flip);
        // 原位编辑 Draw 3D body 时显示参数预览，暂时隐藏原 shape，避免重叠闪烁。
        // 若参数暂时无效而没有 preview，则保留原 body 可见。
        let draw_3d_editing = if self.mode == Mode::Insert3D {
            self.insert_3d_ctx.as_ref().and_then(|ctx| {
                ctx.preview_shape
                    .is_some()
                    .then_some(ctx.editing_body)
                    .flatten()
            })
        } else {
            None
        };
        let solids: Vec<(BodyId, ShapeId, [f32; 3], bool, u8)> = self
            .doc
            .bodies
            .iter()
            .filter(|b| {
                b.visible
                    && matches!(b.kind, BodyKind::Solid | BodyKind::Surface)
                    && draw_3d_editing != Some(b.id)
            })
            // 有有效 Split 预览时不再在相同坐标叠画源 object；旧的双层透明
            // 渲染会把内部三角面误显示为“漏模/多余面”。
            .filter(|b| split_preview_replaces_target != Some(b.id))
            .map(|b| {
                // 0=normal, 1=Union member, 2=Original, 3=Subtracting, 4=Intersecting.
                let preview_role = match boolean_preview {
                    Some(ctx) if ctx.op == BoolOp::Union && self.selected.contains(&b.id) => 1,
                    Some(ctx) if ctx.target == Some(b.id) => 2,
                    Some(ctx) if ctx.tool == Some(b.id) && ctx.op == BoolOp::Subtract => 3,
                    Some(ctx) if ctx.tool == Some(b.id) && ctx.op == BoolOp::Intersect => 4,
                    _ => match split_preview {
                        // Split 目标 object 沿用 Original 的青色角色色。
                        Some(ctx) if ctx.target == Some(b.id) => 2,
                        _ => 0,
                    },
                };
                let highlight = preview_role != 0
                    || self.selected.contains(&b.id)
                    || self.hover_body == Some(b.id);
                (
                    b.id,
                    b.shape.unwrap_or(ShapeId(u64::MAX)),
                    match preview_role {
                        // Union operands share one preview color so the pending
                        // multi-object operation reads as a single group.
                        1 => [0.10, 0.50, 0.92],
                        // Pair operations use stable role colors in both the
                        // viewport and the corresponding Properties inputs.
                        2 => [0.04, 0.68, 0.66],
                        3 => [0.96, 0.14, 0.08],
                        4 => [0.70, 0.20, 0.94],
                        _ => [
                            b.material.color[0],
                            b.material.color[1],
                            b.material.color[2],
                        ],
                    },
                    highlight,
                    preview_role,
                )
            })
            .collect();
        let surface_sketch_plane = (self.mode == Mode::Sketch && self.sketch_on_body.is_some())
            .then(|| self.sketch.as_ref().map(|sketch| sketch.plane))
            .flatten();
        let mut surface_reference_points = Vec::new();
        for (_id, shape, color, highlight, preview_role) in solids {
            if self
                .offset_ctx
                .as_ref()
                .is_some_and(|ctx| ctx.source == _id && ctx.preview.is_some())
                || self
                    .surface_ctx
                    .as_ref()
                    .is_some_and(|ctx| ctx.editing == Some(_id) && ctx.preview.is_some())
            {
                continue;
            }
            if shape == ShapeId(u64::MAX) {
                continue;
            }
            let mesh = self.mesh_for(shape);
            // Section View：切掉法线一侧（Flip 可切换保留侧），保留侧正常着色，
            // 切口封盖用同色系略深的纯色面表示，不叠加三角网格。
            if self.mode == Mode::SectionView {
                if let Some(plane) = section_plane {
                    if let Some(split) = section_interaction::crosses(&mesh, &plane)
                        .then(|| self.section_split(shape, &plane))
                        .flatten()
                    {
                        let keep_positive = !section_flip;
                        let kept = if keep_positive {
                            &split.positive
                        } else {
                            &split.negative
                        };
                        if kept.is_empty() {
                            continue;
                        }
                        // 把侧壁与切口封盖拆成互不重叠的 draw calls。旧实现先画
                        // 完整 half（已经含 cap），又在同一深度重复画一层反绕序
                        // cap，导致 z-fighting、闪烁和透面伪影。
                        let (side_indices, cap_indices) =
                            section_surface_and_cap_indices(kept, &plane);
                        if !side_indices.is_empty() {
                            // Manifold's split output has shared rim vertices.
                            // Plain vertex_normals() averages top/wall/cut edges
                            // and creates radial lighting spikes. Rebuild CAD
                            // smoothing groups after the cut so hard edges stay
                            // hard and curved faces remain smooth.
                            let surface = MeshData::new(kept.positions.clone(), side_indices)
                                .weld_hard(1e-5, 35.0);
                            scene.meshes.push(SceneMesh {
                                positions: surface.positions.clone(),
                                normals: surface.vertex_normals(),
                                indices: surface.indices.clone(),
                                color,
                                highlight,
                            });
                        }
                        // 封盖只画一次，保留 manifold 输出的正确绕序。
                        if !cap_indices.is_empty() {
                            let caps = MeshData::new(kept.positions.clone(), cap_indices)
                                .weld_hard(1e-5, 35.0);
                            let cut_fill = section_cut_face_color(color);
                            scene.meshes.push(SceneMesh {
                                positions: caps.positions.clone(),
                                normals: caps.vertex_normals(),
                                indices: caps.indices.clone(),
                                color: cut_fill,
                                highlight: false,
                            });
                        }
                        // 保留半体的特征轮廓。
                        if kept.positions.len() <= 200_000 {
                            let mut outline = Vec::new();
                            for (a, b) in kept.sharp_edges(25.0) {
                                outline.push(kept.positions[a]);
                                outline.push(kept.positions[b]);
                            }
                            if !outline.is_empty() {
                                scene.body_edge_lines.push(SceneLines {
                                    positions: outline,
                                    color: if highlight {
                                        theme_line([0.78, 0.49, 0.19], palette.outline)
                                    } else {
                                        if appearance
                                            == crate::appearance::UiAppearance::PorcelainWorkshop
                                        {
                                            crate::appearance::rgb(egui::Color32::from_rgb(
                                                108, 121, 140,
                                            ))
                                        } else {
                                            [0.30, 0.33, 0.38]
                                        }
                                    },
                                });
                            }
                        }
                        continue;
                    }
                }
            }
            if self.mode == Mode::Sketch {
                // Sketch 中现有实体只作为空间参照：浅色半透明面 + 强化轮廓。
                // 背向相机的锐边改画虚线，退出 Sketch 后自然恢复普通实体渲染。
                scene.planes.push(ScenePlane {
                    positions: mesh.positions.clone(),
                    indices: mesh.indices.clone(),
                    color: if appearance == crate::appearance::UiAppearance::PorcelainWorkshop {
                        crate::appearance::rgb(egui::Color32::from_rgb(87, 110, 143))
                    } else {
                        [0.78, 0.86, 0.96]
                    },
                    opacity: 0.18,
                });
                let (visible_edges, hidden_edges) =
                    sketch_reference_edges(&mesh, self.camera.eye, 22.0);
                if let Some(plane) = surface_sketch_plane {
                    for edge in visible_edges.chunks_exact(2) {
                        let a = edge[0].map(|value| value as f64);
                        let b = edge[1].map(|value| value as f64);
                        let midpoint = [
                            (a[0] + b[0]) * 0.5,
                            (a[1] + b[1]) * 0.5,
                            (a[2] + b[2]) * 0.5,
                        ];
                        for world in [a, b, midpoint] {
                            // Sketch 相机正视当前平面且使用正交投影，因此 to_local
                            // 正好是用户看到的 3D 轮廓在草图平面上的投影位置。
                            let (u, v) = plane.to_local(world);
                            surface_reference_points.push([u, v]);
                        }
                    }
                }
                if !visible_edges.is_empty() {
                    scene.body_edge_lines.push(SceneLines {
                        positions: visible_edges,
                        color: if appearance == crate::appearance::UiAppearance::PorcelainWorkshop {
                            crate::appearance::rgb(egui::Color32::from_rgb(88, 114, 148))
                        } else {
                            [0.72, 0.82, 0.95]
                        },
                    });
                }
                if !hidden_edges.is_empty() {
                    scene.body_edge_lines.push(SceneLines {
                        positions: hidden_edges,
                        color: if appearance == crate::appearance::UiAppearance::PorcelainWorkshop {
                            crate::appearance::rgb(egui::Color32::from_rgb(152, 167, 185))
                        } else {
                            [0.43, 0.52, 0.66]
                        },
                    });
                }
                continue;
            }
            scene.meshes.push(SceneMesh {
                positions: mesh.positions.clone(),
                normals: mesh.vertex_normals(),
                indices: mesh.indices.clone(),
                color,
                highlight,
            });
            // Super3D-style feature outlines: keep unselected edges subdued,
            // while selected/hovered bodies get a clear orange contour.
            if mesh.positions.len() <= 200_000 {
                let mut positions = Vec::new();
                for (a, b) in mesh.sharp_edges(25.0) {
                    positions.push(mesh.positions[a]);
                    positions.push(mesh.positions[b]);
                }
                if !positions.is_empty() {
                    scene.body_edge_lines.push(SceneLines {
                        positions,
                        color: match preview_role {
                            1 => [0.22, 0.72, 1.0],
                            2 => [0.08, 0.96, 0.90],
                            3 => [1.0, 0.30, 0.20],
                            4 => [0.86, 0.38, 1.0],
                            _ if highlight => theme_line([0.78, 0.49, 0.19], palette.outline),
                            _ => {
                                if appearance == crate::appearance::UiAppearance::PorcelainWorkshop
                                {
                                    crate::appearance::rgb(egui::Color32::from_rgb(108, 121, 140))
                                } else {
                                    [0.30, 0.33, 0.38]
                                }
                            }
                        },
                    });
                }
            }
        }
        if let Some(sketch) = self.sketch.as_mut() {
            sketch.set_external_key_points(surface_reference_points);
        }

        // Sketch lines follow Super3D's pale-blue drafting color on the dark viewport.
        scene.sketch_lines.color = theme_line([0.75, 0.89, 1.0], palette.sketch_line);
        scene.world_sketch_lines.color = scene.sketch_lines.color;
        // sketch bodies：画草图线
        // When editing a persistent Sketch, the live SketchSession is the only
        // renderer for that body. Drawing the stored body as well leaves a
        // solid duplicate behind every drag preview.
        let editing_sketch_body = (self.mode == Mode::Sketch)
            .then_some(self.sketch_edit_body)
            .flatten();
        for body in &self.doc.bodies {
            if !body.visible
                || body.kind != BodyKind::Sketch
                || editing_sketch_body == Some(body.id)
            {
                continue;
            }
            if let Some(sk) = body.sketch() {
                let mut lines = Vec::new();
                for (points, closed) in sk.entities.iter().flat_map(|entity| entity.to_chains()) {
                    append_chain_world64(&mut lines, &points, &sk.plane, closed);
                }
                scene.world_sketch_lines.positions.extend(lines);
            }
        }

        // 当前草图会话（正式线、Super3D 风格橙色虚线预览、选中高亮）
        if let Some(sk) = &self.sketch {
            let active_drag_entity = sk.active_drag_entity();
            let visible_entities: Vec<_> = sk
                .entities
                .iter()
                .enumerate()
                .filter(|(index, _)| Some(*index) != active_drag_entity)
                .map(|(_, entity)| entity.clone())
                .collect();
            let lines = sketch_lines(&SketchFeature {
                vertex_locks: Default::default(),
                plane: sk.plane.clone(),
                entities: visible_entities,
                world_xyz_coordinates: sk.world_xyz_coordinates,
                on_body: self.sketch_on_body,
            });
            scene.sketch_lines.positions.extend(lines);
            if let Some(plan) = &sk.trim_preview {
                for (points, closed) in plan.removed.to_chains() {
                    append_chain_world(
                        &mut scene.highlight_lines.positions,
                        &points,
                        &sk.plane,
                        closed,
                        0.3,
                    );
                }
                scene.highlight_lines.color = [1.0, 0.25, 0.12];
            }
            scene.sketch_preview_lines.color = theme_line([0.94, 0.63, 0.25], palette.outline);
            // 绘制中的预览（4 mm dash / 3 mm gap，与 Super3D 一致）
            if let Some((pts, closed)) = sk.drawing_preview() {
                append_dashed_chain_world(
                    &mut scene.sketch_preview_lines.positions,
                    &pts,
                    &sk.plane,
                    closed,
                    0.3,
                );
            }
            // Direct key-point/object dragging has two distinct cues: the
            // original geometry becomes a subdued dashed ghost, while the live
            // moved/resized result is a solid orange preview. This avoids the
            // old pair of overlapping solid objects during edit-sketch drags.
            if let Some(index) = active_drag_entity {
                if let Some(original) = sk.entities.get(index) {
                    let mut ghost = Vec::new();
                    for (points, closed) in original.to_chains() {
                        append_dashed_chain_world(&mut ghost, &points, &sk.plane, closed, 0.3);
                    }
                    scene.body_edge_lines.push(SceneLines {
                        positions: ghost,
                        color: theme_line([0.49, 0.61, 0.75], palette.grid_major),
                    });
                }
                if let Some(preview) = &sk.edit_preview {
                    for (points, closed) in preview.to_chains() {
                        append_chain_world(
                            &mut scene.highlight_lines.positions,
                            &points,
                            &sk.plane,
                            closed,
                            0.3,
                        );
                    }
                    scene.highlight_lines.color = theme_line([0.94, 0.58, 0.18], palette.outline);
                }
            // Parameter editing still uses the established orange dashed
            // preview until Apply.
            } else if let Some(prev) = &sk.edit_preview {
                for (pts, closed) in prev.to_chains() {
                    append_dashed_chain_world(
                        &mut scene.sketch_preview_lines.positions,
                        &pts,
                        &sk.plane,
                        closed,
                        0.3,
                    );
                }
            }
            for (points, closed) in sk.transform_preview_chains() {
                append_dashed_chain_world(
                    &mut scene.sketch_preview_lines.positions,
                    &points,
                    &sk.plane,
                    closed,
                    0.3,
                );
            }
            // A custom Sketch Rotate pivot is the in-plane point pierced by
            // the plane-normal rotation axis. Keep it visible independently
            // of the general sketch-points toggle.
            let mut pivots = Vec::new();
            if sk.tool == SketchTool::Rotate {
                pivots.extend(
                    sk.transform_selection
                        .iter()
                        .filter_map(|&idx| sk.locked_vertex_point(idx)),
                );
                if sk.rotate_custom_pivot && pivots.len() < sk.transform_selection.len() {
                    pivots.push(sk.rotate_pivot);
                }
            } else if let Some(point) = sk
                .selected_entity
                .and_then(|index| sk.locked_vertex_point(index))
            {
                pivots.push(point);
            }
            for pivot in pivots {
                let normal = v3(sk.plane.normal()).normalize_or_zero();
                let world = v3(glam::DVec3::from_array(
                    sk.plane.to_world(pivot[0], pivot[1]),
                )) + normal * 0.5;
                let radius = (self.world_per_pixel_at(world) * 5.0).max(0.1);
                scene.rotation_axis_lines.color = [0.97, 0.19, 0.22];
                append_sketch_point_marker(
                    &mut scene.rotation_axis_lines.positions,
                    world,
                    v3(sk.plane.x_axis()).normalize_or_zero(),
                    v3(sk.plane.y_axis()).normalize_or_zero(),
                    radius,
                );
            }
            // Pick/Align/Move/Rotate 均支持多选；Align 固定参考实体额外使用红色轮廓。
            let selected_entities: Vec<usize> = match sk.tool {
                SketchTool::Align => sk.align_selection.clone(),
                SketchTool::Move | SketchTool::Rotate => sk.transform_selection.clone(),
                _ => sk.selected_entities(),
            };
            for idx in selected_entities {
                if Some(idx) == active_drag_entity {
                    continue;
                }
                if let Some(e) = sk.entities.get(idx) {
                    let mut hl = Vec::new();
                    for (pts, closed) in e.to_chains() {
                        append_chain_world(&mut hl, &pts, &sk.plane, closed, 0.3);
                    }
                    if sk.tool == SketchTool::Align && sk.align_reference == Some(idx) {
                        scene.rotation_axis_lines.positions.extend(hl);
                        scene.rotation_axis_lines.color = [0.96, 0.16, 0.20];
                    } else {
                        scene.highlight_lines.positions.extend(hl);
                        scene.highlight_lines.color =
                            theme_line([0.94, 0.58, 0.18], palette.outline);
                    }
                }
            }
            if self.sketch_points_visible {
                scene.sketch_point_lines.color = [0.97, 0.19, 0.22];
                let x_axis = v3(sk.plane.x_axis()).normalize_or_zero();
                let y_axis = v3(sk.plane.y_axis()).normalize_or_zero();
                let normal = v3(sk.plane.normal()).normalize_or_zero();
                for point in sk.key_points() {
                    let world = v3(glam::DVec3::from_array(
                        sk.plane.to_world(point[0], point[1]),
                    )) + normal * 0.5;
                    let radius = (self.world_per_pixel_at(world) * 3.2).max(0.08);
                    append_sketch_point_marker(
                        &mut scene.sketch_point_lines.positions,
                        world,
                        x_axis,
                        y_axis,
                        radius,
                    );
                }
            }
        }

        self.offset_scene(&mut scene);
        self.curve_scene(&mut scene);
        self.surface_scene(&mut scene);

        // Extrude ghost previews（半透明蓝色）
        if self.mode == Mode::Extrude {
            let previews = self
                .extrude_ctx
                .as_ref()
                .map(|ctx| ctx.ghost_shapes.clone())
                .unwrap_or_default();
            for gs in previews {
                if let Ok(mesh) = self.kernel.tessellate(gs) {
                    let positions = mesh.positions.clone();
                    let indices = mesh.indices.clone();
                    scene.planes.push(ScenePlane {
                        positions,
                        indices,
                        color: [0.47, 0.67, 0.94],
                        opacity: 0.35,
                    });
                }
            }
        }

        // Mirror 预览：预览 shape 尚未写入 Document，只在工作区用半透明蓝色显示。
        let mirror_previews: Vec<ShapeId> = if self.mode == Mode::Mirror {
            self.mirror_ctx
                .as_ref()
                .map(|ctx| ctx.previews.iter().map(|(_, shape)| *shape).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for shape in mirror_previews {
            if let Ok(mesh) = self.kernel.tessellate(shape) {
                scene.planes.push(ScenePlane {
                    positions: mesh.positions.clone(),
                    indices: mesh.indices.clone(),
                    color: [0.18, 0.63, 0.96],
                    opacity: 0.42,
                });
                let mut outline = Vec::new();
                for (a, b) in mesh.sharp_edges(25.0) {
                    outline.push(mesh.positions[a]);
                    outline.push(mesh.positions[b]);
                }
                if !outline.is_empty() {
                    scene.body_edge_lines.push(SceneLines {
                        positions: outline,
                        color: [0.25, 0.76, 1.0],
                    });
                }
            }
        }

        // Split 预览：法线侧/反向侧切件用两种半透明色区分，Apply 前不写入 Document。
        let split_previews: Vec<ShapeId> = if self.mode == Mode::Split {
            self.split_ctx
                .as_ref()
                .map(|ctx| ctx.previews.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for (i, shape) in split_previews.iter().enumerate() {
            if let Ok(mesh) = self.kernel.tessellate(*shape) {
                let color = if i % 2 == 0 {
                    [0.22, 0.66, 0.95]
                } else {
                    [0.96, 0.55, 0.20]
                };
                scene.meshes.push(SceneMesh {
                    positions: mesh.positions.clone(),
                    normals: mesh.vertex_normals(),
                    indices: mesh.indices.clone(),
                    color,
                    highlight: true,
                });
                let mut outline = Vec::new();
                for (a, b) in mesh.sharp_edges(25.0) {
                    outline.push(mesh.positions[a]);
                    outline.push(mesh.positions[b]);
                }
                if !outline.is_empty() {
                    scene.body_edge_lines.push(SceneLines {
                        positions: outline,
                        color,
                    });
                }
            }
        }

        // Split 显示半透明切割平面；Section View 只保留边框，避免平面填充
        // 遮住剖面或与切口封盖争夺深度。
        let cutting_plane = match self.mode {
            Mode::Split => self.split_ctx.as_ref().and_then(|ctx| ctx.plane_def),
            Mode::SectionView => self.section_view_ctx.as_ref().and_then(|ctx| ctx.plane_def),
            _ => None,
        };
        if let Some(plane) = cutting_plane {
            let half = self
                .visible_solid_bounds()
                .map(|(min, max)| ((max - min).length() * 0.55).max(40.0))
                .unwrap_or(120.0);
            if self.mode == Mode::Split {
                append_plane_quad(&mut scene, &plane, half as f32);
            } else {
                append_plane_border(&mut scene, &plane, half as f32);
            }
        }

        // Draw 3D 当前基础体预览；Done/Exit 前不写入 Document。
        if self.mode == Mode::Insert3D {
            if let Some(shape) = self
                .insert_3d_ctx
                .as_ref()
                .and_then(|ctx| ctx.preview_shape)
            {
                if let Ok(mesh) = self.kernel.tessellate(shape) {
                    scene.preview_surfaces.push(ScenePlane {
                        positions: mesh.positions.clone(),
                        indices: mesh.indices.clone(),
                        color: [0.20, 0.68, 0.50],
                        opacity: 0.88,
                    });
                    let mut outline = Vec::new();
                    for (start, end) in draw_3d_preview_edges(&mesh, &self.camera) {
                        append_solid_world_segment_width(
                            &mut outline,
                            start,
                            end,
                            self.camera.forward(),
                            self.world_per_pixel_at((start + end) * 0.5),
                            1,
                        );
                    }
                    if !outline.is_empty() {
                        scene.body_edge_lines.push(SceneLines {
                            positions: outline,
                            color: [0.52, 0.98, 0.76],
                        });
                    }
                }
            }
            let handle_params = self.insert_3d_ctx.as_ref().and_then(|ctx| {
                (ctx.stage == Draw3DPlacementStage::Precision).then_some(ctx.params?)
            });
            if let Some(params) = handle_params {
                let camera_right = self.camera.right();
                let camera_up = self.camera.up.normalize_or_zero();
                for (kind, point) in draw_3d_control_points(&params) {
                    let world = glam::Vec3::from_array(point.map(|value| value as f32));
                    let radius = (self.world_per_pixel_at(world)
                        * if kind == Draw3DDragKind::Move {
                            5.2
                        } else {
                            4.2
                        })
                    .max(0.08);
                    if kind == Draw3DDragKind::Move {
                        scene.rotation_axis_lines.color = [0.16, 0.63, 0.96];
                        append_sketch_point_marker(
                            &mut scene.rotation_axis_lines.positions,
                            world,
                            camera_right,
                            camera_up,
                            radius,
                        );
                    } else {
                        scene.sketch_point_lines.color = [0.97, 0.19, 0.22];
                        append_sketch_point_marker(
                            &mut scene.sketch_point_lines.positions,
                            world,
                            camera_right,
                            camera_up,
                            radius,
                        );
                    }
                }
            }
        }

        // 选中的 3D 表面：橙色半透明面 + 加粗橙色边界。
        if let Some(face) = self.face_pick.clone() {
            if let Some(shape) = self.body(face.body_id).and_then(|b| b.shape) {
                let mesh = self.mesh_for(shape);
                let mut positions: Vec<[f32; 3]> = Vec::new();
                let mut indices: Vec<u32> = Vec::new();
                let face_center = glam::Vec3::from_array(face.center);
                let toward_camera = (self.camera.eye - face_center).normalize_or_zero();
                let off =
                    toward_camera * (self.world_per_pixel_at(face_center) * 0.35).clamp(0.01, 1.0);
                for &vi in &face.triangles {
                    indices.push(positions.len() as u32);
                    let p = glam::Vec3::from_array(mesh.positions[vi as usize]) + off;
                    positions.push(p.to_array());
                }
                scene.planes.push(ScenePlane {
                    positions,
                    indices,
                    color: [0.98, 0.49, 0.10],
                    opacity: 0.38,
                });

                scene.edge_lines.color = theme_line([1.0, 0.48, 0.06], palette.outline);
                for (a, b) in face_patch_boundary_edges(&face.triangles) {
                    let start = glam::Vec3::from_array(mesh.positions[a as usize]) + off;
                    let end = glam::Vec3::from_array(mesh.positions[b as usize]) + off;
                    append_solid_world_segment_width(
                        &mut scene.edge_lines.positions,
                        start,
                        end,
                        self.camera.forward(),
                        self.world_per_pixel_at((start + end) * 0.5),
                        2,
                    );
                }
            }
        }

        // 悬停识别的 3D 表面（浅色提示，双击选中）
        if let Some(face) = self.hover_face.clone() {
            if face_pick_none_or_different(&self.face_pick, &face) {
                if let Some(shape) = self.body(face.body_id).and_then(|b| b.shape) {
                    let mesh = self.mesh_for(shape);
                    let mut positions: Vec<[f32; 3]> = Vec::new();
                    let mut indices: Vec<u32> = Vec::new();
                    let off = glam::Vec3::from_array(face.normal) * 0.15;
                    for &vi in &face.triangles {
                        indices.push(positions.len() as u32);
                        let p = glam::Vec3::from_array(mesh.positions[vi as usize]) + off;
                        positions.push(p.to_array());
                    }
                    scene.planes.push(ScenePlane {
                        positions,
                        indices,
                        color: [0.62, 0.78, 0.96],
                        opacity: 0.22,
                    });
                }
            }
        }

        // 棱选择层：悬停只给轻量提示，真正选中用 3 px 橙色实线。
        // 所有线仍参与深度测试，不会穿透实体显示背面棱。
        if let Some(edge) = &self.hover_edge {
            if self.selected_edge.as_ref() != Some(edge) {
                let positions = edge
                    .segments
                    .iter()
                    .flat_map(|segment| [segment.a, segment.b])
                    .collect();
                scene.body_edge_lines.push(SceneLines {
                    positions,
                    color: [0.94, 0.68, 0.31],
                });
            }
        }
        if let Some(edge) = &self.selected_edge {
            scene.edge_lines.color = theme_line([1.0, 0.46, 0.04], palette.outline);
            for segment in &edge.segments {
                let start = glam::Vec3::from_array(segment.a);
                let end = glam::Vec3::from_array(segment.b);
                append_solid_world_segment_width(
                    &mut scene.edge_lines.positions,
                    start,
                    end,
                    self.camera.forward(),
                    self.world_per_pixel_at((start + end) * 0.5),
                    3,
                );
            }
        }

        // Rotate：世界轴为橙色实线；拾取的空间轴以原始线段范围显示为红色虚线。
        if self.align_active && self.axis_align.enabled {
            let point = glam::DVec3::from_array(self.axis_align.point).as_vec3();
            let direction = glam::DVec3::from_array(self.axis_align.direction)
                .as_vec3()
                .normalize_or_zero();
            if point.is_finite() && direction.is_finite() && direction.length_squared() > 0.5 {
                let span = self
                    .selected_bounds()
                    .map(|(a, b)| (b - a).length().max(50.))
                    .unwrap_or(100.);
                scene.rotation_axis_lines.color = [0.96, 0.16, 0.20];
                append_dashed_world_segment_2px(
                    &mut scene.rotation_axis_lines.positions,
                    point - direction * span,
                    point + direction * span,
                    self.camera.forward(),
                    self.world_per_pixel_at(point),
                );
            }
        }
        if self.gizmo == Some(GizmoMode::Rotate) {
            if let Some(axis) = self.rotate_custom_axis {
                scene.rotation_axis_lines.color = [0.96, 0.16, 0.20];
                append_dashed_world_segment_2px(
                    &mut scene.rotation_axis_lines.positions,
                    glam::Vec3::from_array(axis.display_start),
                    glam::Vec3::from_array(axis.display_end),
                    self.camera.forward(),
                    self.world_per_pixel_at(
                        (glam::Vec3::from_array(axis.display_start)
                            + glam::Vec3::from_array(axis.display_end))
                            * 0.5,
                    ),
                );
            } else if !self.rotate_axis_pick_active {
                if let Some((point, direction)) = self.rotation_axis_for_selection() {
                    let span = self
                        .selected_bounds()
                        .map(|(min, max)| (max - min).length().max(20.0) * 0.75)
                        .unwrap_or(50.0);
                    scene.edge_lines.color = theme_line([0.94, 0.58, 0.18], palette.outline);
                    scene.edge_lines.positions.extend_from_slice(&[
                        (point - direction * span).to_array(),
                        (point + direction * span).to_array(),
                    ]);
                }
            }
        }

        // 选中 sketch body 的区域高亮（边框线）。Pick、highlight 与
        // Extrude 必须使用同一份 Region 缓存及索引，尤其是在 Undo 恢复
        // Sketch 后，不能各自重新 polygonize 再解释 selected_regions。
        if self.selected.len() == 1 && self.mode != Mode::Sketch {
            let sketch = self.profile_sketch(self.selected[0]);
            if let Some(sk) = sketch {
                let regions = self.regions_of(self.selected[0]);
                for (i, r) in regions.iter().enumerate() {
                    let sel = self.selected_regions_for(self.selected[0]).contains(&i);
                    let mut lines = Vec::new();
                    append_ring(&mut lines, &r.outer, &sk.plane);
                    for h in &r.holes {
                        append_ring(&mut lines, h, &sk.plane);
                    }
                    if sel {
                        if let Ok(mesh) = self.validated_region_mesh(self.selected[0], i) {
                            let mut positions = Vec::with_capacity(mesh.triangles.len() * 3);
                            for triangle in &mesh.triangles {
                                for point in triangle {
                                    positions.push(sk.plane.to_world(point[0], point[1]));
                                }
                            }
                            scene.region_surfaces.push(SceneRegionSurface {
                                indices: (0..positions.len() as u32).collect(),
                                positions,
                                color: theme_line([0.94, 0.58, 0.18], palette.outline),
                                opacity: 0.45,
                            });
                        }
                        // Validation failure deliberately keeps the complete outline.
                        scene.region_lines.positions.extend(lines);
                        scene.region_lines.color = theme_line([0.94, 0.58, 0.18], palette.outline);
                    } else {
                        scene.world_sketch_lines.positions.extend(lines);
                    }
                }
            }
        }

        *self.scene.lock().unwrap() = scene;
        self.applied_version = self.scene_version;
    }

    fn mesh_for(&mut self, shape: ShapeId) -> MeshData {
        if let Some(m) = self.mesh_cache.get(&shape) {
            return m.clone();
        }
        if let Ok(m) = self.kernel.tessellate(shape) {
            // 焊接重复顶点 + 按面夹角分组平滑法线（CAD 硬边）：
            // 弧面条带间（约 5°）法线平滑，顶/底面与侧面（90°）保持硬边，
            // 避免全平滑导致的边缘软阴影环
            // Exact meshes already have analytic surface normals and a stable
            // triangle->topology map. Welding can remove triangles and breaks
            // that identity; it also replaces analytic normals with averages.
            let m = if self.kernel.exact_shape(shape).is_some() {
                m
            } else {
                let thread = self
                    .doc
                    .bodies
                    .iter()
                    .filter(|b| b.shape == Some(shape))
                    .find_map(|b| match &b.feature {
                        Feature::Derived { source, .. } => source
                            .strip_prefix("Thread v1: ")
                            .and_then(|s| serde_json::from_str::<crate::threads::Saved>(s).ok())
                            .map(|saved| (b.id, saved)),
                        _ => None,
                    });
                let m = if let Some((id, saved)) = thread
                    .filter(|(_, saved)| saved.mesh_signature == crate::threads::mesh_signature(&m))
                {
                    let bands = crate::threads::remaining_targets(
                        id,
                        &saved.remaining,
                        &saved.annotations,
                        &m,
                    );
                    crate::threads::split_band_boundaries(m, &bands)
                } else {
                    m
                };
                m.weld_hard(1e-5, 35.0)
            };
            self.mesh_cache.insert(shape, m.clone());
            return m;
        }
        MeshData::default()
    }

    pub fn shape_measurements(
        &mut self,
        shape: ShapeId,
    ) -> Option<(glam::Vec3, glam::Vec3, f64, f64)> {
        if let Some(cached) = self.measurement_cache.get(&shape) {
            return Some(*cached);
        }
        let mesh = self.kernel.tessellate(shape).ok()?;
        let (min, max) = mesh.bounds()?;
        let (area, volume) = crate::ui::mesh_surface_area_and_volume(&mesh);
        let value = (min, max, area, volume);
        self.measurement_cache.insert(shape, value);
        Some(value)
    }

    /// Section View 剖切：(shape, 平面) → 两半网格（带缓存）。
    /// shape 不可变，平面键按法线/偏移的位模式生成，缓存长期有效；
    /// 超过上限时整体清空重建。
    fn section_split(&mut self, shape: ShapeId, plane: &PlaneDef) -> Option<CachedSectionSplit> {
        let normal = plane.normal();
        let key = [
            normal.x.to_bits(),
            normal.y.to_bits(),
            normal.z.to_bits(),
            plane.origin().dot(normal).to_bits(),
        ];
        if let Some(hit) = self.section_cache.get(&(shape, key)) {
            return hit.intersects.then(|| hit.clone());
        }
        // Sectioning is a topology operation, so it must start from the
        // kernel's raw tessellation. `mesh_for` is a render cache whose
        // `weld_hard` pass intentionally duplicates vertices at shading-group
        // boundaries; feeding that display mesh back into Manifold became
        // unreliable once Boolean results were promoted to faceted B-reps.
        let mesh = self.kernel.tessellate(shape).ok()?;
        if mesh.is_empty() {
            return None;
        }
        let (positive, negative) =
            s33d_kernel::mesh_bool::split_by_plane(&mesh, plane.origin, normal.to_array()).ok()?;
        if self.section_cache.len() > 128 {
            self.section_cache.clear();
        }
        // A plane between disconnected shells crosses the aggregate bounds but
        // does not cut any material. Both halves must have actual cut caps.
        let intersects = !positive.is_empty()
            && !negative.is_empty()
            && !section_surface_and_cap_indices(&positive, plane)
                .1
                .is_empty()
            && !section_surface_and_cap_indices(&negative, plane)
                .1
                .is_empty();
        let cached = CachedSectionSplit {
            intersects,
            positive,
            negative,
        };
        self.section_cache.insert((shape, key), cached.clone());
        cached.intersects.then_some(cached)
    }

    // ================= 视口交互 =================

    /// 视口指针事件路由（由 ui.rs 调用）。
    pub fn viewport_pointer(&mut self, response: &egui::Response) {
        if self.tool_exit_prompt.is_some() {
            return;
        }
        if self.section_pointer(response) {
            return;
        }
        let rect = response.rect;
        let pos = response.hover_pos();
        // gizmo 变换拖拽（左键，仅在 Idle + gizmo 激活 + 有选中体时）
        if self.gizmo.is_some() && self.mode == Mode::Idle && !self.selected.is_empty() {
            self.gizmo_pointer(response);
            // gizmo 拖拽期间不处理其它指针
            if self.gizmo_drag.is_some() {
                let scroll = response.ctx.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    self.camera.zoom(scroll / 100.0);
                    self.bump_scene();
                }
                return;
            }
        }
        // Sketch direct manipulation uses a real press-drag-release lifecycle.
        // Completed entities can be moved/resized both in Pick and while a
        // drawing tool remains active; an unfinished construction is rejected
        // by SketchSession::begin_entity_drag.
        if self.mode == Mode::Sketch && response.drag_started_by(egui::PointerButton::Primary) {
            let start = response
                .ctx
                .input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            if let Some(start) = start {
                let additive = response.ctx.input(|input| {
                    input.modifiers.shift || input.modifiers.command || input.modifiers.ctrl
                });
                self.sketch_pointer_down_at(start, rect, additive, true);
                // Apply the distance already travelled while egui was deciding
                // whether this was a click or a drag.
                if let Some(current) = response.interact_pointer_pos() {
                    self.on_pointer_move(current, rect);
                }
            }
        }
        // Draw 3D Precision uses the same press-drag-release idea as Sketch:
        // drag the translucent preview to move it, or a red key point to resize.
        if self.mode == Mode::Insert3D
            && self
                .insert_3d_ctx
                .as_ref()
                .is_some_and(|ctx| ctx.stage == Draw3DPlacementStage::Precision)
        {
            // Arm on press, before egui's drag-distance threshold. Expensive
            // geometry previews may skip the frame carrying drag_started;
            // the press snapshot must survive until release, including on
            // successive drags of the same preview.
            let captured = self
                .insert_3d_ctx
                .as_ref()
                .is_some_and(|ctx| ctx.direct_drag.is_some());
            if !captured
                && ((response.is_pointer_button_down_on()
                    && response.ctx.input(|i| i.pointer.primary_down()))
                    || response.drag_started_by(egui::PointerButton::Primary))
            {
                let start = response
                    .ctx
                    .input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
                if let Some(start) = start {
                    if self.begin_draw_3d_direct_drag(start, rect) {
                        if let Some(current) = response.interact_pointer_pos() {
                            self.update_draw_3d_direct_drag(current, rect);
                        }
                        return;
                    }
                }
            } else if captured && response.ctx.input(|input| input.pointer.primary_down()) {
                if let Some(current) = response.interact_pointer_pos() {
                    if self.update_draw_3d_direct_drag(current, rect) {
                        return;
                    }
                }
            } else if response.drag_stopped_by(egui::PointerButton::Primary)
                && self.finish_draw_3d_direct_drag()
            {
                return;
            }
        }
        // Extrude and Pocket share a draft; dragging never commits it.
        if self.mode == Mode::Extrude {
            if response.drag_started_by(egui::PointerButton::Primary) {
                if let (Some(pos), Some(ctx)) = (
                    response.ctx.input(|i| i.pointer.press_origin()),
                    &self.extrude_ctx,
                ) {
                    self.extrude_drag_start = Some((pos, ctx.depth));
                    response.ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
            }
            if let (Some((start, depth)), Some(current), Some(ctx)) = (
                self.extrude_drag_start,
                response.interact_pointer_pos(),
                self.extrude_ctx.as_ref(),
            ) {
                let center = self.body_center(ctx.body_id);
                let offset = self.normal_drag_distance(
                    current - start,
                    ctx.sketch.plane.normal().as_vec3(),
                    center,
                );
                let next = (depth + offset).max(0.1);
                if (ctx.depth - next).abs() > 1e-9 {
                    self.set_extrude_depth(next);
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.extrude_drag_start = None;
            }
        }
        // 更新相机拖拽
        if response.dragged_by(egui::PointerButton::Middle) {
            let delta = response.drag_delta();
            if self.cam_drag.is_none() {
                self.cam_drag = Some(CamDrag::Orbit {
                    last: pos.unwrap_or(rect.center()),
                });
            }
            self.camera.orbit(delta.x, delta.y, 0.008);
            self.bump_scene();
        } else if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            self.camera.pan(delta.x, delta.y);
            self.bump_scene();
        } else {
            self.cam_drag = None;
        }
        // 缩放
        let scroll = response.ctx.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 {
            self.camera.zoom(scroll / 100.0);
            self.bump_scene();
        }
        // 左键点击
        if response.clicked() {
            self.on_left_click(response);
        }
    }

    /// gizmo 左键拖拽：开始/更新/结束变换。
    fn gizmo_pointer(&mut self, response: &egui::Response) {
        let Some(gizmo) = self.gizmo else { return };
        if gizmo == GizmoMode::Rotate && self.rotate_axis_pick_active {
            return;
        }
        // 开始拖拽
        if response.drag_started_by(egui::PointerButton::Primary) {
            let items: Vec<GizmoDragItem> = self
                .selected
                .iter()
                .copied()
                .filter_map(|body_id| {
                    self.body(body_id)
                        .and_then(|body| body.shape)
                        .map(|start_shape| GizmoDragItem {
                            body_id,
                            start_shape,
                            temp_shape: None,
                        })
                })
                .collect();
            if items.is_empty() {
                return;
            }
            let Some(pivot) = self.selected_center() else {
                return;
            };
            let Some(pos) = response.interact_pointer_pos() else {
                return;
            };
            self.gizmo_drag = Some(GizmoDrag {
                items,
                pivot,
                last_pos: pos,
                mat: glam::Mat4::IDENTITY,
            });
            return;
        }
        // 拖拽中
        if response.dragged_by(egui::PointerButton::Primary) {
            let Some(drag) = self.gizmo_drag.clone() else {
                return;
            };
            let Some(pos) = response.interact_pointer_pos() else {
                return;
            };
            let delta = pos - drag.last_pos;
            if delta == egui::Vec2::ZERO {
                return;
            }
            if drag.mat == glam::Mat4::IDENTITY {
                self.begin_document_change();
            }
            let step = self.gizmo_step_matrix(drag.pivot, gizmo, delta);
            let mat = step * drag.mat;
            let mut items = drag.items;
            let mut changed = false;
            // 拖拽中对每个 object 使用网格级变换（快），结束时统一 B-rep 最终化。
            for item in &mut items {
                let new_shape = if let Ok(mesh) = self.kernel.tessellate(item.start_shape) {
                    let mut mesh = mesh;
                    mesh.transform(&mat);
                    Some(self.kernel.insert_mesh(mesh))
                } else {
                    None
                };
                if let Some(new_shape) = new_shape {
                    if let Some(old) = item.temp_shape.replace(new_shape) {
                        self.kernel.delete(old);
                    }
                    if let Some(body) = self.doc.body_mut(item.body_id) {
                        body.shape = Some(new_shape);
                    }
                    changed = true;
                }
            }
            if changed {
                self.gizmo_drag = Some(GizmoDrag {
                    items,
                    pivot: drag.pivot,
                    last_pos: pos,
                    mat,
                });
                self.mesh_cache.clear();
                self.bump_scene();
            }
            return;
        }
        // 结束拖拽：用 B-rep transform 最终化（保持参数化），清理临时网格
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if let Some(drag) = self.gizmo_drag.take() {
                for item in &drag.items {
                    if let Some(temp) = item.temp_shape {
                        self.kernel.delete(temp);
                    }
                }
                if drag.mat != glam::Mat4::IDENTITY {
                    let mut applied = 0usize;
                    for item in &drag.items {
                        match self.kernel.transform(item.start_shape, &drag.mat) {
                            Ok(final_shape) => {
                                self.transform_thread_state(
                                    item.body_id,
                                    item.start_shape,
                                    final_shape,
                                    &drag.mat,
                                );
                                if let Some(body) = self.doc.body_mut(item.body_id) {
                                    body.shape = Some(final_shape);
                                }
                                applied += 1;
                            }
                            Err(error) => {
                                if let Some(body) = self.doc.body_mut(item.body_id) {
                                    body.shape = Some(item.start_shape);
                                }
                                self.toast(format!(
                                    "{} failed for object {}: {error}",
                                    gizmo.name(),
                                    item.body_id
                                ));
                            }
                        }
                    }
                    self.mesh_cache.clear();
                    self.face_pick = None;
                    self.selected_edge = None;
                    self.toast(format!("{} applied to {applied} object(s)", gizmo.name()));
                    self.record_history(format!("{} {} objects", gizmo.name(), applied));
                    self.bump_scene();
                } else {
                    // 无实际位移：恢复所有 objects 的原形状。
                    for item in &drag.items {
                        if let Some(body) = self.doc.body_mut(item.body_id) {
                            body.shape = Some(item.start_shape);
                        }
                    }
                    self.bump_scene();
                }
            }
        }
    }

    /// 单次拖拽增量 → 变换矩阵（绕体中心）。
    // ================= Copy / Paste（剪贴板）=================

    /// 把选中的单个 body 复制到剪贴板（保留不可变的完整几何快照）。
    pub fn cut_selected(&mut self) {
        if self.mode != Mode::Idle
            || self.selected.len() != 1
            || self.body(self.selected[0]).and_then(|b| b.shape).is_none()
        {
            self.toast("Select one object to cut");
            return;
        }
        self.copy_selected();
        self.delete_selected();
    }

    pub(crate) fn remember_directory(&mut self, path: &Path, output: bool) {
        let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) else {
            return;
        };
        let directory = if output {
            &mut self.settings.last_output_directory
        } else {
            &mut self.settings.last_input_directory
        };
        if directory.as_deref() != Some(parent) {
            *directory = Some(parent.to_path_buf());
            self.save_ui_preferences();
        }
    }

    pub fn copy_selected(&mut self) {
        let Some(id) = self.require_single_object("Copy") else {
            return;
        };
        let Some(body) = self.doc.body(id).cloned() else {
            return;
        };
        if body.shape.is_none() {
            self.toast("Nothing to copy");
            return;
        }
        self.clipboard = Some(ClipboardItem { body });
        self.paste_count = 0;
        self.toast("Copied to clipboard - use Paste");
        self.record_history("Copy to clipboard");
    }

    /// 粘贴剪贴板内容为新 body（沿 +X 偏移 20mm 避免与原体重叠）。
    pub fn paste_clipboard(&mut self) {
        let Some(item) = self.clipboard.clone() else {
            self.toast("Clipboard is empty - Copy a body first");
            return;
        };
        let next_count = self.paste_count.saturating_add(1);
        let offset = 20.0 * next_count as f32;
        let matrix = glam::Mat4::from_translation(glam::Vec3::new(offset, 0.0, 0.0));
        let Some(source_shape) = item.body.shape else {
            return;
        };
        let shape = match self.kernel.transform(source_shape, &matrix) {
            Ok(shape) => shape,
            Err(error) => {
                self.toast(format!("Paste failed: {error}"));
                return;
            }
        };
        self.paste_count = next_count;
        self.begin_document_change();
        let id = self.doc.alloc_id();
        let mut body = item.body;
        let source_id = body.id;
        body.id = id;
        body.name = format!("Body {id}");
        body.shape = Some(shape);
        body.feature = Feature::Derived {
            source: format!("copy of {source_id}"),
            operation: Some(DerivedOperation::Copy {
                source: source_id,
                offset: [offset as f64, 0.0, 0.0],
                primitive_snapshot: copy_primitive_snapshot(&body.feature, [offset as f64, 0., 0.]),
            }),
        };
        body.visible = true;
        self.doc.add_body(body);
        self.selected = vec![id];
        self.record_history(format!("[sel] select body {id} clears regions"));
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.bump_scene();
        self.toast(format!("Pasted Body {id}"));
        self.record_history("Paste");
    }

    // ================= Array（线性阵列复制）=================

    /// 把选中的单个 solid 沿启用的轴（可多轴，笛卡尔积）按间距/总数量复制成阵列。
    /// 每轴 Copies 包含原件；原件不动，新对象偏移 = (i*dx, j*dy, k*dz)。
    pub fn array_selected(&mut self) {
        if self.queue_model_command(("Array", "阵列"), Self::array_selected) {
            return;
        }
        let Some(id) = self.require_single_object("Array") else {
            return;
        };
        let axes = self.array_axes;
        let copies = self.array_copies;
        let positive = self.array_positive;
        let spacing = self.array_spacing;
        let ranges = array_step_ranges(axes, copies);
        let total_objects = array_total_objects(ranges);
        if total_objects <= 1 {
            self.toast("Enable at least one axis with Copies >= 2 (including the original)");
            return;
        }
        if ranges
            .iter()
            .enumerate()
            .any(|(axis, steps)| *steps > 0 && spacing[axis].abs() < 1e-3)
        {
            self.toast("Array spacing must be greater than zero");
            return;
        }
        let Some(body) = self.doc.body(id).cloned() else {
            return;
        };
        let Some(shape) = body.shape else {
            self.toast("Nothing to array");
            return;
        };
        let dir: [f32; 3] = [
            if axes[0] {
                if positive[0] {
                    spacing[0]
                } else {
                    -spacing[0]
                }
            } else {
                0.0
            },
            if axes[1] {
                if positive[1] {
                    spacing[1]
                } else {
                    -spacing[1]
                }
            } else {
                0.0
            },
            if axes[2] {
                if positive[2] {
                    spacing[2]
                } else {
                    -spacing[2]
                }
            } else {
                0.0
            },
        ];
        let mut prepared = Vec::new();
        for i in 0..=ranges[0] {
            for j in 0..=ranges[1] {
                for k in 0..=ranges[2] {
                    if i == 0 && j == 0 && k == 0 {
                        continue; // 原件
                    }
                    let mut t = [0.0f32; 3];
                    t[0] = dir[0] * i as f32;
                    t[1] = dir[1] * j as f32;
                    t[2] = dir[2] * k as f32;
                    // Preserve native surfaces. Flattening copies to a display
                    // mesh loses cylinder/bore identity and other exact faces.
                    let matrix = glam::Mat4::from_translation(glam::Vec3::from_array(t));
                    match self.kernel.transform(shape, &matrix) {
                        Ok(copy) => prepared.push((t, copy)),
                        Err(error) => {
                            for (_, copy) in prepared {
                                self.kernel.delete(copy);
                            }
                            self.toast(format!("Array failed: {error}"));
                            return;
                        }
                    }
                }
            }
        }
        self.begin_document_change();
        let mut created = 0u32;
        let mut last_id = id;
        for (t, new_shape) in prepared {
            let new_id = self.doc.alloc_id();
            let mut nb = body.clone();
            nb.id = new_id;
            nb.name = format!("Body {new_id}");
            nb.shape = Some(new_shape);
            nb.feature = Feature::Derived {
                source: format!("copy of {id}"),
                operation: Some(DerivedOperation::Copy {
                    source: id,
                    offset: [t[0] as f64, t[1] as f64, t[2] as f64],
                    primitive_snapshot: copy_primitive_snapshot(&body.feature, t.map(f64::from)),
                }),
            };
            nb.visible = true;
            self.doc.add_body(nb);
            created += 1;
            last_id = new_id;
        }
        self.selected = vec![last_id];
        self.record_history(format!("[sel] select body {last_id} clears regions"));
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.bump_scene();
        self.toast(format!(
            "Array created: {} total objects ({created} new)",
            created + 1
        ));
        let desc: Vec<String> = axes
            .iter()
            .enumerate()
            .filter(|(_, &on)| on)
            .map(|(a, _)| {
                format!(
                    "{}{}x{}@{}",
                    ["X", "Y", "Z"][a],
                    if positive[a] { "+" } else { "-" },
                    copies[a],
                    spacing[a]
                )
            })
            .collect();
        self.record_history(format!("Array {}", desc.join(" ")));
        self.array_exit_applied = Some(self.array_exit_signature());
    }

    // ================= Chamfer / Fillet（棱线识别 + CSG 倒角/圆角）=================

    /// 拾取悬停位置最近的锐棱链（屏幕距离 < 6px；圆形棱整条识别）。
    pub fn pick_edge(&mut self, pos: egui::Pos2, rect: egui::Rect) -> Option<EdgeRef> {
        let ndc = screen_to_ndc(pos, rect);
        let (ro, rd) = self.ray_from_ndc(ndc);
        let view_proj = self
            .camera
            .proj_matrix(rect.width() / rect.height().max(1.0))
            * self.camera.view_matrix();
        let solids: Vec<(BodyId, ShapeId)> = self
            .doc
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Solid && b.visible)
            .filter_map(|b| b.shape.map(|s| (b.id, s)))
            .collect();
        let mut best: Option<(EdgeRef, f32)> = None; // (edge, 屏幕距离)
        for (body_id, shape) in &solids {
            let geometry = if let Some(cached) = self.pick_geometry_cache.get(shape) {
                Arc::clone(cached)
            } else {
                let cached = Arc::new(PickGeometry::new(self.mesh_for(*shape)));
                // Bound transient-preview retention between document GC passes.
                if self.pick_geometry_cache.len() >= 64 {
                    self.pick_geometry_cache.clear();
                }
                self.pick_geometry_cache.insert(*shape, Arc::clone(&cached));
                cached
            };
            let m = &geometry.mesh;
            if m.is_empty() {
                continue;
            }
            let Some((tri, _)) = ray_mesh_tri(ro, rd, &m) else {
                continue;
            };
            let t = &m.indices[tri * 3..tri * 3 + 3];
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (ti, tj) = (t[i] as usize, t[j] as usize);
                let key = if ti < tj { (ti, tj) } else { (tj, ti) };
                let Some(&chain) = geometry.edge_to_chain.get(&key) else {
                    continue;
                };
                let segments = geometry.chains[chain].clone();
                if segments.is_empty() {
                    continue;
                }
                // 屏幕距离：链折线各段投影距离的最小值
                let mut sd = f32::INFINITY;
                for s in &segments {
                    let pa = glam::Vec3::from_array(s.a);
                    let pb = glam::Vec3::from_array(s.b);
                    sd = sd.min(edge_screen_dist(&view_proj, rect, pos, pa, pb));
                }
                if sd < 6.0 && best.as_ref().map(|(_, d)| sd < *d).unwrap_or(true) {
                    best = Some((
                        EdgeRef {
                            body_id: *body_id,
                            segments,
                        },
                        sd,
                    ));
                }
            }
        }
        best.map(|(e, _)| e)
    }

    /// 查找某目标体当前应继续使用的 chamfer/fillet 结果 body。
    ///
    /// 连续操作时不能按创建顺序从头查找，否则第三次及以后会一直取到
    /// 第一个结果，造成特征回滚并让两个结果体重叠显示。当前选中的结果
    /// 最能代表用户正在编辑/继续加工的分支，其次才是最新可见结果。
    fn find_chamfer_body(&self, target: BodyId) -> Option<BodyId> {
        active_chamfer_body(&self.doc, &self.selected, target)
    }

    /// 在给定基础 shape 上计算一次 chamfer/fillet，但不修改 Document。
    /// Apply 与删除特征后的依赖重建共用同一条几何路径。
    fn evaluate_chamfer_feature(
        &mut self,
        base: ShapeId,
        segments: &[EdgeSeg],
        chamfer: Option<f64>,
        fillet: Option<f64>,
    ) -> Result<(ShapeId, usize), String> {
        let name = if fillet.is_some() {
            "Fillet"
        } else {
            "Chamfer"
        };
        let kernel_segments: Vec<EdgeCutterSegment> = segments
            .iter()
            .map(|seg| EdgeCutterSegment {
                a: [seg.a[0] as f64, seg.a[1] as f64, seg.a[2] as f64],
                b: [seg.b[0] as f64, seg.b[1] as f64, seg.b[2] as f64],
                n1: [seg.n1[0] as f64, seg.n1[1] as f64, seg.n1[2] as f64],
                n2: [seg.n2[0] as f64, seg.n2[1] as f64, seg.n2[2] as f64],
            })
            .collect();

        self.report_model_progress(("Preparing edge treatment…", "正在准备棱边处理…"), None)?;
        let mut continuous_kind = "circular";
        let mut continuous_cutter = self
            .kernel
            .circular_edge_cutter(&kernel_segments, chamfer, fillet)
            .map_err(|e| format!("Cutter failed: {e}"))?;
        if continuous_cutter.is_none() {
            continuous_kind = "closed contour";
            continuous_cutter = self
                .kernel
                .closed_planar_edge_cutter(&kernel_segments, chamfer, fillet)
                .map_err(|e| format!("Cutter failed: {e}"))?;
        }

        if let Some(cutter_mesh) = continuous_cutter {
            self.report_model_progress(("Applying edge treatment…", "正在计算棱边结果…"), None)?;
            let cutter = self.kernel.insert_mesh(cutter_mesh);
            let shape = self
                .kernel
                .boolean_subtract(base, cutter)
                .map_err(|e| format!("{name} failed on {continuous_kind}: {e}"))?;
            return Ok((shape, segments.len()));
        }

        // 非平滑/非闭合链保留原逐段路径；仅在 fallback 内平滑弧面法线。
        let mut fallback_segments = segments.to_vec();
        if fallback_segments.len() > 2 {
            let base_n1 = glam::Vec3::from_array(fallback_segments[0].n1);
            for segment in &mut fallback_segments {
                let a1 = glam::Vec3::from_array(segment.n1);
                let a2 = glam::Vec3::from_array(segment.n2);
                if a1.dot(base_n1) < a2.dot(base_n1) {
                    std::mem::swap(&mut segment.n1, &mut segment.n2);
                }
            }
            let n = fallback_segments.len();
            let smooth_n2: Vec<[f32; 3]> = fallback_segments
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let mut acc = glam::Vec3::ZERO;
                    let mut count = 0usize;
                    for segment in &fallback_segments[i.saturating_sub(1)..=(i + 1).min(n - 1)] {
                        acc += glam::Vec3::from_array(segment.n2);
                        count += 1;
                    }
                    (acc / count as f32).normalize().to_array()
                })
                .collect();
            for (segment, normal) in fallback_segments.iter_mut().zip(smooth_n2) {
                segment.n2 = normal;
            }
        }

        let mut cutters = Vec::with_capacity(fallback_segments.len());
        for (index, segment) in fallback_segments.iter().enumerate() {
            self.report_model_progress(
                ("Preparing edge segments…", "正在准备棱边分段…"),
                Some((index, fallback_segments.len())),
            )?;
            let cutter_mesh = self
                .kernel
                .edge_cutter(
                    [
                        segment.a[0] as f64,
                        segment.a[1] as f64,
                        segment.a[2] as f64,
                    ],
                    [
                        segment.b[0] as f64,
                        segment.b[1] as f64,
                        segment.b[2] as f64,
                    ],
                    [
                        segment.n1[0] as f64,
                        segment.n1[1] as f64,
                        segment.n1[2] as f64,
                    ],
                    [
                        segment.n2[0] as f64,
                        segment.n2[1] as f64,
                        segment.n2[2] as f64,
                    ],
                    chamfer,
                    fillet,
                )
                .map_err(|e| format!("Cutter failed: {e}"))?;
            cutters.push(self.kernel.insert_mesh(cutter_mesh));
        }
        if cutters.is_empty() {
            return Err(format!("{name}: no segments processed"));
        }

        // Resolve overlaps between adjacent segment cutters before touching the
        // base. A single subtraction avoids accumulating triangulation seams
        // at every corner. Highly pathological/disjoint cutters can still fail
        // to union, in which case retain the established per-segment path.
        let mut compound = cutters[0];
        let mut compound_is_temporary = false;
        let mut compound_ok = true;
        for cutter in cutters.iter().copied().skip(1) {
            match self.kernel.boolean_union(compound, cutter) {
                Ok(next) => {
                    if compound_is_temporary {
                        self.kernel.delete(compound);
                    }
                    compound = next;
                    compound_is_temporary = true;
                }
                Err(_) => {
                    compound_ok = false;
                    if compound_is_temporary {
                        self.kernel.delete(compound);
                    }
                    break;
                }
            }
        }
        if compound_ok {
            let shape = self
                .kernel
                .boolean_subtract(base, compound)
                .map_err(|e| format!("{name} failed on the combined edge chain: {e}"))?;
            return Ok((shape, fallback_segments.len()));
        }

        let mut shape = base;
        let total = cutters.len();
        for (index, cutter) in cutters.into_iter().enumerate() {
            self.report_model_progress(
                ("Applying edge segments…", "正在计算棱边分段…"),
                Some((index, total)),
            )?;
            shape = self
                .kernel
                .boolean_subtract(shape, cutter)
                .map_err(|e| format!("{name} failed on segment: {e}"))?;
        }
        Ok((shape, fallback_segments.len()))
    }

    /// 以一个已经重算好的节点为种子，按父子顺序重算其后的特征。
    fn evaluate_chamfer_rebuilds(
        &mut self,
        seed_id: BodyId,
        seed_shape: ShapeId,
        order: &[(BodyId, BodyId)],
    ) -> Result<
        Vec<(
            BodyId,
            BodyId,
            ShapeId,
            Vec<s33d_model::ChamferEdgeSeg>,
            Option<[[f64; 3]; 2]>,
        )>,
        String,
    > {
        let mut rebuilt_shapes: HashMap<BodyId, ShapeId> = HashMap::new();
        rebuilt_shapes.insert(seed_id, seed_shape);
        let mut updates = Vec::with_capacity(order.len());
        for &(child_id, new_base) in order {
            let child = self
                .doc
                .body(child_id)
                .cloned()
                .ok_or_else(|| format!("Dependent operation {child_id} is missing"))?;
            let (edges, reference_bounds, chamfer, fillet) = match &child.feature {
                Feature::Chamfer {
                    edges,
                    reference_bounds,
                    chamfer,
                    fillet,
                    ..
                } => (edges, *reference_bounds, *chamfer, *fillet),
                _ => continue,
            };
            let base_shape = rebuilt_shapes
                .get(&new_base)
                .copied()
                .or_else(|| self.doc.body(new_base).and_then(|body| body.shape))
                .ok_or_else(|| format!("Base object {new_base} is missing"))?;
            let mesh = self
                .kernel
                .tessellate(base_shape)
                .map_err(|error| error.to_string())?;
            let stored = stored_chamfer_segments(edges);
            let segments = match reference_bounds {
                Some(reference_bounds) => remap_chamfer_segments(&mesh, &stored, reference_bounds)
                    .ok_or_else(|| {
                        format!("Chamfer {child_id} selected edge no longer resolves")
                    })?,
                None => stored,
            };
            let (shape, _) = self
                .evaluate_chamfer_feature(base_shape, &segments, chamfer, fillet)
                .map_err(|error| {
                    format!("Chamfer {child_id} cannot be rebuilt after the edit: {error}")
                })?;
            rebuilt_shapes.insert(child_id, shape);
            updates.push((
                child_id,
                new_base,
                shape,
                model_chamfer_segments(&segments),
                mesh_reference_bounds(&mesh),
            ));
        }
        Ok(updates)
    }

    /// Chamfer/Fillet 提交：平滑闭链使用单个连续刀具；其余棱链逐段回退。
    /// 结果作为独立 object（Feature::Chamfer）记录，可点选改参数重算。
    pub fn apply_chamfer(&mut self) {
        if self.queue_model_command(("Chamfer / Fillet", "倒角 / 圆角"), Self::apply_chamfer) {
            return;
        }
        let Some(edge) = self.selected_edge.clone() else {
            self.toast("Click an edge first");
            return;
        };
        let target = edge.body_id;
        // 拾取的是 chamfer 结果体：重定向到其原目标体（棱属于原体几何）
        let target = match self.doc.body(target).map(|b| &b.feature) {
            Some(Feature::Chamfer { target: t, .. }) => *t,
            _ => target,
        };
        let Some(target_shape) = self.doc.body(target).and_then(|b| b.shape) else {
            self.toast("Edge body has no shape");
            return;
        };
        // 基础体（链式累积）：
        // - 更新当前 object（未 Done）：用该 object 记录的 base
        // - Done 后新 object：用最新的 chamfer 结果（没有则原目标体）
        let latest = self.find_chamfer_body(target);
        // A different picked location starts the next feature automatically.
        // Compare against the last successful Apply, not the last click: an
        // unsuccessful operation must leave the existing feature untouched.
        let new_location =
            latest
                .and_then(|id| self.doc.body(id))
                .is_some_and(|b| match &b.feature {
                    Feature::Chamfer { edges, .. } => !same_chamfer_location(&edge.segments, edges),
                    _ => false,
                });
        let start_new = self.chamfer_committed || new_location;
        let editing_existing = !start_new && latest.is_some();
        let visible_result_before = latest_visible_chamfer_body(&self.doc, target);
        let base_body = if start_new {
            latest.unwrap_or(target)
        } else {
            latest
                .and_then(|cid| {
                    self.doc.body(cid).and_then(|b| match &b.feature {
                        Feature::Chamfer { base, .. } => *base,
                        _ => None,
                    })
                })
                .unwrap_or(target)
        };
        let base = self
            .doc
            .body(base_body)
            .and_then(|b| b.shape)
            .unwrap_or(target_shape);
        let d = self.chamfer_param.max(0.05) as f64;
        let (chamfer, fillet) = if self.chamfer_fillet {
            (None, Some(d))
        } else {
            (Some(d), None)
        };
        let name = if self.chamfer_fillet {
            "Fillet"
        } else {
            "Chamfer"
        };
        let (shape, seg_count) =
            match self.evaluate_chamfer_feature(base, &edge.segments, chamfer, fillet) {
                Ok(result) => result,
                Err(error) => {
                    self.toast(error);
                    return;
                }
            };
        // 编辑历史中的中间节点时，后继不能继续引用旧 shape；先完整试算，
        // 全部成功后再与当前节点一起提交。
        let descendant_updates = if editing_existing {
            let edited_id = latest.expect("editing_existing requires an existing feature");
            let order = match chamfer_descendant_plan(&self.doc, target, edited_id, edited_id) {
                Ok(order) => order,
                Err(error) => {
                    self.toast(error);
                    return;
                }
            };
            match self.evaluate_chamfer_rebuilds(edited_id, shape, &order) {
                Ok(updates) => updates,
                Err(error) => {
                    self.toast(format!("Cannot edit this operation: {error}"));
                    return;
                }
            }
        } else {
            Vec::new()
        };
        // feature 数据（模型层类型）
        let feature = Feature::Chamfer {
            target,
            base: Some(base_body),
            edges: edge
                .segments
                .iter()
                .map(|s| s33d_model::ChamferEdgeSeg {
                    a: [s.a[0] as f64, s.a[1] as f64, s.a[2] as f64],
                    b: [s.b[0] as f64, s.b[1] as f64, s.b[2] as f64],
                    n1: [s.n1[0] as f64, s.n1[1] as f64, s.n1[2] as f64],
                    n2: [s.n2[0] as f64, s.n2[1] as f64, s.n2[2] as f64],
                })
                .collect(),
            reference_bounds: self
                .kernel
                .tessellate(base)
                .ok()
                .and_then(|mesh| mesh_reference_bounds(&mesh)),
            chamfer,
            fillet,
        };
        let mat = self
            .doc
            .body(target)
            .map(|b| b.material.clone())
            .unwrap_or_default();
        self.begin_document_change();
        // 已有结果 body：原地更新；否则新建 object
        // 隐藏原目标体：场景只显示带倒角的结果体（Objects 列表可恢复）
        if let Some(tb) = self.doc.body_mut(target) {
            tb.visible = false;
        }
        let result_id = if start_new {
            // Done 后：新棱 → 新的 chamfer object；隐藏被取代的上一个结果
            if let Some(prev) = latest {
                if let Some(pb) = self.doc.body_mut(prev) {
                    pb.visible = false;
                }
            }
            let id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id,
                name: format!("Chamfer {id}"),
                kind: BodyKind::Solid,
                feature: feature.clone(),
                shape: Some(shape),
                material: mat.clone(),
                visible: true,
            });
            self.chamfer_committed = false;
            id
        } else if let Some(cid) = latest {
            if let Some(b) = self.doc.body_mut(cid) {
                b.shape = Some(shape);
                b.feature = feature.clone();
                b.material = mat.clone();
            }
            cid
        } else {
            let id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id,
                name: format!("Chamfer {id}"),
                kind: BodyKind::Solid,
                feature,
                shape: Some(shape),
                material: mat,
                visible: true,
            });
            id
        };
        for (child_id, new_base, rebuilt_shape, edges, reference_bounds) in &descendant_updates {
            if let Some(body) = self.doc.body_mut(*child_id) {
                body.shape = Some(*rebuilt_shape);
                if let Feature::Chamfer {
                    base,
                    edges: stored_edges,
                    reference_bounds: stored_bounds,
                    ..
                } = &mut body.feature
                {
                    *base = Some(*new_base);
                    *stored_edges = edges.clone();
                    *stored_bounds = *reference_bounds;
                }
            }
        }
        // 一个特征链只能有一个可见结果。除了解决正常的链式显示，这也会
        // 自动收拢旧版本中遗留的重叠可见对象。编辑历史节点时继续显示
        // 原来的活动末端，而不是显示中间节点并让后续效果消失。
        let visible_result = if editing_existing {
            visible_result_before.unwrap_or(result_id)
        } else {
            result_id
        };
        for body in &mut self.doc.bodies {
            if matches!(
                &body.feature,
                Feature::Chamfer { target: t, .. } if *t == target
            ) {
                body.visible = body.id == visible_result;
            }
        }
        self.chamfer_committed = false;
        self.selected = vec![result_id];
        self.record_history(format!("[sel] select body {result_id} clears regions"));
        self.clear_region_selection();
        // 保留 selected_edge：改参数后可重新 Apply（选新边才替换）
        self.hover_edge = None;
        // 保持工具激活：可连续选边连续倒角（Esc 或再点按钮退出）
        self.mesh_cache.clear();
        self.bump_scene();
        let distance = self.settings.unit.format_mm(d);
        self.toast(format!(
            "{name} {distance} applied to {} segment(s){} - change params and Apply again, or Esc to exit",
            seg_count,
            if descendant_updates.is_empty() {
                String::new()
            } else {
                format!("; {} later operation(s) rebuilt", descendant_updates.len())
            }
        ));
        self.record_history(format!("{name} {distance} ({seg_count} segments)"));
    }

    // ================= Align（多对象对齐，参考 Tinkercad）=================

    /// 把其它选中 solid 沿某轴按 min/center/max 对齐到锁定 object；锁定体不移动。
    pub fn align_selected(&mut self, axis: usize, mode: AlignMode) {
        if axis > 2 {
            self.toast("Invalid align axis");
            return;
        }
        let ids: Vec<BodyId> = self
            .selected
            .iter()
            .copied()
            .filter(|&i| {
                self.doc
                    .body(i)
                    .map(|b| b.kind == BodyKind::Solid && b.shape.is_some())
                    .unwrap_or(false)
            })
            .collect();
        if ids.len() < 2 {
            self.toast("Select at least 2 solid bodies to align (Shift-click to multi-select)");
            return;
        }
        let Some(reference) = self.align_reference.filter(|id| ids.contains(id)) else {
            self.toast("Lock one selected object as the alignment reference");
            return;
        };
        // 各体 bbox
        let mut bboxes: Vec<(BodyId, [f32; 3], [f32; 3])> = Vec::new();
        for id in &ids {
            let Some(shape) = self.doc.body(*id).and_then(|b| b.shape) else {
                continue;
            };
            if let Ok(mesh) = self.kernel.tessellate(shape) {
                if let Some((mn, mx)) = mesh.bounds() {
                    bboxes.push((*id, mn.to_array(), mx.to_array()));
                }
            }
        }
        if bboxes.len() < 2 {
            self.toast("Could not compute bounds of selected bodies");
            return;
        }
        let Some((_, reference_min, reference_max)) =
            bboxes.iter().find(|(id, _, _)| *id == reference)
        else {
            self.toast("Could not compute bounds of the locked object");
            return;
        };
        self.begin_document_change();
        let mut moved = 0usize;
        for (id, mn, mx) in &bboxes {
            if *id == reference {
                continue;
            }
            let t = alignment_translation(reference_min, reference_max, mn, mx, axis, mode);
            let delta = t[axis];
            if delta.abs() < 1e-5 {
                continue;
            }
            let mat = glam::Mat4::from_translation(glam::Vec3::from_array(t));
            let Some(shape) = self.doc.body(*id).and_then(|b| b.shape) else {
                continue;
            };
            match self.kernel.transform(shape, &mat) {
                Ok(new) => {
                    // 注意：不删除旧 shape——undo 快照仍引用它
                    self.transform_thread_state(*id, shape, new, &mat);
                    if let Some(b) = self.doc.body_mut(*id) {
                        b.shape = Some(new);
                    }
                    moved += 1;
                }
                Err(e) => self.toast(format!("Align failed: {e}")),
            }
        }
        self.mesh_cache.clear();
        self.bump_scene();
        let ax = ["X", "Y", "Z"][axis.min(2)];
        let md = match mode {
            AlignMode::Min => "min",
            AlignMode::Center => "center",
            AlignMode::Max => "max",
        };
        self.record_history(format!(
            "Align {ax} {md} to object {reference} ({moved} bodies)"
        ));
        self.toast(format!(
            "Aligned {moved} bodies along {ax} ({md}); object {reference} stayed locked"
        ));
    }

    fn gizmo_step_matrix(
        &self,
        pivot: glam::Vec3,
        gizmo: GizmoMode,
        delta: egui::Vec2,
    ) -> glam::Mat4 {
        if gizmo == GizmoMode::Rotate {
            let (point, direction) = if let Some(axis) = self.rotate_custom_axis {
                (
                    glam::Vec3::from_array(axis.point),
                    glam::Vec3::from_array(axis.direction).normalize_or_zero(),
                )
            } else {
                let direction = match self.rotate_input.0 {
                    0 => glam::Vec3::X,
                    2 => glam::Vec3::Z,
                    _ => glam::Vec3::Y,
                };
                (pivot, direction)
            };
            return rotation_about_axis(point, direction, -delta.x * 0.01);
        }
        let wpp = self.world_per_pixel_at(pivot);
        gizmo_step_matrix_static(
            gizmo,
            delta,
            pivot,
            wpp,
            self.camera.right(),
            self.camera.up,
        )
    }

    /// 对全部选中 objects 应用同一个世界矩阵（B-rep 保持、可撤销、记录历史）。
    pub fn apply_transform(&mut self, mat: &glam::Mat4, label: &str) {
        let mut curves = Vec::new();
        for id in &self.selected {
            if let Some(Feature::Fastener { params }) = self.body(*id).map(|b| &b.feature) {
                curves.push((
                    *id,
                    Feature::Fastener {
                        params: crate::fasteners::transformed_params(*params, mat),
                    },
                ));
            }
            if let Some(Feature::EquationSurface { params }) = self.body(*id).map(|b| &b.feature) {
                let mut params = params.clone();
                params.transform = (mat.as_dmat4()
                    * glam::DMat4::from_cols_array(&params.transform))
                .to_cols_array();
                curves.push((*id, Feature::EquationSurface { params }));
            }
            if let Some(Feature::EquationCurve { params, .. }) = self.body(*id).map(|b| &b.feature)
            {
                let mut params = params.clone();
                params.transform = (mat.as_dmat4()
                    * glam::DMat4::from_cols_array(&params.transform))
                .to_cols_array();
                match equation_curve::sample(&params) {
                    Ok(points) => curves.push((*id, Feature::EquationCurve { params, points })),
                    Err(e) => {
                        self.toast(e);
                        return;
                    }
                }
            }
        }
        let targets: Vec<(BodyId, ShapeId)> = self
            .selected
            .iter()
            .copied()
            .filter_map(|id| {
                self.body(id)
                    .and_then(|body| body.shape)
                    .map(|shape| (id, shape))
            })
            .collect();
        if targets.is_empty() && curves.is_empty() {
            self.toast("Transform needs at least one selected object");
            return;
        }
        let mut transformed = Vec::with_capacity(targets.len());
        for (id, shape) in targets {
            match self.kernel.transform(shape, mat) {
                Ok(new_shape) => transformed.push((id, new_shape)),
                Err(error) => {
                    self.toast(format!("Transform failed for object {id}: {error}"));
                    return;
                }
            }
        }
        self.begin_document_change();
        for (id, feature) in curves {
            self.doc.body_mut(id).unwrap().feature = feature;
        }
        for (id, new_shape) in &transformed {
            let original = self.doc.body(*id).unwrap().shape.unwrap();
            self.transform_thread_state(*id, original, *new_shape, mat);
            if let Some(body) = self.doc.body_mut(*id) {
                body.shape = Some(*new_shape);
            }
        }
        self.mesh_cache.clear();
        self.face_pick = None;
        self.selected_edge = None;
        self.bump_scene();
        self.record_history(format!("{label} ({} objects)", transformed.len()));
        self.toast(format!(
            "{label} applied to {} object(s)",
            transformed.len()
        ));
    }

    /// 体中心（tessellate 包围盒中心，无 shape 时用原点）。
    /// Move To：把选中体的中心移到目标绝对坐标。
    pub fn move_to_selected(&mut self, target: [f32; 3]) {
        let Some(center) = self.selected_center() else {
            self.toast("Move needs at least one selected object");
            return;
        };
        let delta = glam::Vec3::from_array(target) - center;
        self.exit_move_to_baseline = target;
        if delta.length() < 1e-6 {
            self.toast("Already at the target position");
            return;
        }
        let mat = glam::Mat4::from_translation(delta);
        self.apply_transform(
            &mat,
            &format!(
                "Move to ({}, {}, {})",
                self.settings.unit.format_mm(target[0] as f64),
                self.settings.unit.format_mm(target[1] as f64),
                self.settings.unit.format_mm(target[2] as f64)
            ),
        );
    }

    /// Move 面板方向键快捷位移：沿世界轴平移一个 Step（默认 1 mm）。
    /// `axis` 0=X 1=Y 2=Z；`sign` 取 ±1 表示方向。
    pub fn move_nudge(&mut self, axis: usize, sign: f32) {
        let axis = axis.min(2);
        let step = self.move_step;
        if !step.is_finite() || step <= 0.0 {
            self.toast(format!(
                "Step must be a positive number of {}",
                self.settings.unit.symbol()
            ));
            return;
        }
        let mut delta = glam::Vec3::ZERO;
        delta[axis] = step * sign.signum();
        let mat = glam::Mat4::from_translation(delta);
        let axis_name = ["X", "Y", "Z"][axis];
        let direction = if sign >= 0.0 { "+" } else { "-" };
        self.apply_transform(
            &mat,
            &format!(
                "Move {direction}{axis_name} {}",
                self.settings.unit.format_mm(step as f64)
            ),
        );
    }

    pub fn scale_selected_by_percent(&mut self, percent: [f32; 3]) {
        if percent
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        {
            self.toast("Scale percentages must be finite and greater than zero");
            return;
        }
        let Some(center) = self.selected_center() else {
            self.toast("Scale needs at least one selected object");
            return;
        };
        let factors = glam::Vec3::from_array(percent) / 100.0;
        if factors.min_element() < 0.0001 || factors.max_element() > 10_000.0 {
            self.toast("Scale factor is outside the supported range (0.01% to 1,000,000%)");
            return;
        }
        let mat = glam::Mat4::from_translation(center)
            * glam::Mat4::from_scale(factors)
            * glam::Mat4::from_translation(-center);
        self.apply_transform(
            &mat,
            &format!(
                "Scale by X{:.3}% Y{:.3}% Z{:.3}%",
                percent[0], percent[1], percent[2]
            ),
        );
        self.scale_input = [100.0; 3];
        if let Some((min, max)) = self.selected_bounds() {
            self.scale_size_input = (max - min).to_array();
        }
        self.exit_scale_size_baseline = self.scale_size_input;
    }

    pub fn scale_selected_to_size(&mut self, target_size: [f32; 3]) {
        let Some((min, max)) = self.selected_bounds() else {
            self.toast("Scale needs at least one selected object");
            return;
        };
        let current_size = max - min;
        let factors = match scale_to_size_factors(current_size, glam::Vec3::from_array(target_size))
        {
            Ok(factors) => factors,
            Err(message) => {
                self.toast(message);
                return;
            }
        };
        let center = (min + max) * 0.5;
        let mat = glam::Mat4::from_translation(center)
            * glam::Mat4::from_scale(factors)
            * glam::Mat4::from_translation(-center);
        self.apply_transform(
            &mat,
            &format!(
                "Scale to {} × {} × {}",
                self.settings.unit.format_mm(target_size[0] as f64),
                self.settings.unit.format_mm(target_size[1] as f64),
                self.settings.unit.format_mm(target_size[2] as f64)
            ),
        );
        self.scale_input = [100.0; 3];
        if let Some((new_min, new_max)) = self.selected_bounds() {
            self.scale_size_input = (new_max - new_min).to_array();
        }
        self.exit_scale_size_baseline = self.scale_size_input;
    }

    pub fn body_center(&self, body_id: BodyId) -> glam::Vec3 {
        let center = self
            .body(body_id)
            .and_then(|b| b.shape)
            .and_then(|s| self.kernel.tessellate(s).ok())
            .and_then(|m| m.bounds())
            .map(|(min, max)| (min + max) / 2.0);
        center.unwrap_or(glam::Vec3::ZERO)
    }

    pub fn selected_bounds(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        let mut bounds: Option<(glam::Vec3, glam::Vec3)> = None;
        for id in &self.selected {
            let Some((min, max)) = self
                .body(*id)
                .and_then(|body| body.shape)
                .and_then(|shape| self.kernel.tessellate(shape).ok())
                .and_then(|mesh| mesh.bounds())
                .or_else(|| {
                    self.body(*id)
                        .and_then(|b| equation_curve::bounds(&b.feature))
                })
            else {
                continue;
            };
            bounds = Some(match bounds {
                Some((group_min, group_max)) => (group_min.min(min), group_max.max(max)),
                None => (min, max),
            });
        }
        bounds
    }

    /// 所有可见 solid 的世界包围盒（用于 Section View 默认剖面与切割平面可视化）。
    fn visible_solid_bounds(&mut self) -> Option<(glam::Vec3, glam::Vec3)> {
        let shapes: Vec<ShapeId> = self
            .doc
            .bodies
            .iter()
            .filter(|body| body.visible && body.kind == BodyKind::Solid)
            .filter_map(|body| body.shape)
            .collect();
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        let mut any = false;
        for shape in shapes {
            let mesh = self.mesh_for(shape);
            let Some((mesh_min, mesh_max)) = mesh.bounds() else {
                continue;
            };
            any = true;
            min = min.min(mesh_min);
            max = max.max(mesh_max);
        }
        any.then_some((min, max))
    }

    pub fn selected_center(&self) -> Option<glam::Vec3> {
        self.selected_bounds().map(|(min, max)| (min + max) * 0.5)
    }

    fn rotation_axis_for_selection(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        if let Some(axis) = self.rotate_custom_axis {
            let direction = glam::Vec3::from_array(axis.direction).normalize_or_zero();
            if direction.length_squared() > 1e-8 {
                return Some((glam::Vec3::from_array(axis.point), direction));
            }
        }
        let direction = match self.rotate_input.0 {
            0 => glam::Vec3::X,
            2 => glam::Vec3::Z,
            _ => glam::Vec3::Y,
        };
        self.selected_center().map(|center| (center, direction))
    }

    pub fn use_world_rotation_axis(&mut self, axis: usize) {
        self.rotate_input.0 = axis.min(2);
        self.rotate_custom_axis = None;
        self.rotate_axis_pick_active = false;
        self.set_hint(format!(
            "Rotate around world {} axis through the selection center",
            ["X", "Y", "Z"][self.rotate_input.0]
        ));
        self.bump_scene();
    }

    pub fn begin_rotation_axis_pick(&mut self) {
        let enable = !(self.rotate_axis_pick_active || self.rotate_custom_axis.is_some());
        self.set_selected_rotation_axis_enabled(enable);
    }

    pub fn set_selected_rotation_axis_enabled(&mut self, enabled: bool) {
        if self.mode != Mode::Idle || self.gizmo != Some(GizmoMode::Rotate) {
            return;
        }
        self.rotate_custom_axis = None;
        self.rotate_axis_pick_active = enabled;
        self.set_hint(if enabled {
            "Pick a straight solid edge or a visible sketch line as the rotation axis"
        } else {
            "Selected-axis rotation disabled; using the current world axis"
        });
        self.bump_scene();
    }

    pub fn rotate_selected(&mut self, angle_deg: f32) {
        if self.rotate_axis_pick_active {
            self.toast("Pick a straight line for the rotation axis first");
            return;
        }
        if !angle_deg.is_finite() {
            self.toast("Enter a finite rotation angle");
            return;
        }
        if angle_deg.abs() < 1e-5 {
            self.toast("Rotation angle is zero");
            return;
        }
        let Some((point, direction)) = self.rotation_axis_for_selection() else {
            self.toast("Rotate needs at least one selected object");
            return;
        };
        let mat = rotation_about_axis(point, direction, angle_deg.to_radians());
        let axis_label = if self.rotate_custom_axis.is_some() {
            "picked axis".to_string()
        } else {
            format!("world {}", ["X", "Y", "Z"][self.rotate_input.0.min(2)])
        };
        self.apply_transform(&mat, &format!("Rotate {angle_deg}° around {axis_label}"));
    }

    fn pick_rotation_axis(&mut self, pos: egui::Pos2, rect: egui::Rect) -> Option<RotationAxis> {
        let view_proj = self
            .camera
            .proj_matrix(rect.width() / rect.height().max(1.0))
            * self.camera.view_matrix();
        if let Some(edge) = self.pick_edge(pos, rect) {
            if let Some(axis) = straight_axis_from_edge(&edge) {
                return Some(axis);
            }
            // Chamfer 的链可能把“长直边 + 相切圆角”合为一条。此时允许拾取
            // 光标下足够长的直段，但拒绝圆周离散产生的短 chord。
            let nearest = edge.segments.iter().min_by(|left, right| {
                let left_distance = edge_screen_dist(
                    &view_proj,
                    rect,
                    pos,
                    glam::Vec3::from_array(left.a),
                    glam::Vec3::from_array(left.b),
                );
                let right_distance = edge_screen_dist(
                    &view_proj,
                    rect,
                    pos,
                    glam::Vec3::from_array(right.a),
                    glam::Vec3::from_array(right.b),
                );
                left_distance
                    .partial_cmp(&right_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(segment) = nearest {
                let a = glam::Vec3::from_array(segment.a);
                let b = glam::Vec3::from_array(segment.b);
                let direction = b - a;
                let mut min = glam::Vec3::splat(f32::INFINITY);
                let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
                for point in edge
                    .segments
                    .iter()
                    .flat_map(|segment| [segment.a, segment.b])
                    .map(glam::Vec3::from_array)
                {
                    min = min.min(point);
                    max = max.max(point);
                }
                let chain_extent = (max - min).length().max(1e-5);
                let mut lengths: Vec<f32> = edge
                    .segments
                    .iter()
                    .map(|segment| {
                        (glam::Vec3::from_array(segment.b) - glam::Vec3::from_array(segment.a))
                            .length()
                    })
                    .collect();
                lengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let median = lengths[lengths.len() / 2].max(1e-5);
                if direction.length() >= chain_extent * 0.12 && direction.length() >= median * 2.5 {
                    return Some(RotationAxis {
                        point: a.to_array(),
                        direction: direction.normalize().to_array(),
                        display_start: a.to_array(),
                        display_end: b.to_array(),
                    });
                }
            }
        }

        let sketches: Vec<SketchFeature> = self
            .doc
            .bodies
            .iter()
            .filter(|body| body.visible && body.is_sketch())
            .filter_map(|body| body.sketch().cloned())
            .collect();
        let mut best: Option<(RotationAxis, f32)> = None;
        for sketch in sketches {
            for entity in &sketch.entities {
                let s33d_model::SketchEntity::Line { a, b } = entity else {
                    continue;
                };
                let aw =
                    glam::Vec3::from_array(sketch.plane.to_world(a[0], a[1]).map(|v| v as f32));
                let bw =
                    glam::Vec3::from_array(sketch.plane.to_world(b[0], b[1]).map(|v| v as f32));
                let distance = edge_screen_dist(&view_proj, rect, pos, aw, bw);
                let direction = bw - aw;
                if distance <= 8.0
                    && direction.length_squared() > 1e-8
                    && best
                        .as_ref()
                        .map(|(_, best_distance)| distance < *best_distance)
                        .unwrap_or(true)
                {
                    best = Some((
                        RotationAxis {
                            point: aw.to_array(),
                            direction: direction.normalize().to_array(),
                            display_start: aw.to_array(),
                            display_end: bw.to_array(),
                        },
                        distance,
                    ));
                }
            }
        }
        best.map(|(axis, _)| axis)
    }

    /// 指定世界位置处每逻辑像素对应的世界尺寸。
    fn world_per_pixel_at(&self, point: glam::Vec3) -> f32 {
        let h = self.viewport_rect.height().max(1.0);
        if self.camera.ortho {
            (self.camera.ortho_half * 2.0) / h
        } else {
            let dist = (point - self.camera.eye).length().max(1.0);
            2.0 * dist * (self.camera.fov_y_deg.to_radians() / 2.0).tan() / h
        }
    }

    fn on_left_click(&mut self, response: &egui::Response) {
        if self.tool_exit_prompt.is_some() {
            return;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        // 诊断（debug 开关）：每次点击写日志文件（不受 History 截断影响）
        if self.debug_enabled {
            let line = format!(
                "[click] t={} mode={:?} dbl={} pos=({:.0},{:.0}) selected={:?} regions={:?}\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0),
                self.mode,
                response.double_clicked(),
                pos.x,
                pos.y,
                self.selected,
                self.selected_regions
            );
            append_debug_log(&line);
        }
        let rect = response.rect;
        let modifiers = self
            .workspace_click_modifiers
            .unwrap_or_else(|| response.ctx.input(|i| i.modifiers));
        let shift = modifiers.shift;
        let ctrl = modifiers.command || modifiers.ctrl;
        // Count clicks before any tool-specific picker can consume them.
        let click_count = self
            .advance_workspace_click(pos, response.double_clicked() || response.triple_clicked());
        let sequence = (
            self.last_click,
            self.click_count,
            self.click_selection_base.clone(),
        );
        let selection_camera = if click_count >= 2 {
            let camera = self
                .click_selection_base
                .as_ref()
                .map(|snapshot| snapshot.camera.clone())
                .unwrap_or_else(|| self.camera.clone());
            self.restore_sidebar_for_exit_click();
            if self.request_tool_exit_prompt(response, modifiers, click_count) {
                return;
            }
            if !self.exit_tool_for_workspace_selection() {
                if self.tool_exit_waiting() {
                    self.pending_tool_exit = Some(Some(tool_exit_prompt::WorkspaceExitClick {
                        response: response.clone(),
                        modifiers,
                        count: click_count,
                    }));
                }
                return;
            }
            self.restore_click_selection();
            // Sketch Exit restores the 3D camera. Pick through the view the
            // user actually clicked, then restore the exited tool's final view.
            Some(std::mem::replace(&mut self.camera, camera))
        } else {
            None
        };
        if self.mode == Mode::OffsetShell {
            self.pick_offset_face(pos, rect);
            return;
        }
        if self.mode == Mode::Idle && self.align_active && self.axis_align.picking {
            self.pick_alignment_axis(pos, rect);
            return;
        }
        if click_count == 1
            && self.mode == Mode::Idle
            && !self.rotate_axis_pick_active
            && !self.chamfer_active
        {
            if let Some(id) = self.pick_curve(pos, rect) {
                self.select(id, shift || ctrl);
                if self.last_click.is_none() {
                    (self.last_click, self.click_count, self.click_selection_base) = sequence;
                }
                return;
            }
        }
        match self.mode {
            Mode::Idle => {
                if self.rotate_axis_pick_active && self.gizmo == Some(GizmoMode::Rotate) {
                    if let Some(axis) = self.pick_rotation_axis(pos, rect) {
                        self.rotate_custom_axis = Some(axis);
                        self.rotate_axis_pick_active = false;
                        self.hover_edge = None;
                        self.set_hint("Rotation axis picked; drag or enter an angle, then Apply");
                        self.toast("Rotation axis selected");
                    } else {
                        self.toast("Pick a straight solid edge or a visible sketch line");
                    }
                    self.bump_scene();
                    return;
                }
                if self.chamfer_active {
                    // Multi-clicks already exited through the shared handler.
                    if let Some(edge) = self.pick_edge(pos, rect) {
                        self.selected = vec![edge.body_id];
                        self.selected_edge = Some(edge);
                        self.clear_region_selection();
                        self.face_pick = None;
                        self.record_history("Select edge for chamfer/fillet");
                    } else {
                        self.selected_edge = None;
                    }
                    self.bump_scene();
                    return;
                }
                // Align 是“先进入工具、再组装选单”的流程：工具激活时
                // 普通点击也使用追加/取消语义，不要用新 object 替换整个选单。
                let additive = shift || ctrl || self.align_active;

                if click_count == 3 {
                    self.last_click = None;
                    self.click_count = 0;
                    self.click_selection_base = None;
                    self.selected_edge = None;
                    self.face_pick = None;
                    self.clear_region_selection();
                    if let Some(solid_id) = self
                        .pick_body_id(pos, rect)
                        .or_else(|| self.pick_curve(pos, rect))
                    {
                        let selected = self.select_workspace_owner(solid_id, additive);
                        self.record_history(format!("Triple-click select object {solid_id}"));
                        self.toast(if selected {
                            "Object selected"
                        } else {
                            "Object removed from selection"
                        });
                        self.bump_scene();
                    } else if let Some((sketch_id, _, _)) = self.pick_sketch_region(pos, rect) {
                        let selected = self.select_workspace_owner(sketch_id, additive);
                        self.record_history(format!("Triple-click select object {sketch_id}"));
                        self.toast(if selected {
                            "Sketch object selected"
                        } else {
                            "Sketch object removed from selection"
                        });
                        self.bump_scene();
                    } else {
                        self.clear_selection();
                    }
                } else if click_count == 2 {
                    self.selected_edge = None;
                    // A matching consumed pocket sketch must not mask its
                    // visible recessed floor or cylindrical inner wall.
                    let face_hit = self.pick_face(pos, rect);
                    let region_hit = self.pick_sketch_region(pos, rect);
                    let pick_face = self.face_precedes_sketch_region(&face_hit, &region_hit);
                    if let (Some((face, _)), true) = (face_hit, pick_face) {
                        // 双击 solid 表面：选中该平面（可作为 sketch 平面）
                        let bid = face.body_id;
                        let selected = self.select_workspace_owner(bid, additive);
                        self.face_pick = selected.then_some(face.clone());
                        self.clear_region_selection();
                        self.record_history(format!(
                            "Select face of body {} n=({:.2},{:.2},{:.2})",
                            bid, face.normal[0], face.normal[1], face.normal[2]
                        ));
                        let cylindrical = selected && self.thread_target_for_face(&face).is_some();
                        self.toast(if cylindrical {
                            self.settings.language.text("Cylinder wall selected - right-click to generate thread", "已选择完整圆柱面；右键可生成螺纹")
                        } else if selected {
                            "Surface selected - use Extrude, Pocket, Revolve, or Sketch in Properties"
                        } else {
                            "Face owner removed from selection"
                        });
                        self.bump_scene();
                    } else if let Some((id, region_idx, _)) = region_hit {
                        let text_group = match self.text_region_group(id, region_idx) {
                            Ok(group) => group,
                            Err(error) => {
                                eprintln!("{error}");
                                self.toast(self.settings.language.text(
                                    "Cannot select this text region; check its boundaries",
                                    "无法选择此文字区域，请检查轮廓",
                                ));
                                return;
                            }
                        };
                        self.sanitize_region_selection();
                        self.face_pick = None;
                        if let Some(group) = text_group {
                            self.select_region_group(id, group, additive);
                        } else if additive && self.selected.len() == 1 && self.selected[0] == id {
                            if let Some(p) =
                                self.selected_regions.iter().position(|&r| r == region_idx)
                            {
                                self.selected_regions.remove(p);
                                self.record_history(format!(
                                    "Deselect region {region_idx} of sketch {id}"
                                ));
                            } else {
                                self.selected_regions.push(region_idx);
                                self.selected_regions.sort_unstable();
                                self.record_history(format!(
                                    "Add region {region_idx} to selection (sketch {id})"
                                ));
                            }
                        } else {
                            self.selected = vec![id];
                            self.selected_regions = vec![region_idx];
                            let region_area = self
                                .regions_of(id)
                                .get(region_idx)
                                .map(|r| r.signed_area())
                                .unwrap_or(0.0);
                            self.record_history(format!(
                                "Select region {} (area {:.1}) of sketch {}",
                                region_idx, region_area, id
                            ));
                        }
                        self.set_region_selection(id, self.selected_regions.clone());
                        let region_count = self.regions_of(id).len();
                        self.toast(format!(
                            "Region selected: {} of {} - press E to extrude",
                            self.selected_regions.len(),
                            region_count
                        ));
                        self.bump_scene();
                    } else {
                        self.toast("Double-click a sketch region or a solid face to select it");
                    }
                } else {
                    // 单击：优先选中光标下的可见锐棱；面和整个 object 分别留给
                    // 双击与三击，避免选择意图互相抢占。
                    if let Some(edge) = self.pick_edge(pos, rect) {
                        let body_id = edge.body_id;
                        let selected = self.select_workspace_owner(body_id, additive);
                        self.selected_edge = selected.then_some(edge);
                        self.clear_region_selection();
                        self.face_pick = None;
                        self.record_history(format!("Single-click select edge of body {body_id}"));
                        self.toast(if selected {
                            "Edge selected - double-click for face, triple-click for object"
                        } else {
                            "Edge owner removed from selection"
                        });
                        self.bump_scene();
                    } else if let Some((sketch_id, region_idx, _)) =
                        self.pick_sketch_region(pos, rect)
                    {
                        self.selected_edge = None;
                        match self.text_region_group(sketch_id, region_idx) {
                            Ok(Some(group)) => {
                                self.select_region_group(sketch_id, group, additive);
                                return;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                eprintln!("{error}");
                                self.toast(self.settings.language.text(
                                    "Cannot select this text region; check its boundaries",
                                    "无法选择此文字区域，请检查轮廓",
                                ));
                                return;
                            }
                        }
                        let keep = self.selected.len() == 1
                            && self.selected[0] == sketch_id
                            && !self.selected_regions.is_empty();
                        if !keep {
                            self.select(sketch_id, additive);
                            self.face_pick = None;
                        }
                    } else {
                        self.selected_edge = None;
                        // 点在实体面内部但不靠近棱时，保留现场等待第二/第三击；
                        // 真正空白处才清选择。
                        if self.pick_body_id(pos, rect).is_none()
                            && (!self.selected.is_empty() || self.face_pick.is_some())
                        {
                            self.clear_selection();
                        }
                    }
                }
            }
            Mode::PickPlane => {
                self.pick_plane(pos, rect);
            }
            Mode::Sketch => {
                self.sketch_pointer_down_at(pos, rect, shift || ctrl, false);
            }
            Mode::Insert3D => {
                let placing = self.insert_3d_ctx.as_ref().is_some_and(|ctx| {
                    matches!(
                        ctx.stage,
                        Draw3DPlacementStage::AwaitBase | Draw3DPlacementStage::Sizing
                    )
                });
                if placing && self.draw_3d_placement_click(pos, rect) {
                    // Placement consumes both clicks even when an existing
                    // object happens to be behind the cursor.
                } else if self.begin_draw_3d_direct_drag(pos, rect) {
                    // Selecting the live preview must not switch to a body behind it.
                    self.finish_draw_3d_direct_drag();
                } else if let Some(id) = self.pick_body_id(pos, rect) {
                    self.edit_draw_3d_body(id);
                } else {
                    self.toast("Choose a primitive, or click a Draw 3D object to edit it");
                }
            }
            Mode::Boolean => {
                if let Some(id) = self.pick_body_id(pos, rect) {
                    self.boolean_click(id, shift || ctrl);
                }
            }
            Mode::Split => {
                let picking_object = self.split_ctx.as_ref().is_some_and(|ctx| ctx.picking);
                if picking_object {
                    if let Some(id) = self.pick_body_id(pos, rect) {
                        self.split_click(id, shift || ctrl);
                    } else {
                        self.toast("Click a solid object to split");
                    }
                } else if let Some((face, _)) = self.pick_face(pos, rect) {
                    self.set_split_plane(face);
                } else {
                    self.toast("Click a planar surface to set the cutting plane");
                }
            }
            Mode::SectionView => {
                if let Some((face, _)) = self.pick_face(pos, rect) {
                    self.set_section_view_plane(face);
                } else {
                    self.toast("Click a planar surface to set the section plane");
                }
            }
            Mode::Mirror => {
                if let Some((face, _)) = self.pick_face(pos, rect) {
                    self.set_mirror_plane(face);
                } else {
                    self.toast("Click a planar surface to set the mirror plane");
                }
            }
            Mode::RevolveAxis => {
                let source = self
                    .revolve_ctx
                    .as_ref()
                    .map(|ctx| (ctx.body_id, ctx.sketch.clone()));
                let source_is_sketch = source
                    .as_ref()
                    .and_then(|(id, _)| self.body(*id))
                    .is_some_and(|body| body.is_sketch());
                if let Some((_, sketch)) = source.filter(|_| source_is_sketch) {
                    let local = self.screen_to_plane(pos, rect, &sketch.plane);
                    if let Some((p, q)) = pick_sketch_line(&sketch.entities, local) {
                        self.revolve_with_axis(p, [q[0] - p[0], q[1] - p[1]]);
                    } else {
                        self.toast("Click a sketch line as the revolve axis");
                    }
                } else if let Some(edge) = self.pick_edge(pos, rect) {
                    if let Some(axis) = straight_axis_from_edge(&edge) {
                        self.revolve_with_world_axis(axis);
                    } else {
                        self.toast("Choose one straight edge in the selected surface plane");
                    }
                } else {
                    self.toast("Click a straight boundary edge as the revolve axis");
                }
            }
            Mode::Extrude => {
                // Pocket 模式：单击 solid 指定目标体（未定/重选）
                let is_pocket = self.extrude_ctx.as_ref().map(|c| c.pocket).unwrap_or(false);
                if is_pocket {
                    if let Some(solid_id) = self.pick_body_id(pos, rect) {
                        let preview_depth = if let Some(ctx) = self.extrude_ctx.as_mut() {
                            ctx.target_body = Some(solid_id);
                            Some(ctx.depth)
                        } else {
                            None
                        };
                        let name = self
                            .doc
                            .body(solid_id)
                            .map(|b| b.name.clone())
                            .unwrap_or_default();
                        self.toast(format!("Pocket target: {name}"));
                        self.record_history(format!("Pocket target body {solid_id}"));
                        if let Some(depth) = preview_depth {
                            self.set_extrude_depth(depth);
                        } else {
                            self.bump_scene();
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some(camera) = selection_camera {
            self.camera = camera;
            self.bump_scene();
        } else if click_count == 1 && self.last_click.is_none() {
            // A first click can switch PickPlane to Sketch or select a Draw
            // 3D body. Preserve the sequence across that internal transition.
            (self.last_click, self.click_count, self.click_selection_base) = sequence;
        }
    }

    /// Route one screen-space pointer-down into the active sketch plane.
    /// `begin_drag` is true only for an actual primary-button drag start; a
    /// normal selection click is deliberately disarmed after selection.
    fn sketch_pointer_down_at(
        &mut self,
        pos: egui::Pos2,
        rect: egui::Rect,
        additive: bool,
        begin_drag: bool,
    ) {
        let plane = self.sketch.as_ref().map(|sketch| sketch.plane.clone());
        let local = plane
            .as_ref()
            .map(|plane| self.screen_to_plane(pos, rect, plane));
        let snap_radius = plane
            .as_ref()
            .zip(local)
            .map(|(plane, local)| {
                let world = glam::Vec3::from_array(
                    plane
                        .to_world(local[0], local[1])
                        .map(|component| component as f32),
                );
                (self.world_per_pixel_at(world) * 10.0).clamp(0.05, 12.0) as f64
            })
            .unwrap_or(1.0);
        if let (Some(sketch), Some(local)) = (&mut self.sketch, local) {
            if sketch.tool == SketchTool::Trim {
                // Trim is click-only and must not snap to grid, start a drag,
                // or use the drawing step as the picking radius.
                if !begin_drag && sketch.trim_at(local, snap_radius) {
                    self.sync_sketch_history();
                    self.sync_active_sketch_edit();
                }
                self.bump_scene();
                return;
            }
            let pick_tool = sketch.tool == SketchTool::Pick;
            if begin_drag {
                // Pen is a continuous drawing mode, even when a new stroke
                // starts on an existing outline or one of its key points.
                if sketch.tool == SketchTool::Freehand {
                    sketch.begin_freehand(local, snap_radius * 0.15);
                } else {
                    let tolerance = snap_radius.max((self.snap_step * 1.2).max(1.5));
                    sketch.begin_entity_drag(local, tolerance, additive);
                }
                self.bump_scene();
                return;
            }
            // Pick/Align need the real cursor position; drawing tools snap the
            // point before adding geometry.
            let point = if matches!(
                sketch.tool,
                SketchTool::Pick | SketchTool::Align | SketchTool::Move | SketchTool::Rotate
            ) {
                local
            } else {
                sketch.snap(
                    local,
                    self.sketch_geometry_snap,
                    self.snap_enabled,
                    self.snap_step,
                    snap_radius,
                )
            };
            sketch.pointer_down_with_additive(point, self.snap_step, additive);
            if pick_tool && !begin_drag {
                sketch.cancel_drag();
            }
        }
        self.bump_scene();
    }

    pub fn on_pointer_move(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        match self.mode {
            Mode::Sketch => {
                // 先取平面并计算局部坐标（不可变借用），再可变借用 sketch
                let plane = self.sketch.as_ref().map(|s| s.plane.clone());
                let local = plane.as_ref().map(|p| self.screen_to_plane(pos, rect, p));
                let snap_radius = plane
                    .as_ref()
                    .zip(local)
                    .map(|(plane, local)| {
                        let world = glam::Vec3::from_array(
                            plane
                                .to_world(local[0], local[1])
                                .map(|component| component as f32),
                        );
                        (self.world_per_pixel_at(world) * 10.0).clamp(0.05, 12.0) as f64
                    })
                    .unwrap_or(1.0);
                if let (Some(sk), Some(local)) = (&mut self.sketch, local) {
                    sk.pointer_move_with_snap(
                        local,
                        self.sketch_geometry_snap,
                        self.snap_enabled,
                        self.snap_step,
                        snap_radius,
                    );
                }
                self.bump_scene();
            }
            Mode::Insert3D => {
                if self
                    .insert_3d_ctx
                    .as_ref()
                    .is_some_and(|ctx| ctx.stage == Draw3DPlacementStage::Sizing)
                {
                    self.resize_insert_3d_from_pointer(pos, rect);
                } else {
                    self.update_draw_3d_direct_drag(pos, rect);
                }
            }
            _ => {}
        }
    }

    pub fn on_pointer_up(&mut self) {
        self.finish_draw_3d_direct_drag();
        let changed = if let Some(sk) = &mut self.sketch {
            let changed = sk.pointer_up([0.0, 0.0]);
            sk.clear_snap();
            changed
        } else {
            false
        };
        if changed {
            // Key-point resize and whole-entity drag must update dependent
            // solids before the user leaves Sketch.
            if !self.sync_active_sketch_edit() {
                // A newly created sketch has no persistent edit body yet, but
                // its committed drag still needs to invalidate the scene.
                self.bump_scene();
            }
        } else {
            self.bump_scene();
        }
    }

    /// 草图平面的局部轴名（角度说明用）。
    pub fn sketch_axis_names(&self) -> (String, String) {
        self.sketch
            .as_ref()
            .map(|s| s.axis_names())
            .unwrap_or_else(|| ("x".to_string(), "y".to_string()))
    }

    /// 取消当前选中的 sketch 实体（Pick 面板 Done 用，不退出 sketch）。
    pub fn sketch_deselect(&mut self) {
        if let Some(sk) = &mut self.sketch {
            sk.deselect();
        }
        self.bump_scene();
    }

    /// 读取某 sketch 实体的 Line 端点（面板显示用）。
    pub fn sketch_line_entity(&self, idx: usize) -> Option<([f64; 2], [f64; 2])> {
        self.sketch.as_ref().and_then(|s| s.line_entity(idx))
    }

    /// 直接更新某 Line 实体的端点（Pick 模式下修改选中段）。
    pub fn sketch_update_line(&mut self, idx: usize, a: [f64; 2], b: [f64; 2]) {
        if let Some(sk) = &mut self.sketch {
            sk.update_line_entity(idx, a, b);
        }
        if !self.sync_active_sketch_edit() {
            self.bump_scene();
        }
    }

    /// 屏幕坐标 → 平面局部坐标（用相机射线与平面求交）。
    pub fn screen_to_plane(&self, pos: egui::Pos2, rect: egui::Rect, plane: &PlaneDef) -> [f64; 2] {
        let ndc = screen_to_ndc(pos, rect);
        let (ray_origin, ray_dir) = self.ray_from_ndc(ndc);
        let hit = ray_plane_intersect(ray_origin, ray_dir, plane);
        hit.map(|p| plane.to_local(p))
            .map(|(u, v)| [u, v])
            .unwrap_or([0.0, 0.0])
    }

    /// 由 NDC 构造世界空间射线。
    pub fn ray_from_ndc(&self, ndc: [f32; 2]) -> ([f64; 3], [f64; 3]) {
        let aspect = (self.viewport_rect.width() / self.viewport_rect.height().max(1.0)).max(1e-4);
        let inv_vp = (self.camera.proj_matrix(aspect) * self.camera.view_matrix()).inverse();
        let ndc_near = glam::Vec4::new(ndc[0], ndc[1], 0.0, 1.0);
        let ndc_far = glam::Vec4::new(ndc[0], ndc[1], 1.0, 1.0);
        let near = inv_vp * ndc_near;
        let far = inv_vp * ndc_far;
        let p_near = near.truncate() / near.w;
        let p_far = far.truncate() / far.w;
        let dir = (p_far - p_near).normalize();
        (
            [p_near.x as f64, p_near.y as f64, p_near.z as f64],
            [dir.x as f64, dir.y as f64, dir.z as f64],
        )
    }

    /// 拾取 body（遍历所有 solid mesh，最近命中）。
    pub fn pick_body_id(&mut self, pos: egui::Pos2, rect: egui::Rect) -> Option<BodyId> {
        let ndc = screen_to_ndc(pos, rect);
        let (ro, rd) = self.ray_from_ndc(ndc);
        // 先收集 solid shape 列表（避免循环内可变借用）
        let solids: Vec<(BodyId, ShapeId)> = self
            .doc
            .bodies
            .iter()
            .filter(|b| b.visible && matches!(b.kind, BodyKind::Solid | BodyKind::Surface))
            .filter_map(|b| b.shape.map(|s| (b.id, s)))
            .collect();
        let mut best: Option<(BodyId, f64)> = None;
        for (id, shape) in solids {
            let mesh = self.mesh_for(shape);
            if let Some(d) = ray_mesh_intersect(ro, rd, &mesh) {
                if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((id, d));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    /// 在选中的 3D 表面上启动草图（跳过 XY/YZ/ZX 平面选择，直接 2D 绘图）。
    pub fn start_sketch_on_face(&mut self, face: &FaceHit) {
        if self.thread_target_for_face(face).is_some() {
            self.toast(
                self.settings
                    .language
                    .text("Choose a planar surface for Sketch", "请选择平面创建草图"),
            );
            return;
        }
        let mut plane = PlaneDef::from_normal([
            face.normal[0] as f64,
            face.normal[1] as f64,
            face.normal[2] as f64,
        ]);
        plane.origin = [
            face.center[0] as f64,
            face.center[1] as f64,
            face.center[2] as f64,
        ];
        self.toast(format!("Sketch on face of body {}", face.body_id));
        self.record_history(format!("Sketch on face of body {}", face.body_id));
        self.face_pick = None; // 面已成为草图平面，清除高亮避免遮挡
        self.start_sketch(plane);
        self.sketch_on_body = Some(face.body_id); // Pocket 自动目标体（在 start_sketch 清空之后设置）
    }

    /// 拾取 solid 表面平面：射线命中三角形 → 共面 patch 扩散（连通合并）。
    pub fn pick_face(&mut self, pos: egui::Pos2, rect: egui::Rect) -> Option<(FaceHit, f64)> {
        let ndc = screen_to_ndc(pos, rect);
        let (ro, rd) = self.ray_from_ndc(ndc);
        let solids: Vec<(BodyId, ShapeId)> = self
            .doc
            .bodies
            .iter()
            .filter(|b| matches!(b.kind, BodyKind::Solid | BodyKind::Surface) && b.visible)
            .filter_map(|b| b.shape.map(|s| (b.id, s)))
            .collect();
        // 1. 最近命中三角形
        let mut best: Option<(BodyId, ShapeId, usize, f64)> = None;
        for (id, shape) in &solids {
            let mesh = self.mesh_for(*shape);
            if let Some((tri, d)) = ray_mesh_tri(ro, rd, &mesh) {
                if best.map(|(_, _, _, bd)| d < bd).unwrap_or(true) {
                    best = Some((*id, *shape, tri, d));
                }
            }
        }
        let (body_id, shape, hit_tri, hit_dist) = best?;
        let mesh = self.mesh_for(shape);
        let tri_count = mesh.triangle_count();
        if hit_tri >= tri_count {
            return None;
        }
        // Cache cylinder queries: hover must not deserialize B-Rep every frame.
        let cylinder = if self.mode == Mode::Idle {
            self.cylinder_targets_for_pick(body_id, shape)
                .into_iter()
                .find(|t| t.contains_triangle(&mesh, &mesh.indices[hit_tri * 3..hit_tri * 3 + 3]))
        } else {
            None
        };
        // 2. Native planar/cylindrical topology, otherwise coplanar flood.
        let norms = mesh.face_normals();
        let n0 = glam::Vec3::from_array(norms[hit_tri]);
        let native_patch = self.kernel.exact_shape(shape).and_then(|exact| {
            let &face = exact.triangle_faces.get(hit_tri)?;
            if !exact
                .planar_faces
                .get(face as usize)
                .copied()
                .unwrap_or(false)
                && !cylinder.as_ref().is_some_and(|t| t.face_id == face)
            {
                return None;
            }
            Some(
                exact
                    .triangle_faces
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &f)| (f == face).then_some(i))
                    .collect::<Vec<_>>(),
            )
        });
        // 顶点位置键（round 1e-3）→ 三角形列表
        let mut vert_map: std::collections::HashMap<(i64, i64, i64), Vec<usize>> =
            std::collections::HashMap::new();
        for (ti, t) in mesh.indices.chunks_exact(3).enumerate() {
            for &vi in t {
                let p = mesh.positions[vi as usize];
                let key = (
                    (p[0] / 1e-3).round() as i64,
                    (p[1] / 1e-3).round() as i64,
                    (p[2] / 1e-3).round() as i64,
                );
                vert_map.entry(key).or_default().push(ti);
            }
        }
        // patch 平面偏移（n · p）
        let mut offs: Vec<f32> = Vec::with_capacity(tri_count);
        for t in mesh.indices.chunks_exact(3) {
            let p = glam::Vec3::from_array(mesh.positions[t[0] as usize]);
            offs.push(n0.dot(p));
        }
        let o0 = offs[hit_tri];
        let mut seen = vec![false; tri_count];
        let mut queue = vec![hit_tri];
        let mut patch: Vec<usize> = Vec::new();
        if let Some(cylinder) = &cylinder {
            // A full wall can consist of several spline/half-cylinder faces.
            // The query validated their combined boundary; do not truncate it
            // back to the representative native face ID here.
            patch = mesh
                .indices
                .chunks_exact(3)
                .enumerate()
                .filter_map(|(i, t)| cylinder.contains_triangle(&mesh, t).then_some(i))
                .collect();
            queue.clear();
        } else if let Some(native_patch) = native_patch {
            patch = native_patch;
            queue.clear();
        }
        while let Some(t) = queue.pop() {
            if seen[t] {
                continue;
            }
            seen[t] = true;
            let n = glam::Vec3::from_array(norms[t]);
            let same_plane = (n.dot(n0) - 1.0).abs() < 1e-3 && (offs[t] - o0).abs() < 1e-2;
            if !same_plane {
                continue;
            }
            patch.push(t);
            let tri = &mesh.indices[t * 3..t * 3 + 3];
            for &vi in tri {
                let p = mesh.positions[vi as usize];
                let key = (
                    (p[0] / 1e-3).round() as i64,
                    (p[1] / 1e-3).round() as i64,
                    (p[2] / 1e-3).round() as i64,
                );
                if let Some(nbrs) = vert_map.get(&key) {
                    for &nt in nbrs {
                        if !seen[nt] {
                            queue.push(nt);
                        }
                    }
                }
            }
        }
        if patch.is_empty() {
            return None;
        }
        // 3. patch 中心与三角形索引
        let mut center = glam::Vec3::ZERO;
        let mut count = 0usize;
        let mut tris: Vec<u32> = Vec::new();
        let mut area_mm2 = 0.0_f64;
        for &t in &patch {
            let triangle = mesh.indices.get(t * 3..t * 3 + 3)?;
            tris.extend_from_slice(triangle);
            let mut points = [glam::DVec3::ZERO; 3];
            for (slot, &vi) in triangle.iter().enumerate() {
                let point = *mesh.positions.get(vi as usize)?;
                center += glam::Vec3::from_array(point);
                points[slot] = glam::DVec3::from_array(point.map(f64::from));
                count += 1;
            }
            area_mm2 += (points[1] - points[0])
                .cross(points[2] - points[0])
                .length()
                * 0.5;
        }
        if !area_mm2.is_finite() || area_mm2 <= 1e-12 {
            return None;
        }
        center /= count.max(1) as f32;
        Some((
            FaceHit {
                body_id,
                normal: n0.to_array(),
                center: center.to_array(),
                triangles: tris,
                area_mm2,
            },
            hit_dist,
        ))
    }
    /// 计算（或从缓存取）一个 sketch body 的平面分割区域。
    /// 悬停/点击会高频调用，实体未变时直接复用缓存。
    fn regions_of(&mut self, id: BodyId) -> Vec<Region2D> {
        let Some(sk) = self.profile_sketch(id) else {
            return Vec::new();
        };
        let fp = s33d_model::entities_fingerprint(&sk.entities);
        if let Some((old_fp, regions)) = self.regions_cache.get(&id) {
            if *old_fp == fp {
                return regions.clone();
            }
        }
        let regions = s33d_model::compute_regions(&sk.entities);
        self.regions_cache.insert(id, (fp, regions.clone()));
        regions
    }

    /// Both rendering and modeling consume the same checked revision cache.
    fn validated_region_mesh(
        &mut self,
        id: BodyId,
        index: usize,
    ) -> Result<Arc<s33d_model::ValidatedRegionMesh>, String> {
        let sketch = self
            .profile_sketch(id)
            .ok_or_else(|| "Sketch profile is unavailable".to_owned())?;
        let revision = s33d_model::entities_fingerprint(&sketch.entities);
        let regions = self.regions_of(id);
        let region = regions
            .get(index)
            .ok_or_else(|| "Selected region is no longer available".to_owned())?;
        let (result, fresh) = self.region_mesh_cache.resolve(id, revision, index, region);
        if fresh {
            if let Err(error) = &result {
                eprintln!("{error}");
                let message = self.settings.language.text(
                    "The selected region failed geometry validation; its outline is kept. See Properties for details.",
                    "所选区域未通过几何验证，已保留轮廓。请查看属性中的详细信息。",
                );
                self.toast(message);
                self.set_hint(message);
            }
        }
        result
    }

    /// Keep an active failure visible after the notification expires. Owner
    /// and geometry checks also protect the frame before bump_scene executes.
    pub(crate) fn selected_region_validation_errors(&self) -> Vec<String> {
        let [owner] = self.selected.as_slice() else {
            return Vec::new();
        };
        if !self.region_selection_is_current(*owner) {
            return Vec::new();
        }
        let Some(sketch) = self.profile_sketch(*owner) else {
            return Vec::new();
        };
        let revision = s33d_model::entities_fingerprint(&sketch.entities);
        self.selected_regions_for(*owner)
            .iter()
            .filter_map(|&index| {
                self.region_mesh_cache
                    .error(*owner, revision, index)
                    .map(str::to_owned)
            })
            .collect()
    }

    /// 拾取 sketch 封闭区域：射线与 sketch 平面求交，返回命中的最小封闭区域。
    /// （与 Super3D `_regionIndexAt` 等价——选中点击位置周围的最小封闭空间。）
    pub fn pick_sketch_region(
        &mut self,
        pos: egui::Pos2,
        rect: egui::Rect,
    ) -> Option<(BodyId, usize, f64)> {
        let ndc = screen_to_ndc(pos, rect);
        let (ro, rd) = self.ray_from_ndc(ndc);
        // 先收集 sketch 数据（避免循环内可变借用）
        let sketches: Vec<(BodyId, PlaneDef)> = self
            .doc
            .bodies
            .iter()
            .filter(|b| b.visible)
            .filter_map(|b| self.profile_sketch(b.id).map(|sk| (b.id, sk.plane)))
            .collect();
        let mut best: Option<(BodyId, usize, f64)> = None;
        for (id, plane) in sketches {
            let regions = self.regions_of(id);
            let Some(hit) = ray_plane_intersect(ro, rd, &plane) else {
                continue;
            };
            let (u, v) = plane.to_local(hit);
            for (i, r) in regions.iter().enumerate() {
                if s33d_model::point_in_region([u, v], r) {
                    let d = dist3(hit, ro);
                    if best.map(|(_, _, bd)| d < bd).unwrap_or(true) {
                        best = Some((id, i, d));
                    }
                }
            }
        }
        best.map(|(id, i, d)| (id, i, d))
    }
    fn pick_body(&mut self, pos: egui::Pos2, rect: egui::Rect, additive: bool) {
        if let Some(id) = self.pick_body_id(pos, rect) {
            self.select(id, additive);
            self.toast(format!(
                "Selected {}",
                self.body(id).map(|b| b.name.clone()).unwrap_or_default()
            ));
        } else {
            self.clear_selection();
        }
    }

    fn pick_plane(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        let ndc = screen_to_ndc(pos, rect);
        let (ro, rd) = self.ray_from_ndc(ndc);
        // 顺序与面板/可视化一致：XY(0) / YZ(1) / ZX(2)
        let order = [DatumPlane::Front, DatumPlane::Right, DatumPlane::Top];
        let h = self.plane_block_size as f64;
        let mut best: Option<(DatumPlane, f64)> = None;
        for plane in order {
            let def = plane.def();
            if let Some(p) = ray_plane_intersect(ro, rd, &def) {
                let (u, v) = def.to_local(p);
                // 命中范围与显示几何一致（三个 h×h 方块）：
                // XY：u∈[0,h]、v∈[0,h]；YZ：u∈[-h,0]、v∈[0,h]；ZX：u∈[0,h]、v∈[-h,0]
                let in_range = match plane {
                    DatumPlane::Top => (0.0..=h).contains(&u) && (-h..=0.0).contains(&v),
                    DatumPlane::Front => (0.0..=h).contains(&u) && (0.0..=h).contains(&v),
                    DatumPlane::Right => (-h..=0.0).contains(&u) && (0.0..=h).contains(&v),
                };
                if in_range {
                    let d = dist3(p, ro);
                    if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                        best = Some((plane, d));
                    }
                }
            }
        }
        if let Some((plane, _)) = best {
            self.start_sketch_on(plane);
        }
    }

    /// 悬停更新（基准面高亮/body 悬停预览）。
    pub fn update_hover(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        let key = (
            pos,
            rect,
            self.scene_version,
            self.mode,
            self.chamfer_active,
        );
        if self.last_hover_query == Some(key) {
            return;
        }
        self.update_hover_uncached(pos, rect);
        self.last_hover_query = Some((
            pos,
            rect,
            self.scene_version,
            self.mode,
            self.chamfer_active,
        ));
    }

    fn update_hover_uncached(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        if self.mode == Mode::PickPlane {
            let ndc = screen_to_ndc(pos, rect);
            let (ro, rd) = self.ray_from_ndc(ndc);
            let h = self.plane_block_size as f64;
            let order = [DatumPlane::Front, DatumPlane::Right, DatumPlane::Top];
            let mut hit = None;
            let mut best_d = f64::INFINITY;
            for (i, plane) in order.iter().enumerate() {
                let def = plane.def();
                if let Some(p) = ray_plane_intersect(ro, rd, &def) {
                    let (u, v) = def.to_local(p);
                    let in_range = match plane {
                        DatumPlane::Top => (0.0..=h).contains(&u) && (-h..=0.0).contains(&v),
                        DatumPlane::Front => (0.0..=h).contains(&u) && (0.0..=h).contains(&v),
                        DatumPlane::Right => (-h..=0.0).contains(&u) && (0.0..=h).contains(&v),
                    };
                    if in_range && dist3(p, ro) < best_d {
                        best_d = dist3(p, ro);
                        hit = Some(i);
                    }
                }
            }
            if hit != self.plane_pick_hover {
                self.plane_pick_hover = hit;
                self.bump_scene();
            }
        } else if self.mode == Mode::Idle {
            if self.chamfer_active {
                // Chamfer/Fillet 模式：悬停识别棱线
                let e = self.pick_edge(pos, rect);
                if e != self.hover_edge {
                    self.hover_edge = e;
                    self.bump_scene();
                }
                return;
            }
            let edge = self.pick_edge(pos, rect);
            if edge != self.hover_edge {
                self.hover_edge = edge;
                self.bump_scene();
            }
            let hover = self.pick_body_id(pos, rect);
            if hover != self.hover_body {
                self.hover_body = hover;
                self.bump_scene();
            }
            // Hover and double-click use the same pocket-sketch overlap rule.
            let face_hit = self.pick_face(pos, rect);
            let region_hit = self.pick_sketch_region(pos, rect);
            let face_hover = self
                .face_precedes_sketch_region(&face_hit, &region_hit)
                .then(|| face_hit.map(|(face, _)| face))
                .flatten();
            let changed = match (&self.hover_face, &face_hover) {
                (Some(a), Some(b)) => {
                    a.body_id != b.body_id
                        || (a.normal[0] - b.normal[0]).abs() > 0.01
                        || (a.normal[1] - b.normal[1]).abs() > 0.01
                        || (a.normal[2] - b.normal[2]).abs() > 0.01
                        || glam::Vec3::from_array(a.center)
                            .distance(glam::Vec3::from_array(b.center))
                            > 0.1
                }
                (None, None) => false,
                _ => true,
            };
            if changed {
                self.hover_face = face_hover;
                self.bump_scene();
            }
        } else if matches!(self.mode, Mode::Mirror | Mode::Split | Mode::SectionView) {
            let face_hover = self.pick_face(pos, rect).map(|(face, _)| face);
            let changed = match (&self.hover_face, &face_hover) {
                (Some(a), Some(b)) => {
                    a.body_id != b.body_id
                        || (a.normal[0] - b.normal[0]).abs() > 0.01
                        || (a.normal[1] - b.normal[1]).abs() > 0.01
                        || (a.normal[2] - b.normal[2]).abs() > 0.01
                        || glam::Vec3::from_array(a.center)
                            .distance(glam::Vec3::from_array(b.center))
                            > 0.1
                }
                (None, None) => false,
                _ => true,
            };
            if changed {
                self.hover_face = face_hover;
                self.bump_scene();
            }
            let cleared_body = self.hover_body.take().is_some();
            let cleared_edge = self.hover_edge.take().is_some();
            if cleared_body || cleared_edge {
                self.bump_scene();
            }
        } else if matches!(self.mode, Mode::Boolean | Mode::Insert3D) {
            let hover = self.pick_body_id(pos, rect);
            if hover != self.hover_body {
                self.hover_body = hover;
                self.bump_scene();
            }
            let cleared_edge = self.hover_edge.take().is_some();
            let cleared_face = self.hover_face.take().is_some();
            if cleared_edge || cleared_face {
                self.bump_scene();
            }
        }
    }

    pub fn clear_hover_preview(&mut self) {
        self.last_hover_query = None;
        self.last_click = None;
        self.click_count = 0;
        self.click_selection_base = None;
        let body = self.hover_body.take().is_some();
        let face = self.hover_face.take().is_some();
        let edge = self.hover_edge.take().is_some();
        let plane = self.plane_pick_hover.take().is_some();
        let changed = body || face || edge || plane;
        if changed {
            self.bump_scene();
        }
    }
}

/// egui's bundled fonts do not contain CJK glyphs. Keep the bundled Latin
/// faces first (so the existing visual proportions stay unchanged) and add a
/// Windows CJK face as the last-resort glyph source for both UI font families.
fn install_ui_fonts(ctx: &egui::Context) {
    ctx.set_fonts(load_ui_fonts());
}

pub(crate) fn load_ui_fonts() -> egui::FontDefinitions {
    let cjk_candidates = [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simsun.ttc",
    ];
    let mut fonts = egui::FontDefinitions::default();

    let cjk_name = cjk_candidates
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .map(|bytes| {
            let name = "s33d-cjk-fallback".to_owned();
            fonts
                .font_data
                .insert(name.clone(), egui::FontData::from_owned(bytes).into());
            name
        });
    if let Some(name) = cjk_name.as_ref() {
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }

    // Calculator math glyphs must not fall through to a CJK face: symbols
    // such as π, √, superscripts and backspace otherwise look inconsistent or
    // become empty squares. Give that window a dedicated Segoe family while
    // leaving the rest of the application's typography unchanged.
    let mut calculator_fonts = Vec::new();
    for (name, path) in [
        ("s33d-segoe-ui", r"C:\Windows\Fonts\segoeui.ttf"),
        ("s33d-segoe-symbol", r"C:\Windows\Fonts\seguisym.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_owned(), egui::FontData::from_owned(bytes).into());
            calculator_fonts.push(name.to_owned());
        }
    }
    calculator_fonts.extend(
        fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts.families.insert(
        egui::FontFamily::Name(Arc::from("s33d-calculator")),
        calculator_fonts,
    );
    // Use actual bold glyphs for onboarding hierarchy; RichText::strong alone
    // changes emphasis color in egui, not the font weight. System fonts remain
    // optional, with the existing proportional/CJK chain as portable fallback.
    let mut onboarding_bold = Vec::new();
    for (name, path) in [
        ("s33d-ui-bold-latin", r"C:\Windows\Fonts\segoeuib.ttf"),
        ("s33d-ui-bold-cjk", r"C:\Windows\Fonts\msyhbd.ttc"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_owned(), egui::FontData::from_owned(bytes).into());
            onboarding_bold.push(name.to_owned());
        }
    }
    onboarding_bold.extend(fonts.families[&egui::FontFamily::Proportional].clone());
    fonts.families.insert(
        egui::FontFamily::Name(Arc::from("s33d-onboarding-bold")),
        onboarding_bold,
    );
    fonts
}

/// 一条世界空间线段投影到屏幕后，与鼠标位置的最近距离（像素）。
fn edge_screen_dist(
    view_proj: &glam::Mat4,
    rect: egui::Rect,
    mouse: egui::Pos2,
    a: glam::Vec3,
    b: glam::Vec3,
) -> f32 {
    let project = |p: glam::Vec3| -> egui::Pos2 {
        let clip = *view_proj * p.extend(1.0);
        if clip.w <= 1e-6 {
            return egui::pos2(-1e9, -1e9);
        }
        let ndc = clip.truncate() / clip.w;
        egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        )
    };
    let pa = project(a);
    let pb = project(b);
    let ab = pb - pa;
    let l2 = ab.length_sq();
    let t = if l2 > 1e-9 {
        (((mouse.x - pa.x) * ab.x + (mouse.y - pa.y) * ab.y) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = egui::pos2(pa.x + ab.x * t, pa.y + ab.y * t);
    (egui::vec2(mouse.x - q.x, mouse.y - q.y)).length()
}

type FaceVertexKey = (i64, i64, i64);
type FaceEdgeKey = (FaceVertexKey, FaceVertexKey);

fn quantized_face_vertex(point: [f32; 3]) -> FaceVertexKey {
    const SCALE: f32 = 1_000.0;
    (
        (point[0] * SCALE).round() as i64,
        (point[1] * SCALE).round() as i64,
        (point[2] * SCALE).round() as i64,
    )
}

fn ordered_face_edge(a: FaceVertexKey, b: FaceVertexKey) -> FaceEdgeKey {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// 从选中共面三角 patch 恢复闭合外边界与孔边界。
/// 位置键与面拾取使用相同的 1e-3 mm 容差，可跨已拆分的渲染顶点合并边。
/// Resolve a whole picked patch against the immutable native face map. Display
/// welding changes vertex indices; use the same position keys as weld_hard.
fn exact_face_for_hit(
    exact: &s33d_kernel::occt::ExactShape,
    display: &MeshData,
    hit: &FaceHit,
) -> Option<u32> {
    use std::collections::{HashMap, HashSet};
    let key = |mesh: &MeshData, tri: &[u32]| -> Option<[[i64; 3]; 3]> {
        let mut points = [[0; 3]; 3];
        for i in 0..3 {
            points[i] = mesh
                .positions
                .get(tri[i] as usize)?
                .map(|v| (v / 1e-5).round() as i64);
        }
        points.sort_unstable();
        (points[0] != points[1] && points[1] != points[2]).then_some(points)
    };
    let selected: HashSet<_> = hit
        .triangles
        .chunks_exact(3)
        .filter_map(|t| key(display, t))
        .collect();
    if selected.is_empty() {
        return None;
    }
    let mut faces: HashMap<u32, HashSet<_>> = HashMap::new();
    for (tri, &face) in exact
        .mesh
        .indices
        .chunks_exact(3)
        .zip(&exact.triangle_faces)
    {
        if let Some(key) = key(&exact.mesh, tri) {
            faces.entry(face).or_default().insert(key);
        }
    }
    // Equality prevents extending only a fragment or an unrelated coplanar face.
    faces
        .into_iter()
        .find_map(|(face, triangles)| (triangles == selected).then_some(face))
}

fn bounded_planar_face_profile(
    mesh: &MeshData,
    face: &FaceHit,
) -> std::result::Result<(PlaneDef, Region2D), String> {
    if face.triangles.len() < 3 || face.triangles.len() % 3 != 0 {
        return Err("Selected surface has no usable triangle boundary".to_string());
    }
    let normal = glam::DVec3::new(
        face.normal[0] as f64,
        face.normal[1] as f64,
        face.normal[2] as f64,
    );
    if !normal.is_finite() || normal.length_squared() < 1e-12 {
        return Err("Selected surface has no stable plane".to_string());
    }
    let mut plane = PlaneDef::from_normal(normal.normalize().to_array());
    plane.origin = [
        face.center[0] as f64,
        face.center[1] as f64,
        face.center[2] as f64,
    ];

    let mut edge_counts: HashMap<FaceEdgeKey, usize> = HashMap::new();
    let mut positions: HashMap<FaceVertexKey, [f32; 3]> = HashMap::new();
    for triangle in face.triangles.chunks_exact(3) {
        let mut keys = [(0, 0, 0); 3];
        for index in 0..3 {
            let vertex = *mesh
                .positions
                .get(triangle[index] as usize)
                .ok_or_else(|| "Selected surface contains an invalid mesh index".to_string())?;
            let key = quantized_face_vertex(vertex);
            keys[index] = key;
            positions.entry(key).or_insert(vertex);
        }
        for (a, b) in [(keys[0], keys[1]), (keys[1], keys[2]), (keys[2], keys[0])] {
            if a != b {
                *edge_counts.entry(ordered_face_edge(a, b)).or_default() += 1;
            }
        }
    }
    if edge_counts.values().any(|count| *count > 2) {
        return Err("Selected surface boundary is non-manifold".to_string());
    }
    let boundary: Vec<FaceEdgeKey> = edge_counts
        .into_iter()
        .filter_map(|(edge, count)| (count == 1).then_some(edge))
        .collect();
    if boundary.len() < 3 {
        return Err("Selected surface has no fixed closed boundary".to_string());
    }

    let mut adjacency: HashMap<FaceVertexKey, Vec<FaceVertexKey>> = HashMap::new();
    for &(a, b) in &boundary {
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    if adjacency.values().any(|neighbors| neighbors.len() != 2) {
        return Err("Selected surface boundary is open or branched".to_string());
    }

    let mut unused: std::collections::HashSet<FaceEdgeKey> = boundary.into_iter().collect();
    let mut rings = Vec::<Vec<[f64; 2]>>::new();
    while let Some(&(start, first)) = unused.iter().next() {
        unused.remove(&ordered_face_edge(start, first));
        let mut ring_keys = vec![start];
        let mut current = first;
        let mut guard = 0usize;
        while current != start {
            ring_keys.push(current);
            guard += 1;
            if guard > adjacency.len() + 1 {
                return Err("Selected surface boundary could not be closed".to_string());
            }
            let next = adjacency
                .get(&current)
                .and_then(|neighbors| {
                    neighbors
                        .iter()
                        .copied()
                        .find(|candidate| unused.contains(&ordered_face_edge(current, *candidate)))
                })
                .ok_or_else(|| "Selected surface boundary is open".to_string())?;
            unused.remove(&ordered_face_edge(current, next));
            current = next;
        }
        if ring_keys.len() < 3 {
            return Err("Selected surface boundary is degenerate".to_string());
        }
        let ring: Vec<[f64; 2]> = ring_keys
            .into_iter()
            .map(|key| {
                let point = positions[&key];
                let local = plane.to_local([point[0] as f64, point[1] as f64, point[2] as f64]);
                [local.0, local.1]
            })
            .collect();
        if s33d_kernel::region::signed_area(&ring).abs() <= 1e-10 {
            return Err("Selected surface boundary has zero area".to_string());
        }
        rings.push(ring);
    }
    rings.sort_by(|left, right| {
        s33d_kernel::region::signed_area(right)
            .abs()
            .partial_cmp(&s33d_kernel::region::signed_area(left).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut outer = rings.remove(0);
    if s33d_kernel::region::signed_area(&outer) < 0.0 {
        outer.reverse();
    }
    let shell = Region2D::new(outer.clone());
    let mut holes = Vec::new();
    for mut ring in rings {
        if !s33d_model::point_in_region(ring[0], &shell) {
            return Err("Selected patch contains disconnected surface regions".to_string());
        }
        if s33d_kernel::region::signed_area(&ring) > 0.0 {
            ring.reverse();
        }
        holes.push(ring);
    }
    Ok((plane, Region2D { outer, holes }))
}

fn sketch_from_surface_region(
    plane: PlaneDef,
    region: &Region2D,
    body_id: BodyId,
) -> SketchFeature {
    let mut entities = Vec::new();
    for ring in std::iter::once(&region.outer).chain(region.holes.iter()) {
        for index in 0..ring.len() {
            entities.push(SketchEntity::Line {
                a: ring[index],
                b: ring[(index + 1) % ring.len()],
            });
        }
    }
    SketchFeature {
        vertex_locks: Default::default(),
        plane,
        entities,
        world_xyz_coordinates: false,
        on_body: Some(body_id),
    }
}

fn apply_workspace_owner_selection(selected: &mut Vec<BodyId>, id: BodyId, additive: bool) -> bool {
    if additive {
        if let Some(position) = selected.iter().position(|selected| *selected == id) {
            selected.remove(position);
            return false;
        }
        selected.push(id);
    } else {
        selected.clear();
        selected.push(id);
    }
    true
}

fn next_boolean_operand_slot(
    target: Option<BodyId>,
    tool: Option<BodyId>,
) -> Option<BooleanOperandSlot> {
    if target.is_none() {
        Some(BooleanOperandSlot::Target)
    } else if tool.is_none() {
        Some(BooleanOperandSlot::Tool)
    } else {
        None
    }
}

fn array_step_ranges(axes: [bool; 3], counts_including_original: [u32; 3]) -> [u32; 3] {
    std::array::from_fn(|axis| {
        if axes[axis] {
            counts_including_original[axis].max(1).saturating_sub(1)
        } else {
            0
        }
    })
}

fn array_total_objects(step_ranges: [u32; 3]) -> u64 {
    step_ranges
        .iter()
        .map(|steps| u64::from(*steps) + 1)
        .product()
}

/// hover 面是否与已选面不同（避免重复渲染两层高亮）。
fn face_pick_none_or_different(pick: &Option<FaceHit>, hover: &FaceHit) -> bool {
    match pick {
        None => true,
        Some(p) => {
            p.body_id != hover.body_id
                || (p.triangles != hover.triangles
                    && ((p.normal[0] - hover.normal[0]).abs() > 0.01
                        || (p.normal[1] - hover.normal[1]).abs() > 0.01
                        || (p.normal[2] - hover.normal[2]).abs() > 0.01))
        }
    }
}

/// 将 `ax + by + cz = d` 转成内核平面；origin 取平面上距世界原点最近的点。
fn plane_from_equation(equation: [f64; 4]) -> std::result::Result<PlaneDef, &'static str> {
    let normal = glam::DVec3::new(equation[0], equation[1], equation[2]);
    if !normal.is_finite() || !equation[3].is_finite() || normal.length_squared() <= 1e-12 {
        return Err("Plane equation needs at least one non-zero finite coefficient");
    }
    let origin = normal * (equation[3] / normal.length_squared());
    let mut plane = PlaneDef::from_normal(normal.to_array());
    plane.origin = origin.to_array();
    Ok(plane)
}

/// 将既有参数化特征安全识别为 Draw 3D 基础体。
/// 只接受无孔、单轮廓、完整 360° 的解析形状，避免把自由造型误判后改坏。
/// Freeze only known parameter provenance; cylinder selection still certifies
/// these parameters against the destination geometry before using them.
fn copy_primitive_snapshot(feature: &Feature, offset: [f64; 3]) -> Option<Primitive3DParams> {
    let mut params = match feature {
        Feature::Derived {
            operation:
                Some(DerivedOperation::Copy {
                    primitive_snapshot, ..
                }),
            ..
        } => primitive_snapshot.clone(),
        _ => draw_3d_params_from_feature(feature),
    }?;
    for axis in 0..3 {
        params.position[axis] += offset[axis];
    }
    Some(params)
}

fn draw_3d_params_from_feature(feature: &Feature) -> Option<Primitive3DParams> {
    match feature {
        Feature::Primitive3D { params } => Some(*params),
        Feature::Extrude { sketch, params, .. } => recognize_extruded_primitive(sketch, params),
        Feature::Revolve { sketch, params, .. } => recognize_revolved_primitive(sketch, params),
        _ => None,
    }
}

fn recognize_extruded_primitive(
    sketch: &SketchFeature,
    extrude: &ExtrudeParams,
) -> Option<Primitive3DParams> {
    if sketch.on_body.is_some()
        || sketch.entities.len() != 1
        || extrude.symmetric
        || extrude.bevel.abs() > 1e-8
        || !extrude.depth.is_finite()
        || extrude.depth.abs() <= 1e-6
    {
        return None;
    }

    let mut frame = sketch.plane.clone();
    let mut result = match &sketch.entities[0] {
        SketchEntity::Rect { c1, c2 } => {
            let width = (c2[0] - c1[0]).abs();
            let depth = (c2[1] - c1[1]).abs();
            if width <= 1e-6 || depth <= 1e-6 {
                return None;
            }
            let center = [(c1[0] + c2[0]) * 0.5, (c1[1] + c2[1]) * 0.5];
            frame.origin = sketch.plane.to_world(center[0], center[1]);
            let mut params = Primitive3DParams::defaults(Primitive3DKind::Box);
            params.width = width;
            params.depth = depth;
            params.height = extrude.depth.abs();
            params
        }
        SketchEntity::RotatedRect {
            c,
            width,
            height,
            rot,
        } => {
            if *width <= 1e-6 || *height <= 1e-6 {
                return None;
            }
            let (sin, cos) = rot.sin_cos();
            let plane_x = sketch.plane.x_axis();
            let plane_y = sketch.plane.y_axis();
            frame.origin = sketch.plane.to_world(c[0], c[1]);
            frame.x_axis = (plane_x * cos + plane_y * sin).to_array();
            frame.y_axis = (-plane_x * sin + plane_y * cos).to_array();
            let mut params = Primitive3DParams::defaults(Primitive3DKind::Box);
            params.width = *width;
            params.depth = *height;
            params.height = extrude.depth.abs();
            params
        }
        SketchEntity::Polygon { c, r, n: 4, rot } => {
            if !r.is_finite() || *r <= 1e-6 {
                return None;
            }
            let points: Vec<[f64; 2]> = (0..4)
                .map(|index| {
                    let angle = *rot + index as f64 * std::f64::consts::FRAC_PI_2;
                    [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
                })
                .collect();
            let edge_x = glam::DVec2::from_array(points[1]) - glam::DVec2::from_array(points[0]);
            let edge_y = glam::DVec2::from_array(points[2]) - glam::DVec2::from_array(points[1]);
            let local_x = edge_x.normalize();
            let local_y = edge_y.normalize();
            let plane_x = sketch.plane.x_axis();
            let plane_y = sketch.plane.y_axis();
            frame.origin = sketch.plane.to_world(c[0], c[1]);
            frame.x_axis = (plane_x * local_x.x + plane_y * local_x.y).to_array();
            frame.y_axis = (plane_x * local_y.x + plane_y * local_y.y).to_array();
            let mut params = Primitive3DParams::defaults(Primitive3DKind::Box);
            params.width = edge_x.length();
            params.depth = edge_y.length();
            params.height = extrude.depth.abs();
            params
        }
        SketchEntity::Circle { c, r } => {
            if !r.is_finite() || *r <= 1e-6 {
                return None;
            }
            frame.origin = sketch.plane.to_world(c[0], c[1]);
            let mut params = Primitive3DParams::defaults(Primitive3DKind::Cylinder);
            params.radius = *r;
            params.height = extrude.depth.abs();
            params
        }
        _ => return None,
    };

    // 负深度沿相反法线生成；翻转一个平面内轴即可让 Draw 3D 的正高度保持原方向。
    if extrude.depth < 0.0 {
        frame.y_axis = frame.y_axis.map(|value| -value);
    }
    result.position = frame.origin;
    result.frame = Some(frame);
    Some(result)
}

fn recognize_revolved_primitive(
    sketch: &SketchFeature,
    revolve: &RevolveParams,
) -> Option<Primitive3DParams> {
    if sketch.on_body.is_some()
        || !revolve.angle_deg.is_finite()
        || (revolve.angle_deg.abs() - 360.0).abs() > 1e-5
    {
        return None;
    }
    let regions = s33d_model::compute_regions(&sketch.entities);
    if regions.len() != 1 || !regions[0].holes.is_empty() {
        return None;
    }
    recognize_revolved_region(&regions[0], &sketch.plane, revolve)
}

fn recognize_revolved_region(
    region: &Region2D,
    plane: &PlaneDef,
    revolve: &RevolveParams,
) -> Option<Primitive3DParams> {
    if region.outer.len() < 3 {
        return None;
    }
    let axis = glam::DVec2::from_array(revolve.axis_dir);
    if !axis.is_finite() || axis.length_squared() <= 1e-12 {
        return None;
    }
    let axis = axis.normalize();
    let axis_point = glam::DVec2::from_array(revolve.axis_point);
    let mut radial = glam::DVec2::new(axis.y, -axis.x);
    let signed: Vec<f64> = region
        .outer
        .iter()
        .map(|point| (glam::DVec2::from_array(*point) - axis_point).dot(radial))
        .collect();
    let signed_min = signed.iter().copied().fold(f64::INFINITY, f64::min);
    let signed_max = signed.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let extent = region
        .outer
        .iter()
        .flat_map(|point| [point[0].abs(), point[1].abs()])
        .fold(1.0_f64, f64::max);
    let tol = (extent * 1e-4).max(1e-6);
    if signed_min < -tol && signed_max > tol {
        return None;
    }
    if signed_max <= tol && signed_min < 0.0 {
        radial = -radial;
    }

    let profile: Vec<(f64, f64)> = region
        .outer
        .iter()
        .map(|point| {
            let offset = glam::DVec2::from_array(*point) - axis_point;
            (offset.dot(radial), offset.dot(axis))
        })
        .collect();
    if profile
        .iter()
        .any(|(rho, z)| !rho.is_finite() || !z.is_finite() || *rho < -tol)
    {
        return None;
    }
    let rho_min = profile
        .iter()
        .map(|(rho, _)| *rho)
        .fold(f64::INFINITY, f64::min)
        .max(0.0);
    let rho_max = profile
        .iter()
        .map(|(rho, _)| *rho)
        .fold(f64::NEG_INFINITY, f64::max);
    let z_min = profile
        .iter()
        .map(|(_, z)| *z)
        .fold(f64::INFINITY, f64::min);
    let z_max = profile
        .iter()
        .map(|(_, z)| *z)
        .fold(f64::NEG_INFINITY, f64::max);
    let height = z_max - z_min;
    if rho_max <= tol || height <= tol {
        return None;
    }
    let profile_tol = (rho_max.max(height) * 2e-3).max(tol * 4.0);

    // 离轴圆形截面完整旋转为 Torus。
    if rho_min > profile_tol {
        let major = (rho_min + rho_max) * 0.5;
        let minor = (rho_max - rho_min) * 0.5;
        let center_z = (z_min + z_max) * 0.5;
        if minor <= profile_tol
            || (height * 0.5 - minor).abs() > profile_tol
            || profile.iter().any(|(rho, z)| {
                (((rho - major).powi(2) + (z - center_z).powi(2)).sqrt() - minor).abs()
                    > profile_tol
            })
        {
            return None;
        }
        let frame = revolve_frame(plane, axis_point + axis * z_min, radial, axis);
        let mut params = Primitive3DParams::defaults(Primitive3DKind::Torus);
        params.position = frame.origin;
        params.frame = Some(frame);
        params.major_radius = major;
        params.minor_radius = minor;
        return Some(params);
    }

    // 半圆 + 轴线形成的半圆盘完整旋转为 Sphere。
    let sphere_radius = height * 0.5;
    let sphere_center_z = (z_min + z_max) * 0.5;
    let curved_points = profile.iter().filter(|(rho, _)| *rho > profile_tol).count();
    let is_sphere = curved_points >= 6
        && (rho_max - sphere_radius).abs() <= profile_tol
        && profile.iter().all(|(rho, z)| {
            *rho <= profile_tol
                || ((rho.powi(2) + (z - sphere_center_z).powi(2)).sqrt() - sphere_radius).abs()
                    <= profile_tol
        });
    if is_sphere {
        let frame = revolve_frame(plane, axis_point + axis * z_min, radial, axis);
        let mut params = Primitive3DParams::defaults(Primitive3DKind::Sphere);
        params.position = frame.origin;
        params.frame = Some(frame);
        params.radius = sphere_radius;
        return Some(params);
    }

    // 轴线 + 一条直母线 + 两端盖形成 Cylinder / Cone / Frustum。
    let end_tol = (height * 1e-4).max(tol);
    let bottom_radius = profile
        .iter()
        .filter(|(_, z)| (*z - z_min).abs() <= end_tol)
        .map(|(rho, _)| *rho)
        .fold(0.0_f64, f64::max);
    let top_radius = profile
        .iter()
        .filter(|(_, z)| (*z - z_max).abs() <= end_tol)
        .map(|(rho, _)| *rho)
        .fold(0.0_f64, f64::max);
    let axis_reaches_bottom = profile
        .iter()
        .any(|(rho, z)| *rho <= profile_tol && (*z - z_min).abs() <= end_tol);
    let axis_reaches_top = profile
        .iter()
        .any(|(rho, z)| *rho <= profile_tol && (*z - z_max).abs() <= end_tol);
    if !axis_reaches_bottom
        || !axis_reaches_top
        || bottom_radius.max(top_radius) <= profile_tol
        || profile.iter().any(|(rho, z)| {
            let t = ((*z - z_min) / height).clamp(0.0, 1.0);
            let outer = bottom_radius + (top_radius - bottom_radius) * t;
            let on_axis = *rho <= profile_tol;
            let on_end = (*z - z_min).abs() <= end_tol || (*z - z_max).abs() <= end_tol;
            let on_outer = (*rho - outer).abs() <= profile_tol;
            !(on_axis || on_end || on_outer)
        })
    {
        return None;
    }

    let reverse = bottom_radius < top_radius;
    let (base_point, frame_axis, bottom_radius, top_radius) = if reverse {
        (z_max, -axis, top_radius, bottom_radius)
    } else {
        (z_min, axis, bottom_radius, top_radius)
    };
    let cylinder = (bottom_radius - top_radius).abs() <= profile_tol;
    let origin_2d = axis_point + axis * base_point;
    let frame = if cylinder {
        extrusion_frame_from_revolve(plane, origin_2d, radial, frame_axis)
    } else {
        revolve_frame(plane, origin_2d, radial, frame_axis)
    };
    let mut params = Primitive3DParams::defaults(if cylinder {
        Primitive3DKind::Cylinder
    } else {
        Primitive3DKind::Cone
    });
    params.position = frame.origin;
    params.frame = Some(frame);
    params.height = height;
    params.radius = bottom_radius;
    params.top_radius = top_radius.max(0.0);
    Some(params)
}

fn revolve_frame(
    plane: &PlaneDef,
    origin_2d: glam::DVec2,
    radial: glam::DVec2,
    axis: glam::DVec2,
) -> PlaneDef {
    let plane_x = plane.x_axis();
    let plane_y = plane.y_axis();
    PlaneDef {
        origin: plane.to_world(origin_2d.x, origin_2d.y),
        x_axis: (plane_x * radial.x + plane_y * radial.y)
            .normalize()
            .to_array(),
        y_axis: (plane_x * axis.x + plane_y * axis.y).normalize().to_array(),
    }
}

fn extrusion_frame_from_revolve(
    plane: &PlaneDef,
    origin_2d: glam::DVec2,
    radial: glam::DVec2,
    axis: glam::DVec2,
) -> PlaneDef {
    let plane_x = plane.x_axis();
    let plane_y = plane.y_axis();
    let x = (plane_x * radial.x + plane_y * radial.y).normalize();
    let normal = (plane_x * axis.x + plane_y * axis.y).normalize();
    let y = normal.cross(x).normalize();
    PlaneDef {
        origin: plane.to_world(origin_2d.x, origin_2d.y),
        x_axis: x.to_array(),
        y_axis: y.to_array(),
    }
}

fn insert_3d_circle(center: [f64; 2], radius: f64, segments: usize) -> Vec<[f64; 2]> {
    let segments = segments.max(3);
    (0..segments)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            [
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]
        })
        .collect()
}

fn draw_3d_base_frame(params: &Primitive3DParams) -> PlaneDef {
    let mut frame = if matches!(
        params.kind,
        Primitive3DKind::Box | Primitive3DKind::Cylinder
    ) {
        params.frame.unwrap_or_else(|| DatumPlane::Top.def())
    } else {
        // Revolved primitives use profile Y as their height/rotation axis.
        let profile = params.frame.unwrap_or_else(|| DatumPlane::Front.def());
        PlaneDef {
            origin: params.position,
            x_axis: profile.x_axis,
            y_axis: (-profile.normal()).to_array(),
        }
    };
    frame.origin = params.position;
    frame
}

/// Desktop spheres use their center; persistent/Agent position remains the
/// legacy base to keep old projects and scripts geometrically unchanged.
pub(super) fn draw_3d_reference_point(params: &Primitive3DParams) -> [f64; 3] {
    let origin = glam::DVec3::from_array(params.position);
    if params.kind == Primitive3DKind::Sphere {
        (origin + draw_3d_base_frame(params).normal() * params.radius).to_array()
    } else {
        params.position
    }
}

pub(super) fn set_draw_3d_reference_point(params: &mut Primitive3DParams, point: [f64; 3]) {
    params.position = if params.kind == Primitive3DKind::Sphere {
        (glam::DVec3::from_array(point) - draw_3d_base_frame(params).normal() * params.radius)
            .to_array()
    } else {
        point
    };
}

fn sphere_preview_mesh(params: &Primitive3DParams) -> Result<MeshData, String> {
    if !params.radius.is_finite()
        || params.radius <= 0.0
        || params.position.iter().any(|v| !v.is_finite())
    {
        return Err("Invalid sphere radius or position".into());
    }
    let frame = draw_3d_base_frame(params);
    let center = glam::DVec3::from_array(draw_3d_reference_point(params));
    let mut normals = vec![frame.normal()];
    const S: usize = 64;
    const R: usize = 32;
    for i in 1..R {
        let theta = std::f64::consts::PI * i as f64 / R as f64;
        for j in 0..S {
            let phi = std::f64::consts::TAU * j as f64 / S as f64;
            normals.push(
                frame.normal() * theta.cos()
                    + (frame.x_axis() * phi.cos() + frame.y_axis() * phi.sin()) * theta.sin(),
            );
        }
    }
    normals.push(-frame.normal());
    let mut indices = Vec::new();
    let at = |i: usize, j: usize| (1 + (i - 1) * S + j % S) as u32;
    let bottom = (normals.len() - 1) as u32;
    for j in 0..S {
        indices.extend_from_slice(&[0, at(1, j), at(1, j + 1)]);
        for i in 1..R - 1 {
            indices.extend_from_slice(&[
                at(i, j),
                at(i + 1, j),
                at(i + 1, j + 1),
                at(i, j),
                at(i + 1, j + 1),
                at(i, j + 1),
            ]);
        }
        indices.extend_from_slice(&[bottom, at(R - 1, j + 1), at(R - 1, j)]);
    }
    for tri in indices.chunks_exact_mut(3) {
        let a = normals[tri[0] as usize];
        let b = normals[tri[1] as usize];
        let c = normals[tri[2] as usize];
        if (b - a).cross(c - a).dot(a + b + c) < 0.0 {
            tri.swap(1, 2);
        }
    }
    let mut mesh = MeshData::new(
        normals
            .iter()
            .map(|n| (center + *n * params.radius).as_vec3().to_array())
            .collect(),
        indices,
    );
    mesh.vertex_normals = Some(normals.iter().map(|n| n.as_vec3().to_array()).collect());
    Ok(mesh)
}

/// World-space handles for direct manipulation of a Draw 3D preview. Keeping
/// these derived from `Primitive3DParams` makes rendering and hit testing use
/// exactly the same control points.
fn draw_3d_control_points(params: &Primitive3DParams) -> Vec<(Draw3DDragKind, [f64; 3])> {
    let frame = draw_3d_base_frame(params);
    let origin = frame.origin();
    let x = frame.x_axis();
    let depth = frame.y_axis();
    let up = frame.normal();
    let point = |value: glam::DVec3| value.to_array();
    let mut handles = vec![(Draw3DDragKind::Move, draw_3d_reference_point(params))];
    match params.kind {
        Primitive3DKind::Box => {
            handles.push((
                Draw3DDragKind::Width,
                point(origin + x * params.width * 0.5),
            ));
            handles.push((
                Draw3DDragKind::Depth,
                point(origin + depth * params.depth * 0.5),
            ));
            handles.push((
                Draw3DDragKind::PlanarSize,
                point(origin + x * params.width * 0.5 + depth * params.depth * 0.5),
            ));
            handles.push((Draw3DDragKind::Height, point(origin + up * params.height)));
        }
        Primitive3DKind::Cylinder => {
            handles.push((Draw3DDragKind::Radius, point(origin + x * params.radius)));
            handles.push((Draw3DDragKind::Height, point(origin + up * params.height)));
        }
        Primitive3DKind::Sphere => {
            let center = origin + up * params.radius;
            handles.push((Draw3DDragKind::Radius, point(center + x * params.radius)));
        }
        Primitive3DKind::Cone => {
            handles.push((Draw3DDragKind::Radius, point(origin + x * params.radius)));
            handles.push((Draw3DDragKind::Height, point(origin + up * params.height)));
        }
        Primitive3DKind::Torus => {
            let center_height = origin + up * params.minor_radius;
            handles.push((
                Draw3DDragKind::MajorRadius,
                point(center_height + x * params.major_radius),
            ));
            handles.push((
                Draw3DDragKind::MinorRadius,
                point(center_height + x * (params.major_radius + params.minor_radius)),
            ));
        }
    }
    handles
}

fn draw_3d_dragged_params(
    mut params: Primitive3DParams,
    kind: Draw3DDragKind,
    planar_delta: [f64; 2],
    height_delta: f64,
    minimum: f64,
) -> Primitive3DParams {
    let reference = draw_3d_reference_point(&params);
    let minimum = minimum.max(1e-4);
    match kind {
        Draw3DDragKind::Move => {}
        Draw3DDragKind::Width => {
            params.width = (params.width + 2.0 * planar_delta[0]).max(minimum);
        }
        Draw3DDragKind::Depth => {
            params.depth = (params.depth + 2.0 * planar_delta[1]).max(minimum);
        }
        Draw3DDragKind::PlanarSize => {
            params.width = (params.width + 2.0 * planar_delta[0]).max(minimum);
            params.depth = (params.depth + 2.0 * planar_delta[1]).max(minimum);
        }
        Draw3DDragKind::Height => {
            params.height = (params.height + height_delta).max(minimum);
        }
        Draw3DDragKind::Radius => {
            params.radius = (params.radius + planar_delta[0]).max(minimum);
            if params.kind == Primitive3DKind::Cone {
                params.top_radius = params.top_radius.min(params.radius);
            }
        }
        Draw3DDragKind::MajorRadius => {
            params.major_radius =
                (params.major_radius + planar_delta[0]).max(params.minor_radius + minimum);
        }
        Draw3DDragKind::MinorRadius => {
            let upper = (params.major_radius - minimum * 0.25).max(minimum * 0.5);
            params.minor_radius =
                (params.minor_radius + planar_delta[0]).clamp(minimum * 0.25, upper);
        }
    }
    if params.kind == Primitive3DKind::Sphere {
        set_draw_3d_reference_point(&mut params, reference);
    }
    params
}

/// Turn a Draw 3D center point plus a second ground-plane point into useful
/// initial dimensions. The right-side precision editor remains authoritative.
fn draw_3d_drag_params(
    mut params: Primitive3DParams,
    anchor: [f64; 3],
    current: [f64; 3],
    minimum: f64,
) -> Primitive3DParams {
    let dx = current[0] - anchor[0];
    let dz = current[2] - anchor[2];
    let radial = dx.hypot(dz).max(minimum);
    match params.kind {
        Primitive3DKind::Box => {
            params.width = (2.0 * dx.abs()).max(minimum);
            params.depth = (2.0 * dz.abs()).max(minimum);
            params.height = params.width.max(params.depth).max(minimum);
        }
        Primitive3DKind::Cylinder => {
            params.radius = radial;
            params.height = (2.0 * radial).max(minimum);
        }
        Primitive3DKind::Sphere => {
            params.radius = radial;
        }
        Primitive3DKind::Cone => {
            params.radius = radial;
            params.height = (2.0 * radial).max(minimum);
            params.top_radius = params.top_radius.min(params.radius);
        }
        Primitive3DKind::Torus => {
            params.major_radius = radial;
            params.minor_radius = (radial * 0.25).max(minimum * 0.25);
            if params.minor_radius >= params.major_radius {
                params.minor_radius = params.major_radius * 0.5;
            }
        }
    }
    set_draw_3d_reference_point(&mut params, anchor);
    params
}

/// 计算可从任意草图面位置贯穿目标体的 Pocket cutter。
///
/// 返回的平面位于目标沿法线投影的最大端之外，负深度则越过最小端。
/// 不使用 AABB 的 min/max 角点直接点乘，因为任意方向下那两个角点并不一定是
/// 实际的投影极值。
fn pocket_target_span(
    plane: &PlaneDef,
    target: &MeshData,
) -> std::result::Result<(f64, f64), String> {
    if target.positions.is_empty() {
        return Err("target body has no mesh vertices".to_string());
    }
    let normal = plane.normal();
    if !normal.is_finite() || normal.length_squared() < 1e-12 {
        return Err("sketch plane has an invalid normal".to_string());
    }
    let origin = plane.origin();
    let mut min_along = f64::INFINITY;
    let mut max_along = f64::NEG_INFINITY;
    for position in &target.positions {
        let point = glam::DVec3::new(position[0] as f64, position[1] as f64, position[2] as f64);
        let along = (point - origin).dot(normal);
        if !along.is_finite() {
            return Err("target body contains a non-finite vertex".to_string());
        }
        min_along = min_along.min(along);
        max_along = max_along.max(along);
    }
    let span = max_along - min_along;
    if !span.is_finite() || span <= 1e-6 {
        return Err("target body has no thickness along the pocket direction".to_string());
    }

    Ok((min_along, max_along))
}

fn through_pocket_cutter_spec(
    plane: &PlaneDef,
    target: &MeshData,
) -> std::result::Result<(PlaneDef, f64), String> {
    let (min_along, max_along) = pocket_target_span(plane, target)?;
    // Keep the validated robustness extension for actual Boolean operations.
    let pad = ((max_along - min_along) * 0.02).clamp(2.0, 10.0);
    let start = max_along + pad;
    let end = min_along - pad;
    let mut cutter_plane = plane.clone();
    cutter_plane.origin = (plane.origin() + plane.normal() * start).to_array();
    Ok((cutter_plane, end - start))
}

fn bbox_alignment_value(min: &[f32; 3], max: &[f32; 3], axis: usize, mode: AlignMode) -> f32 {
    match mode {
        AlignMode::Min => min[axis],
        AlignMode::Center => (min[axis] + max[axis]) * 0.5,
        AlignMode::Max => max[axis],
    }
}

/// 在 Align 选单变化后选择参考体。
/// 保留仍有效的当前 Lock；否则优先使用刚加入的 solid，再回退到选单中第一个 solid。
fn align_reference_after_selection(
    current: Option<BodyId>,
    selected: &[BodyId],
    eligible: &[BodyId],
    preferred: Option<BodyId>,
) -> Option<BodyId> {
    let valid = |id: BodyId| selected.contains(&id) && eligible.contains(&id);
    current
        .filter(|id| valid(*id))
        .or_else(|| preferred.filter(|id| valid(*id)))
        .or_else(|| selected.iter().copied().find(|id| eligible.contains(id)))
}

fn rotation_about_axis(point: glam::Vec3, direction: glam::Vec3, angle_rad: f32) -> glam::Mat4 {
    let direction = direction.normalize_or_zero();
    if direction.length_squared() < 1e-8 || !angle_rad.is_finite() {
        return glam::Mat4::IDENTITY;
    }
    glam::Mat4::from_translation(point)
        * glam::Mat4::from_axis_angle(direction, angle_rad)
        * glam::Mat4::from_translation(-point)
}

fn straight_axis_from_edge(edge: &EdgeRef) -> Option<RotationAxis> {
    let longest = edge.segments.iter().max_by(|left, right| {
        let left_len =
            (glam::Vec3::from_array(left.b) - glam::Vec3::from_array(left.a)).length_squared();
        let right_len =
            (glam::Vec3::from_array(right.b) - glam::Vec3::from_array(right.a)).length_squared();
        left_len
            .partial_cmp(&right_len)
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let point = glam::Vec3::from_array(longest.a);
    let direction = glam::Vec3::from_array(longest.b) - point;
    let length = direction.length();
    if length < 1e-5 {
        return None;
    }
    let direction = direction / length;
    let points: Vec<glam::Vec3> = edge
        .segments
        .iter()
        .flat_map(|segment| [segment.a, segment.b])
        .map(glam::Vec3::from_array)
        .collect();
    let scale = points
        .iter()
        .map(|candidate| (*candidate - point).length())
        .fold(length, f32::max);
    let tolerance = (scale * 1e-3).max(1e-4);
    let straight = points
        .iter()
        .all(|candidate| (*candidate - point).cross(direction).length() <= tolerance);
    let (min_t, max_t) = points
        .iter()
        .map(|candidate| (*candidate - point).dot(direction))
        .fold((0.0_f32, length), |(min_t, max_t), t| {
            (min_t.min(t), max_t.max(t))
        });
    straight.then_some(RotationAxis {
        point: point.to_array(),
        direction: direction.to_array(),
        display_start: (point + direction * min_t).to_array(),
        display_end: (point + direction * max_t).to_array(),
    })
}

fn alignment_translation(
    reference_min: &[f32; 3],
    reference_max: &[f32; 3],
    object_min: &[f32; 3],
    object_max: &[f32; 3],
    axis: usize,
    mode: AlignMode,
) -> [f32; 3] {
    let target = bbox_alignment_value(reference_min, reference_max, axis, mode);
    let current = bbox_alignment_value(object_min, object_max, axis, mode);
    let mut translation = [0.0; 3];
    translation[axis] = target - current;
    translation
}

fn scale_to_size_factors(
    current_size: glam::Vec3,
    target_size: glam::Vec3,
) -> std::result::Result<glam::Vec3, &'static str> {
    if !target_size.is_finite() || target_size.min_element() <= 0.0 {
        return Err("Target X, Y, and Z sizes must be finite and greater than zero");
    }
    if !current_size.is_finite() || current_size.min_element() <= 1e-6 {
        return Err("Cannot scale to size because the current selection has a zero-size axis");
    }
    let factors = target_size / current_size;
    if !factors.is_finite() || factors.min_element() < 0.0001 || factors.max_element() > 10_000.0 {
        return Err("Target size is outside the supported scale range");
    }
    Ok(factors)
}

pub(crate) fn synchronized_scale_percent(
    values: [f32; 3],
    changed_axis: usize,
    uniform: bool,
) -> [f32; 3] {
    if uniform {
        [values[changed_axis.min(2)]; 3]
    } else {
        values
    }
}

pub(crate) fn synchronized_scale_size(
    values: [f32; 3],
    current_size: [f32; 3],
    changed_axis: usize,
    uniform: bool,
) -> [f32; 3] {
    if !uniform {
        return values;
    }
    let axis = changed_axis.min(2);
    let base = current_size[axis];
    if !base.is_finite() || base.abs() <= 1.0e-6 || !values[axis].is_finite() {
        return values;
    }
    let factor = values[axis] / base;
    current_size.map(|size| size * factor)
}

/// gizmo 变换矩阵（纯函数，便于测试）。
/// - Translate：沿相机 right/up 平面平移（wpp = 世界每像素）
/// - Rotate：绕当前选择组中心、世界 Y 轴旋转
/// - Scale：绕当前选择组中心统一缩放
fn gizmo_step_matrix_static(
    gizmo: GizmoMode,
    delta: egui::Vec2,
    center: glam::Vec3,
    wpp: f32,
    cam_right: glam::Vec3,
    cam_up: glam::Vec3,
) -> glam::Mat4 {
    match gizmo {
        GizmoMode::Translate => {
            let t = cam_right * (delta.x * wpp) - cam_up * (delta.y * wpp);
            glam::Mat4::from_translation(t)
        }
        GizmoMode::Rotate => {
            let angle = -delta.x * 0.01;
            glam::Mat4::from_translation(center)
                * glam::Mat4::from_rotation_y(angle)
                * glam::Mat4::from_translation(-center)
        }
        GizmoMode::Scale => {
            let f = (1.0 + delta.y * 0.005).clamp(0.05, 20.0);
            glam::Mat4::from_translation(center)
                * glam::Mat4::from_scale(glam::Vec3::splat(f))
                * glam::Mat4::from_translation(-center)
        }
    }
}

/// 返回目标体上当前活动的倒角/圆角结果。
///
/// 优先级：当前选择 > 最新可见 > 最新创建。最后一级仅用于恢复旧工程中
/// 所有结果都被隐藏的异常状态。
fn active_chamfer_body(doc: &Document, selected: &[BodyId], target: BodyId) -> Option<BodyId> {
    let belongs_to_target = |id: BodyId| {
        doc.body(id).is_some_and(|body| {
            matches!(
                &body.feature,
                Feature::Chamfer { target: t, .. } if *t == target
            )
        })
    };

    selected
        .iter()
        .rev()
        .copied()
        .find(|id| belongs_to_target(*id))
        .or_else(|| {
            doc.bodies
                .iter()
                .rev()
                .find_map(|body| (body.visible && belongs_to_target(body.id)).then_some(body.id))
        })
        .or_else(|| {
            doc.bodies
                .iter()
                .rev()
                .find_map(|body| belongs_to_target(body.id).then_some(body.id))
        })
}

fn latest_visible_chamfer_body(doc: &Document, target: BodyId) -> Option<BodyId> {
    doc.bodies.iter().rev().find_map(|body| {
        (body.visible
            && matches!(
                &body.feature,
                Feature::Chamfer { target: candidate, .. } if *candidate == target
            ))
        .then_some(body.id)
    })
}

fn stored_chamfer_segments(edges: &[s33d_model::ChamferEdgeSeg]) -> Vec<EdgeSeg> {
    edges
        .iter()
        .map(|segment| EdgeSeg {
            a: [
                segment.a[0] as f32,
                segment.a[1] as f32,
                segment.a[2] as f32,
            ],
            b: [
                segment.b[0] as f32,
                segment.b[1] as f32,
                segment.b[2] as f32,
            ],
            n1: [
                segment.n1[0] as f32,
                segment.n1[1] as f32,
                segment.n1[2] as f32,
            ],
            n2: [
                segment.n2[0] as f32,
                segment.n2[1] as f32,
                segment.n2[2] as f32,
            ],
        })
        .collect()
}

fn model_chamfer_segments(segments: &[EdgeSeg]) -> Vec<s33d_model::ChamferEdgeSeg> {
    segments
        .iter()
        .map(|segment| s33d_model::ChamferEdgeSeg {
            a: segment.a.map(f64::from),
            b: segment.b.map(f64::from),
            n1: segment.n1.map(f64::from),
            n2: segment.n2.map(f64::from),
        })
        .collect()
}

fn mesh_reference_bounds(mesh: &MeshData) -> Option<[[f64; 3]; 2]> {
    let first = *mesh.positions.first()?;
    let mut min = first.map(f64::from);
    let mut max = min;
    for point in &mesh.positions[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(f64::from(point[axis]));
            max[axis] = max[axis].max(f64::from(point[axis]));
        }
    }
    Some([min, max])
}

fn mesh_sharp_edge_segments(mesh: &MeshData) -> Vec<Vec<EdgeSeg>> {
    let mut mesh = mesh.clone();
    mesh.weld(1e-5);
    let chains = mesh.sharp_edge_chains(25.0, 20.0);
    chains
        .into_iter()
        .filter_map(|chain| {
            let mut segments = Vec::with_capacity(chain.len());
            for (edge_a, edge_b) in chain {
                let key = if edge_a < edge_b {
                    (edge_a, edge_b)
                } else {
                    (edge_b, edge_a)
                };
                let mut normals = Vec::with_capacity(2);
                for triangle in mesh.indices.chunks_exact(3) {
                    if !(triangle.contains(&(key.0 as u32)) && triangle.contains(&(key.1 as u32))) {
                        continue;
                    }
                    let a = glam::Vec3::from_array(mesh.positions[triangle[0] as usize]);
                    let b = glam::Vec3::from_array(mesh.positions[triangle[1] as usize]);
                    let c = glam::Vec3::from_array(mesh.positions[triangle[2] as usize]);
                    let normal = (b - a).cross(c - a);
                    if normal.length_squared() > 1e-12 {
                        normals.push(normal.normalize());
                    }
                    if normals.len() == 2 {
                        break;
                    }
                }
                if normals.len() == 2 {
                    segments.push(EdgeSeg {
                        a: mesh.positions[key.0],
                        b: mesh.positions[key.1],
                        n1: normals[0].to_array(),
                        n2: normals[1].to_array(),
                    });
                }
            }
            (!segments.is_empty()).then_some(segments)
        })
        .collect()
}

#[derive(Clone, Copy)]
struct EdgeChainSignature {
    center: glam::DVec3,
    relative_length: f64,
    normal_axes: glam::DVec3,
    closed: bool,
}

fn edge_chain_signature(segments: &[EdgeSeg], bounds: [[f64; 3]; 2]) -> Option<EdgeChainSignature> {
    if segments.is_empty() {
        return None;
    }
    let min = glam::DVec3::from_array(bounds[0]);
    let max = glam::DVec3::from_array(bounds[1]);
    let extent = max - min;
    let diagonal = extent.length().max(1e-9);
    let safe_extent = extent.max(glam::DVec3::splat(1e-9));
    let mut weighted_center = glam::DVec3::ZERO;
    let mut normal_axes = glam::DVec3::ZERO;
    let mut total_length = 0.0;
    let mut endpoints = Vec::with_capacity(segments.len() * 2);
    for segment in segments {
        let a = glam::DVec3::from_array(segment.a.map(f64::from));
        let b = glam::DVec3::from_array(segment.b.map(f64::from));
        let length = a.distance(b);
        weighted_center += (a + b) * 0.5 * length;
        total_length += length;
        let n1 = glam::DVec3::from_array(segment.n1.map(f64::from)).abs();
        let n2 = glam::DVec3::from_array(segment.n2.map(f64::from)).abs();
        normal_axes += (n1 + n2) * length;
        endpoints.push(a);
        endpoints.push(b);
    }
    if total_length <= 1e-9 {
        return None;
    }
    let center = (weighted_center / total_length - min) / safe_extent;
    let normal_axes = (normal_axes / total_length).normalize_or_zero();
    let tolerance = diagonal * 1e-5 + 1e-7;
    let closed = segments.len() > 2
        && endpoints.iter().all(|endpoint| {
            endpoints
                .iter()
                .filter(|candidate| endpoint.distance(**candidate) <= tolerance)
                .count()
                >= 2
        });
    Some(EdgeChainSignature {
        center,
        relative_length: total_length / diagonal,
        normal_axes,
        closed,
    })
}

fn remap_chamfer_segments(
    mesh: &MeshData,
    stored: &[EdgeSeg],
    reference_bounds: [[f64; 3]; 2],
) -> Option<Vec<EdgeSeg>> {
    let stored_signature = edge_chain_signature(stored, reference_bounds)?;
    let current_bounds = mesh_reference_bounds(mesh)?;
    mesh_sharp_edge_segments(mesh)
        .into_iter()
        .filter_map(|candidate| {
            let signature = edge_chain_signature(&candidate, current_bounds)?;
            if signature.closed != stored_signature.closed {
                return None;
            }
            let center_error = signature.center.distance(stored_signature.center);
            let length_error =
                if signature.relative_length > 1e-9 && stored_signature.relative_length > 1e-9 {
                    (signature.relative_length / stored_signature.relative_length)
                        .ln()
                        .abs()
                } else {
                    1.0
                };
            let normal_error = signature.normal_axes.distance(stored_signature.normal_axes);
            let score = center_error * 8.0 + length_error + normal_error * 0.5;
            (score <= 3.0).then_some((candidate, score))
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(candidate, _)| candidate)
}

fn rebuild_feature_dependencies(feature: &Feature) -> Vec<BodyId> {
    if let Some((_, operands)) = derived_boolean_recipe(feature) {
        return operands;
    }
    match feature {
        Feature::Chamfer { target, base, .. } => vec![base.unwrap_or(*target)],
        Feature::Derived {
            operation: Some(DerivedOperation::Mirror { source, .. }),
            ..
        }
        | Feature::Derived {
            operation: Some(DerivedOperation::Copy { source, .. }),
            ..
        }
        | Feature::Derived {
            operation: Some(DerivedOperation::Split { source, .. }),
            ..
        } => vec![*source],
        _ => Vec::new(),
    }
}

fn derived_boolean_recipe(feature: &Feature) -> Option<(DerivedBooleanOp, Vec<BodyId>)> {
    let Feature::Derived { source, operation } = feature else {
        return None;
    };
    if let Some(DerivedOperation::Boolean { op, operands }) = operation {
        return Some((*op, operands.clone()));
    }

    // Projects saved before structured dependencies used deterministic English
    // descriptions. Recover those links on first rebuild and then write the
    // structured recipe back into the feature.
    let (op, tail) = [
        (DerivedBooleanOp::Union, "Union of "),
        (DerivedBooleanOp::Subtract, "Subtract of "),
        (DerivedBooleanOp::Intersect, "Intersect of "),
    ]
    .into_iter()
    .find_map(|(op, prefix)| source.strip_prefix(prefix).map(|tail| (op, tail)))?;
    let operands: Vec<_> = tail
        .replace(" and ", ",")
        .split(',')
        .map(str::trim)
        .map(str::parse::<BodyId>)
        .collect::<Result<_, _>>()
        .ok()?;
    (operands.len() >= 2).then_some((op, operands))
}

/// 从 `parent` 开始按父子顺序列出后继；直接子节点改接到 `first_base`。
fn chamfer_descendant_plan(
    doc: &Document,
    target: BodyId,
    parent: BodyId,
    first_base: BodyId,
) -> Result<Vec<(BodyId, BodyId)>, String> {
    let children = |base_id: BodyId| -> Vec<BodyId> {
        doc.bodies
            .iter()
            .filter_map(|candidate| match &candidate.feature {
                Feature::Chamfer {
                    target: child_target,
                    base: Some(base),
                    ..
                } if *child_target == target && *base == base_id => Some(candidate.id),
                _ => None,
            })
            .collect()
    };
    let mut queue = std::collections::VecDeque::new();
    for child in children(parent) {
        queue.push_back((child, first_base));
    }
    let mut visited = std::collections::HashSet::new();
    let mut order = Vec::new();
    while let Some((child, new_base)) = queue.pop_front() {
        if !visited.insert(child) {
            return Err("Chamfer dependency cycle detected".into());
        }
        order.push((child, new_base));
        for grandchild in children(child) {
            queue.push_back((grandchild, child));
        }
    }
    Ok(order)
}

/// 计算删除一个 chamfer/fillet 节点后的依赖重接顺序。
/// 返回 `(原目标, 替代基体, [(待重建节点, 新基体)])`，顺序保证父节点先于子节点。
fn chamfer_delete_plan(
    doc: &Document,
    removed: BodyId,
) -> Result<(BodyId, BodyId, Vec<(BodyId, BodyId)>), String> {
    let body = doc
        .body(removed)
        .ok_or_else(|| "Chamfer operation no longer exists".to_string())?;
    let (target, replacement_base) = match &body.feature {
        Feature::Chamfer { target, base, .. } => (*target, base.unwrap_or(*target)),
        _ => return Err("Selected object is not a chamfer operation".into()),
    };
    let order = chamfer_descendant_plan(doc, target, removed, replacement_base)?;
    Ok((target, replacement_base, order))
}

#[cfg(test)]
mod gizmo_tests {
    fn wait_face_commit(app: &mut super::S33DApp) {
        // Software CPU emulation is much slower than the native geometry worker.
        // Keep the normal deadline; acceptance runs must record an explicit override.
        let wait_seconds = std::env::var("S33D_TEST_FACE_COMMIT_TIMEOUT_SECONDS")
            .map(|value| value.parse::<u64>().expect("Invalid test commit timeout"))
            .unwrap_or(90);
        assert!(
            (90..=300).contains(&wait_seconds),
            "Test commit timeout must be 90..=300 seconds"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_seconds);
        while app.face_extrude_job.is_some() {
            app.poll_exact_face_commit();
            assert!(
                std::time::Instant::now() < deadline,
                "Exact face commit timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    use super::*;

    #[test]
    fn performance_pick_and_measurement_caches_follow_geometry_lifetime() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let id = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"box", "width":4.0,"height":2.0,"depth":6.0}),
        )["id"]
            .as_u64()
            .unwrap();
        let shape = app.body(id).unwrap().shape.unwrap();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        app.pick_edge(rect.center(), rect);
        let cached = Arc::clone(app.pick_geometry_cache.get(&shape).unwrap());
        app.pick_edge(rect.center() + egui::vec2(1.0, 0.0), rect);
        assert!(Arc::ptr_eq(&cached, &app.pick_geometry_cache[&shape]));
        let (_, _, area, volume) = app.shape_measurements(shape).unwrap();
        assert!((area - 88.0).abs() < 1e-4 && (volume - 48.0).abs() < 1e-4);
        assert_eq!(app.shape_measurements(shape).unwrap().3, volume);
        app.update_hover(rect.center(), rect);
        let key = app.last_hover_query;
        app.update_hover(rect.center(), rect);
        assert_eq!(app.last_hover_query, key);
        app.bump_scene();
        app.update_hover(rect.center(), rect);
        assert_ne!(app.last_hover_query, key);
        app.clear_hover_preview();
        assert!(app.last_hover_query.is_none());
        app.new_project();
        assert!(app.pick_geometry_cache.is_empty() && app.measurement_cache.is_empty());
        assert!(app.last_hover_query.is_none());
    }

    #[test]
    fn performance_face_preview_preserves_hole_signed_depth_and_offset() {
        // A square ring; deliberately no B-Rep text, so this cannot run OCCT.
        let input = s33d_kernel::occt::ExactShape {
            brep: String::new(),
            mesh: MeshData::new(
                vec![
                    [-2., -2., 0.],
                    [2., -2., 0.],
                    [2., 2., 0.],
                    [-2., 2., 0.],
                    [-1., -1., 0.],
                    [1., -1., 0.],
                    [1., 1., 0.],
                    [-1., 1., 0.],
                ],
                vec![
                    0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7,
                ],
            ),
            triangle_faces: vec![0; 8],
            planar_faces: vec![true],
            face_count: 1,
            solid_count: 0,
        };
        for depth in [3.0, -3.0] {
            let mesh = exact_tools::face_preview_mesh(&input, 0, [0., 0., 1.], depth, 1.0).unwrap();
            let (min, max) = mesh.bounds().unwrap();
            assert_eq!(min.z, (1.0 + depth).min(1.0) as f32);
            assert_eq!(max.z, (1.0 + depth).max(1.0) as f32);
            assert!((crate::ui::mesh_surface_area_and_volume(&mesh).1 - 36.0).abs() < 1e-5);
            let mut edges = HashMap::new();
            for t in mesh.indices.chunks_exact(3) {
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    let entry = edges
                        .entry(if a < b { (a, b) } else { (b, a) })
                        .or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += if a < b { 1 } else { -1 };
                }
            }
            assert!(
                edges.values().all(|v| *v == (2, 0)),
                "Closed, consistently oriented preview"
            );
        }
        assert!(exact_tools::face_preview_mesh(&input, 1, [0., 0., 1.], 3., 0.).is_err());
        assert!(exact_tools::face_preview_mesh(&input, 0, [0., 0., 1.], f64::NAN, 0.).is_err());
    }

    fn agent_call(app: &mut S33DApp, method: &str, params: Value) -> Value {
        app.execute_agent_request(AgentRequest::new(json!(1), method, params))
            .unwrap_or_else(|(code, message)| panic!("{method} failed [{code}]: {message}"))
    }

    #[test]
    fn agent_cancelled_queue_cannot_mutate_and_replay_cannot_create_twice() {
        use crate::agent_protocol::send_request;
        use std::time::{Duration, Instant};
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.agent_server = AgentServer::start("127.0.0.1:0").unwrap();
        let address = app.agent_server.address().unwrap().to_string();
        let mut request = AgentRequest::new(
            json!(1),
            "primitive.create",
            json!({"kind":"box","width":4,"height":2,"depth":6}),
        );
        request.idempotency_key = Some("cancel-box".into());
        let peer_address = address.clone();
        let cancelled = request.clone();
        let client = std::thread::spawn(move || send_request(&peer_address, &cancelled).unwrap());
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            // Cancellation bypasses the UI, even when it cannot drain its queue.
            let response = send_request(
                &address,
                &AgentRequest::new(
                    json!(2),
                    "request.cancel",
                    json!({"idempotency_key":"cancel-box"}),
                ),
            )
            .unwrap();
            if response.error.as_ref().is_some_and(|e| e.code == "expired") {
                break;
            }
            assert!(Instant::now() < until, "write did not enter the queue");
            std::thread::sleep(Duration::from_millis(5));
        }
        app.process_agent_requests();
        assert!(
            app.doc.bodies.is_empty(),
            "cancelled command modified the model"
        );
        assert_eq!(client.join().unwrap().error.unwrap().code, "expired");

        request.idempotency_key = Some("one-box".into());
        let peer_address = address.clone();
        let original = request.clone();
        let client = std::thread::spawn(move || send_request(&peer_address, &original).unwrap());
        let until = Instant::now() + Duration::from_secs(5);
        while !client.is_finished() {
            app.process_agent_requests();
            assert!(Instant::now() < until, "write did not complete");
            std::thread::sleep(Duration::from_millis(5));
        }
        let first = client.join().unwrap();
        assert!(first.ok, "{first:?}");
        request.id = json!(3);
        let replay = send_request(&address, &request).unwrap();
        assert!(replay.ok);
        assert_eq!(replay.id, json!(3));
        assert_eq!(first.result, replay.result);
        app.process_agent_requests();
        assert_eq!(app.doc.bodies.len(), 1, "retry created a second object");
    }

    fn closed_line_entities(points: &[[f64; 2]]) -> Vec<SketchEntity> {
        (0..points.len())
            .map(|index| SketchEntity::Line {
                a: points[index],
                b: points[(index + 1) % points.len()],
            })
            .collect()
    }

    #[test]
    fn recenter_ignores_hidden_and_unreferenced_geometry() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 900.0));
        app.recenter_view();
        assert!(app.camera.eye.is_finite());
        assert_eq!(app.camera.target, glam::Vec3::ZERO);
        agent_call(
            &mut app,
            "primitive.create",
            json!({
                "kind":"box", "position":[100.0,20.0,-40.0],
                "width":4.0, "height":2.0, "depth":6.0
            }),
        );
        let center = app.selected_center().unwrap();
        let hidden = agent_call(
            &mut app,
            "primitive.create",
            json!({
                "kind":"box", "position":[10000.0,10000.0,10000.0],
                "width":100.0, "height":100.0, "depth":100.0
            }),
        )["id"]
            .as_u64()
            .unwrap();
        app.doc
            .bodies
            .iter_mut()
            .find(|b| b.id == hidden)
            .unwrap()
            .visible = false;
        app.camera.ortho = true;
        app.recenter_view();
        assert_eq!(app.camera.target, center);
        assert!(!app.camera.ortho);
        assert!(app.camera.distance() < 100.0);
        app.doc.bodies.retain(|b| b.id != hidden);
        app.recenter_view();
        assert_eq!(app.camera.target, center);
    }

    #[test]
    fn agent_control_creates_transforms_and_undoes_a_primitive() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let created = agent_call(
            &mut app,
            "primitive.create",
            json!({
                "kind":"box",
                "name":"Agent Box",
                "position":[0.0, 0.0, 0.0],
                "width":40.0,
                "depth":20.0,
                "height":8.0
            }),
        );
        let body_id = created["id"].as_u64().unwrap();
        assert_eq!(created["name"], "Agent Box");
        assert_eq!(created["bounds"]["size"], json!([40.0, 8.0, 20.0]));
        app.refresh_scene();

        let transformed = agent_call(
            &mut app,
            "body.transform",
            json!({"body_ids":[body_id], "translation":[12.0,3.0,-4.0]}),
        );
        assert_eq!(transformed["transformed_body_ids"], json!([body_id]));
        let moved_min = transformed["bodies"][0]["bounds"]["min"]
            .as_array()
            .unwrap();
        assert!((moved_min[0].as_f64().unwrap() + 8.0).abs() < 1.0e-4);
        assert!((moved_min[1].as_f64().unwrap() - 3.0).abs() < 1.0e-4);

        agent_call(&mut app, "history.undo", json!({}));
        let restored = agent_call(&mut app, "body.get", json!({"body_id":body_id}));
        assert_eq!(restored["bounds"]["min"], json!([-20.0, 0.0, -10.0]));
    }

    #[test]
    fn agent_trim_preview_apply_history_and_freshness() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let id = agent_call(
            &mut app,
            "sketch.create",
            json!({"entities":[
                {"type":"rectangle","corner1":[0,0],"corner2":[10,10]},
                {"type":"line","start":[20,0],"end":[30,0]}
            ]}),
        )["id"]
            .as_u64()
            .unwrap();
        let solid = agent_call(
            &mut app,
            "feature.extrude",
            json!({"sketch_id":id,"depth":5}),
        );
        let before = app.agent_document_value();
        let params = json!({"sketch_id":id,"point":[25,0],"tolerance":0.1});
        let preview = agent_call(&mut app, "sketch.trim.preview", params.clone());
        assert_eq!(preview["entity_index"], 1);
        assert_eq!(preview["entity_count_after"], 1);
        assert_eq!(preview["regions_after"][0]["area"], 100.0);
        assert_eq!(app.agent_document_value(), before, "preview is read-only");
        let mut apply = params;
        apply["preview_token"] = preview["preview_token"].clone();
        let mut changed_point = apply.clone();
        changed_point["point"] = json!([5, 0]);
        assert_eq!(
            app.execute_agent_request(AgentRequest::new(
                json!(1),
                "sketch.trim.apply",
                changed_point
            ))
            .unwrap_err()
            .0,
            "stale_preview"
        );
        let result = agent_call(&mut app, "sketch.trim.apply", apply.clone());
        assert_eq!(result["body"]["id"], id);
        assert_eq!(
            result["body"]["sketch"]["entities"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(result["rebuilt_body_ids"], json!([solid["id"]]));
        let rebuilt = agent_call(&mut app, "body.get", json!({"body_id":solid["id"]}));
        assert_eq!(rebuilt["bounds"], solid["bounds"]);
        assert_eq!(
            app.execute_agent_request(AgentRequest::new(json!(1), "sketch.trim.apply", apply))
                .unwrap_err()
                .0,
            "stale_preview"
        );
        agent_call(&mut app, "history.undo", json!({}));
        assert_eq!(
            app.doc.body(id).unwrap().sketch().unwrap().entities.len(),
            2
        );
        agent_call(&mut app, "history.redo", json!({}));
        assert_eq!(
            app.doc.body(id).unwrap().sketch().unwrap().entities.len(),
            1
        );
    }

    #[test]
    fn agent_trim_failure_preserves_document_and_selection() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let id = agent_call(
            &mut app,
            "sketch.create",
            json!({"entities":[
                {"type":"rectangle","corner1":[0,0],"corner2":[10,10]}
            ]}),
        )["id"]
            .as_u64()
            .unwrap();
        let solid = agent_call(&mut app, "feature.extrude", json!({"sketch_id":id}));
        let before = app.agent_document_value();
        let mut params = json!({"sketch_id":id,"point":[5,0],"tolerance":0.1});
        let preview = agent_call(&mut app, "sketch.trim.preview", params.clone());
        assert_eq!(preview["regions_after"], json!([]));
        params["preview_token"] = preview["preview_token"].clone();
        let fail = app
            .execute_agent_request(AgentRequest::new(
                json!(1),
                "sketch.trim.apply",
                params.clone(),
            ))
            .unwrap_err();
        assert_eq!(fail.0, "rebuild_failed");
        assert_eq!(app.agent_document_value(), before);
        for (input, code) in [
            (
                json!({"sketch_id":id,"point":[50,50],"tolerance":0.1}),
                "no_hit",
            ),
            (
                json!({"sketch_id":id,"point":[5,0],"tolerance":0}),
                "invalid_params",
            ),
            (
                json!({"sketch_id":solid["id"],"point":[5,0],"tolerance":0.1}),
                "invalid_params",
            ),
            (
                json!({"sketch_id":99999,"point":[5,0],"tolerance":0.1}),
                "not_found",
            ),
        ] {
            assert_eq!(
                app.execute_agent_request(AgentRequest::new(
                    json!(1),
                    "sketch.trim.preview",
                    input
                ))
                .unwrap_err()
                .0,
                code
            );
        }
        app.mode = Mode::Sketch;
        assert_eq!(
            app.execute_agent_request(AgentRequest::new(json!(1), "sketch.trim.apply", params))
                .unwrap_err()
                .0,
            "busy"
        );
        assert_eq!(
            app.execute_agent_request(AgentRequest::new(json!(1), "view.recenter", json!({})))
                .unwrap_err()
                .0,
            "busy"
        );
        app.mode = Mode::Idle;
        assert_eq!(app.agent_document_value(), before);
    }

    #[test]
    #[ignore = "Local imported hardware fixture; set S33D_THREAD_FIXTURE_DIR"]
    fn thread_imported_hardware_full_wall_pick() {
        let folder = std::env::var("S33D_THREAD_FIXTURE_DIR").expect("fixture directory");
        for filename in ["thread-screw.brep", "thread-standoff.brep"] {
            let brep =
                std::fs::read_to_string(std::path::Path::new(&folder).join(filename)).unwrap();
            let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
            let mut app = S33DApp::new(&creation);
            let shape = app.kernel.restore_exact(&brep).unwrap();
            let id = app.doc.alloc_id();
            app.doc.add_body(Body {
                id,
                name: filename.into(),
                kind: BodyKind::Solid,
                feature: Feature::Imported {
                    path: filename.into(),
                },
                shape: Some(shape),
                material: Material::default(),
                visible: true,
            });
            let targets = app.thread_targets(&[id]).unwrap();
            eprintln!("{filename}: {targets:?}");
            assert!(!targets.is_empty(), "No cylinders in {filename}");
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
            for t in &targets {
                let axis = glam::DVec3::from_array(t.axis);
                let center = glam::DVec3::from_array(t.origin) + axis * t.length * 0.5;
                let radial = axis.any_orthonormal_vector();
                app.camera.eye =
                    (center + radial * if t.internal { 0. } else { t.radius * 4. }).as_vec3();
                app.camera.target =
                    (center + radial * if t.internal { t.radius * 2. } else { 0. }).as_vec3();
                app.camera.up = axis.as_vec3();
                let face = app.pick_face(rect.center(), rect).unwrap().0;
                let picked = app
                    .thread_target_for_face(&face)
                    .expect("full cylinder picked");
                assert_eq!(picked.face_id, t.face_id);
                assert!(
                    (face.area_mm2 / (2. * std::f64::consts::PI * t.radius * t.length) - 1.).abs()
                        < 0.01,
                    "{filename}, target {t:?}: picked area {}, expected {}",
                    face.area_mm2,
                    2. * std::f64::consts::PI * t.radius * t.length
                );
                assert!(app.begin_thread_on_face(&face));
                assert_eq!(app.thread_choices.len(), 1);
                assert_eq!(app.thread_checked, vec![true]);
            }
            // Exercise the same Apply path as the desktop, on the copy only.
            let t = targets
                .iter()
                .filter(|t| !t.internal && t.length > 2.)
                .min_by(|a, b| a.radius.total_cmp(&b.radius))
                .unwrap()
                .clone();
            let mesh = app.mesh_for(shape);
            let face = FaceHit {
                body_id: id,
                normal: t.axis.map(|v| v as f32),
                center: t.origin.map(|v| v as f32),
                triangles: mesh
                    .indices
                    .chunks_exact(3)
                    .filter(|tri| t.contains_triangle(&mesh, tri))
                    .flat_map(|tri| tri.iter().copied())
                    .collect(),
                area_mm2: 0.,
            };
            assert!(app.begin_thread_on_face(&face));
            let mut params = app.thread_params.clone();
            params.length = params.length.min(t.length - 0.1);
            app.apply_threads(vec![t], params)
                .expect("thread Apply on imported hardware");
            assert_ne!(app.doc.body(id).unwrap().shape, Some(shape));
            assert!(app.undo());
            assert_eq!(app.doc.body(id).unwrap().shape, Some(shape));
        }
    }

    #[test]
    fn thread_double_click_picker_selects_whole_mesh_cylinder_not_caps() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let id = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"cylinder","radius":3.,"height":10.}),
        )["id"]
            .as_u64()
            .unwrap();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        app.camera.eye = glam::Vec3::new(20., 5., 0.);
        app.camera.target = glam::Vec3::new(0., 5., 0.);
        app.camera.up = glam::Vec3::Y;
        let face = app.pick_face(rect.center(), rect).unwrap().0;
        assert!((face.area_mm2 / (2. * std::f64::consts::PI * 3. * 10.) - 1.).abs() < 0.01);
        assert!(
            face.triangles.len() > 96 * 3,
            "Only a strip of the side was selected"
        );
        let before = app.doc.body(id).unwrap().shape;
        assert!(app.begin_thread_on_face(&face));
        assert_eq!(app.thread_choices.len(), 1);
        assert_eq!(app.thread_checked, vec![true]);
        assert!(app.open_thread_panel);
        assert_eq!(
            app.doc.body(id).unwrap().shape,
            before,
            "Opening thread draft must not cut"
        );
        app.camera.eye = glam::Vec3::new(0., 20., 0.);
        app.camera.target = glam::Vec3::new(0., 5., 0.);
        app.camera.up = glam::Vec3::Z;
        let cap = app.pick_face(rect.center(), rect).unwrap().0;
        assert!(app.thread_target_for_face(&cap).is_none());
        assert!((cap.area_mm2 / (std::f64::consts::PI * 9.) - 1.).abs() < 0.01);
    }

    #[test]
    fn thread_sidebar_is_selection_bound_and_edits_switch_to_custom() {
        type Text = (String, egui::Rect);
        fn collect(shape: &egui::Shape, text: &mut Vec<Text>) {
            match shape {
                egui::Shape::Text(t) => text.push((
                    t.galley.job.text.clone(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()),
                )),
                egui::Shape::Vec(v) => {
                    for s in v {
                        collect(s, text);
                    }
                }
                _ => {}
            }
        }
        fn render(ctx: &egui::Context, app: &mut S33DApp, events: Vec<egui::Event>) -> Vec<Text> {
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(520., 1900.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| crate::threads::panel(app, ui));
                },
            );
            let mut text = Vec::new();
            for item in out.shapes {
                collect(&item.shape, &mut text);
            }
            text
        }
        fn click(ctx: &egui::Context, app: &mut S33DApp, text: &[Text], label: &str) -> Vec<Text> {
            let pos = text
                .iter()
                .find(|t| t.0 == label)
                .unwrap_or_else(|| panic!("Missing {label}: {text:?}"))
                .1
                .center();
            render(
                ctx,
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            render(
                ctx,
                app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            render(ctx, app, vec![])
        }
        let ctx = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = S33DApp::new(&creation);
        ctx.style_mut(|s| s.animation_time = 0.);
        app.settings.language = UiLanguage::English;
        let id = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"cylinder","radius":3.,"height":10.}),
        )["id"]
            .as_u64()
            .unwrap();
        app.selected = vec![id];
        assert!(!render(&ctx, &mut app, vec![])
            .iter()
            .any(|t| t.0 == "Thread generator"));
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        app.camera.eye = glam::Vec3::new(20., 5., 0.);
        app.camera.target = glam::Vec3::new(0., 5., 0.);
        app.camera.up = glam::Vec3::Y;
        let face = app.pick_face(rect.center(), rect).unwrap().0;
        app.face_pick = Some(face);
        app.align_active = true;
        let closed = render(&ctx, &mut app, vec![]);
        assert!(
            app.align_active,
            "Displaying a thread entry must not cancel another tool"
        );
        app.align_active = false;
        assert!(closed.iter().any(|t| t.0 == "Thread generator"));
        assert!(!closed.iter().any(|t| t.0 == "Nominal diameter"));
        let expanded = click(&ctx, &mut app, &closed, "Thread generator");
        let index = |s: &str| expanded.iter().position(|t| t.0 == s).unwrap();
        assert!(index("Selected cylinder geometry") < index("Preset"));
        assert!(index("Preset") < index("Nominal diameter"));
        assert!(index("Nominal diameter") < index("suggest for 3D printing"));
        let menu = click(&ctx, &mut app, &expanded, "Custom");
        assert!(menu.iter().any(|t| t.0 == "M6"));
        assert!(!menu
            .iter()
            .any(|t| t.0 == "M8" || t.0.contains("Print coarse")));
        let chosen = click(&ctx, &mut app, &menu, "M6");
        assert_eq!(app.thread_ui.selected_preset.as_deref(), Some("M6"));
        assert!(chosen.iter().any(|t| t.0 == "M6"));
        app.thread_params.offset = 0.1;
        let edited = render(&ctx, &mut app, vec![]);
        assert!(app.thread_ui.selected_preset.is_none());
        assert!(edited.iter().any(|t| t.0 == "Custom"));
        let suggested = click(&ctx, &mut app, &edited, "suggest for 3D printing");
        assert!(!app.thread_ui.print_open);
        assert_eq!(
            (
                app.thread_ui.printing.nozzle,
                app.thread_ui.printing.fdm_layer
            ),
            (0.4, 0.2)
        );
        assert!(app.thread_params.radial_fit > 0.);
        assert_eq!(app.thread_params.angle_deg, 90.);
        let before_settings = app.thread_params.clone();
        let settings = click(&ctx, &mut app, &suggested, "...");
        assert!(app.thread_ui.print_open);
        assert_eq!(app.thread_params, before_settings);
        let process_menu = click(&ctx, &mut app, &settings, "FDM");
        let resin = click(&ctx, &mut app, &process_menu, "Resin");
        assert_eq!(
            app.thread_ui.printing.process,
            crate::threads::config::PrintProcess::Resin
        );
        assert!(resin.iter().any(|t| t.0 == "Layer height"));
        assert!(!resin.iter().any(|t| t.0 == "Nozzle"));
        assert_eq!(
            app.thread_params, before_settings,
            "Settings alone must not change the thread"
        );
        let closed = click(&ctx, &mut app, &resin, "Close");
        assert!(!app.thread_ui.print_open);
        assert_eq!(app.thread_params, before_settings);
        click(&ctx, &mut app, &closed, "suggest for 3D printing");
        assert_eq!(app.thread_params.angle_deg, 60.);
        assert_ne!(app.thread_params, before_settings);
        app.thread_params.depth = 10.;
        assert!(render(&ctx, &mut app, vec![])
            .iter()
            .any(|t| t.0.starts_with("warning:") && t.0.contains("Tooth depth")));
        app.new_project();
        assert!(app.thread_ui.target_key.is_none());
        assert!(!app.thread_ui.expanded);
        assert!(app.thread_ui.selected_preset.is_none());
    }

    #[test]
    fn thread_sidebar_hides_for_planar_caps() {
        let ctx = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = S33DApp::new(&creation);
        let id = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"cylinder","radius":3.,"height":10.}),
        )["id"]
            .as_u64()
            .unwrap();
        app.selected = vec![id];
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        app.camera.eye = glam::Vec3::new(0., 20., 0.);
        app.camera.target = glam::Vec3::new(0., 5., 0.);
        app.camera.up = glam::Vec3::Z;
        app.face_pick = Some(app.pick_face(rect.center(), rect).unwrap().0);
        let out = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| crate::threads::panel(&mut app, ui));
        });
        fn has_entry(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Text(t) => t.galley.job.text == "Thread generator",
                egui::Shape::Vec(v) => v.iter().any(has_entry),
                _ => false,
            }
        }
        assert!(!out.shapes.iter().any(|s| has_entry(&s.shape)));
    }

    #[test]
    fn thread_array_cylinder_copy_selects_full_wall_and_survives_reload() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let original = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"cylinder","radius":3.,"height":30.,"position":[-1.,0.,0.]}),
        )["id"]
            .as_u64()
            .unwrap();
        app.selected = vec![original];
        app.array_axes = [true, false, true];
        app.array_copies = [3, 1, 3];
        app.array_spacing = [10., 10., 10.];
        app.array_positive = [true; 3];
        app.array_selected();
        let ids: Vec<_> = app.doc.bodies.iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), 9);
        assert_eq!(
            app.thread_targets(&ids).unwrap().len(),
            9,
            "Array copies must retain the cylinder wall, not just the original"
        );

        // Old projects persist the array copies as Mesh payloads. Exercise
        // that representation too, rather than testing only new native copies.
        for &id in &ids[1..] {
            let mesh = app
                .kernel
                .tessellate(app.doc.body(id).unwrap().shape.unwrap())
                .unwrap();
            let shape = app.kernel.insert_mesh(mesh);
            let body = app.doc.body_mut(id).unwrap();
            body.shape = Some(shape);
            if let Feature::Derived {
                operation:
                    Some(DerivedOperation::Copy {
                        primitive_snapshot, ..
                    }),
                ..
            } = &mut body.feature
            {
                *primitive_snapshot = None;
            }
        }
        let encoded = app.project_to_s33d().unwrap();
        app.load_s33d_project(&encoded).unwrap();
        let last = *ids.last().unwrap();
        assert_eq!(app.thread_targets(&[last]).unwrap().len(), 1);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        app.camera.eye = glam::Vec3::new(39., 15., 20.);
        app.camera.target = glam::Vec3::new(19., 15., 20.);
        app.camera.up = glam::Vec3::Y;
        let face = app.pick_face(rect.center(), rect).unwrap().0;
        assert_eq!(face.body_id, last);
        assert!(
            (face.area_mm2 / (2. * std::f64::consts::PI * 3. * 30.) - 1.).abs() < 0.01,
            "Expected the full ~565 mm² wall, got {}",
            face.area_mm2
        );
        assert!(app.begin_thread_on_face(&face));
        assert_eq!(app.thread_choices.len(), 1);
        let target = app.thread_choices[0].clone();
        assert!((target.origin[0] - 19.).abs() < 1e-4);
        assert!((target.origin[2] - 20.).abs() < 1e-4);
        let before = app.doc.body(last).unwrap().shape;
        app.apply_threads(
            vec![target],
            crate::threads::Params {
                length: 3.,
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(app.doc.body(last).unwrap().shape, before);
        assert!(app.undo());
        assert_eq!(app.doc.body(last).unwrap().shape, before);
    }

    #[test]
    #[ignore = "local user array project; set S33D_THREAD_ARRAY_FIXTURE"]
    fn thread_user_array_project_selects_entire_cylinder() {
        let path = std::env::var("S33D_THREAD_ARRAY_FIXTURE").unwrap();
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.load_s33d_project(&std::fs::read_to_string(path).unwrap())
            .unwrap();
        let ids: Vec<_> = app.doc.bodies.iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), 9);
        let targets = app.thread_targets(&ids).unwrap();
        assert_eq!(targets.len(), 9);
        app.camera.eye = glam::Vec3::new(39., 15., 20.);
        app.camera.target = glam::Vec3::new(19., 15., 20.);
        app.camera.up = glam::Vec3::Y;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        let face = app.pick_face(rect.center(), rect).unwrap().0;
        assert_eq!(face.body_id, 9);
        assert!(
            (face.area_mm2 - 565.39).abs() < 0.1,
            "area={}",
            face.area_mm2
        );
        assert_eq!(face.triangles.len() / 3, 192);
        assert!(app.begin_thread_on_face(&face));
        assert_eq!(app.thread_choices.len(), 1);
        assert_eq!(app.thread_choices[0].body_id, 9);
    }

    #[test]
    fn thread_copy_snapshot_survives_source_delete_and_transformed_mesh_reload() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let id = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"cylinder","radius":3.,"height":30.}),
        )["id"]
            .as_u64()
            .unwrap();
        app.selected = vec![id];
        app.copy_selected();
        app.delete_selected();
        app.collect_kernel_garbage();
        app.paste_clipboard();
        let copy = app.selected[0];
        assert_eq!(
            app.thread_targets(&[copy]).unwrap().len(),
            1,
            "A paste must retain its own cylinder provenance after the original is deleted"
        );
        // Project reload freezes truck display ordering as Mesh payloads.
        let project = app.project_to_s33d().unwrap();
        app.load_s33d_project(&project).unwrap();
        app.selected = vec![copy];
        let transform = glam::Mat4::from_rotation_z(0.37)
            * glam::Mat4::from_translation(glam::Vec3::new(4., 3., 2.));
        app.apply_transform(&transform, "test rotate copied cylinder");
        let target = app.thread_targets(&[copy]).unwrap().pop().unwrap();
        let expected = transform.transform_point3(glam::Vec3::new(20., 0., 0.));
        assert!(glam::Vec3::from_array(target.origin.map(|v| v as f32)).distance(expected) < 1e-3);
        assert!((target.radius - 3.).abs() < 1e-4);
        assert!((target.length - 30.).abs() < 1e-4);
        app.apply_transform(
            &glam::Mat4::from_scale(glam::Vec3::new(2., 1., 1.)),
            "test elliptical section",
        );
        assert!(
            app.thread_targets(&[copy]).unwrap().is_empty(),
            "An elliptical cylinder must not be offered for a round thread"
        );
    }

    #[test]
    fn thread_native_tube_array_and_clipboard_preserve_inner_outer_surfaces() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let shape = app
            .kernel
            .revolve_region(
                &Region2D::new(vec![[2.5, 0.], [6., 0.], [6., 10.], [2.5, 10.]]),
                &DatumPlane::Front.def(),
                &Axis2D {
                    point: [0., 0.],
                    dir: [0., 1.],
                },
                360.,
            )
            .unwrap();
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Native tube".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "tube.step".into(),
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        app.selected = vec![id];
        app.array_axes = [true, false, false];
        app.array_copies = [2, 1, 1];
        app.array_spacing = [20.; 3];
        app.array_positive = [true; 3];
        app.array_selected();
        let array_copy = app.selected[0];
        assert!(app
            .kernel
            .exact_shape(app.doc.body(array_copy).unwrap().shape.unwrap())
            .is_some());
        let targets = app.thread_targets(&[array_copy]).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets.iter().filter(|t| t.internal).count(), 1);
        app.copy_selected();
        app.delete_selected();
        app.collect_kernel_garbage();
        app.paste_clipboard();
        let paste = app.selected[0];
        assert!(app
            .kernel
            .exact_shape(app.doc.body(paste).unwrap().shape.unwrap())
            .is_some());
        let targets = app.thread_targets(&[paste]).unwrap();
        assert_eq!(targets.len(), 2);
        assert_ne!(targets[0].face_id, targets[1].face_id);
        // An old mesh-only tube copy keeps distinct synthetic IDs too.
        let mesh = app
            .kernel
            .tessellate(app.doc.body(paste).unwrap().shape.unwrap())
            .unwrap();
        let mesh_shape = app.kernel.insert_mesh(mesh);
        app.doc.body_mut(paste).unwrap().shape = Some(mesh_shape);
        // Its original has been deleted; deliberately point at the surviving
        // native tube with the same actual cumulative placement.
        app.doc.body_mut(paste).unwrap().feature = Feature::Derived {
            source: "legacy tube copy".into(),
            operation: Some(DerivedOperation::Copy {
                source: id,
                offset: [40., 0., 0.],
                primitive_snapshot: None,
            }),
        };
        let targets = app.thread_targets(&[paste]).unwrap();
        assert_eq!(targets.len(), 2);
        assert_ne!(targets[0].face_id, targets[1].face_id);
        assert_eq!(targets.iter().filter(|t| t.internal).count(), 1);
    }

    #[test]
    fn thread_double_click_picker_keeps_inner_outer_and_annular_cap_separate() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let shape = app
            .kernel
            .revolve_region(
                &Region2D::new(vec![[2.5, 0.], [6., 0.], [6., 10.], [2.5, 10.]]),
                &DatumPlane::Front.def(),
                &Axis2D {
                    point: [0., 0.],
                    dir: [0., 1.],
                },
                360.,
            )
            .unwrap();
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Imported tube".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "fixture.step".into(),
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        for (internal, radius, eye, target) in [
            (true, 2.5, [0., 5., 0.], [20., 5., 0.]),
            (false, 6., [20., 5., 0.], [0., 5., 0.]),
        ] {
            app.camera.eye = glam::Vec3::from_array(eye);
            app.camera.target = glam::Vec3::from_array(target);
            app.camera.up = glam::Vec3::Y;
            let face = app.pick_face(rect.center(), rect).unwrap().0;
            assert!((face.area_mm2 / (2. * std::f64::consts::PI * radius * 10.) - 1.).abs() < 0.01);
            assert!(app.begin_thread_on_face(&face));
            assert_eq!(app.thread_choices.len(), 1);
            assert_eq!(app.thread_choices[0].internal, internal);
            assert!((app.thread_choices[0].radius - radius).abs() < 1e-5);
            assert!(!face_pick_none_or_different(&Some(face.clone()), &face));
        }
        assert_eq!(app.cylinder_pick_cache.len(), 1);
        app.camera.eye = glam::Vec3::new(4., 20., 0.);
        app.camera.target = glam::Vec3::new(4., 5., 0.);
        app.camera.up = glam::Vec3::Z;
        let cap = app.pick_face(rect.center(), rect).unwrap().0;
        assert!(app.thread_target_for_face(&cap).is_none());
        assert!((cap.area_mm2 / (std::f64::consts::PI * (36. - 6.25)) - 1.).abs() < 0.01);
        app.reset_document_tools();
        assert!(app.cylinder_pick_cache.is_empty());
    }

    #[test]
    #[ignore = "local user STL; set S33D_MESH_BORE_FIXTURE"]
    fn mesh_bore_double_click_and_thread_apply() {
        let path = std::env::var("S33D_MESH_BORE_FIXTURE").unwrap();
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let shape = app.kernel.import_stl(&path).unwrap()[0];
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Bore test".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported { path },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        let targets = app.thread_targets(&[id]).unwrap();
        assert_eq!(
            targets.len(),
            24,
            "Must include the fine bores as well as the six large stepped holes: {targets:?}"
        );
        for x in [-42.5, 42.5] {
            for z in [-27.5, 27.5] {
                assert!(
                    targets.iter().any(|t| (t.origin[0] - x).abs() < 0.03
                        && (t.origin[2] - z).abs() < 0.03
                        && (t.radius - 2.25).abs() < 0.02),
                    "Missing fine bore at {x}, {z}"
                );
            }
        }
        assert_eq!(
            serde_json::to_value(&targets).unwrap(),
            serde_json::to_value(app.thread_targets(&[id]).unwrap()).unwrap(),
            "Target identity must survive independent queries"
        );
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        for t in &targets {
            let axis = glam::DVec3::from_array(t.axis);
            let center = glam::DVec3::from_array(t.origin) + axis * t.length * 0.5;
            let radial = axis.any_orthonormal_vector();
            app.camera.eye = center.as_vec3();
            app.camera.target = (center + radial * t.radius * 2.).as_vec3();
            app.camera.up = axis.as_vec3();
            let face = app.pick_face(rect.center(), rect).unwrap().0;
            let selected = app
                .thread_target_for_face(&face)
                .expect("entire bore must be threadable");
            assert_eq!(selected.face_id, t.face_id);
            let expected = 2. * std::f64::consts::PI * t.radius * t.length;
            assert!(
                (face.area_mm2 / expected - 1.).abs() < 0.02,
                "{} vs {expected}",
                face.area_mm2
            );
            assert!(app.begin_thread_on_face(&face));
        }
        for radius in [5.5, 2.25] {
            let t = targets
                .iter()
                .find(|t| (t.radius - radius).abs() < 0.02)
                .unwrap()
                .clone();
            let params = crate::threads::Params {
                diameter: 2. * (t.radius + 0.3),
                depth: 0.3,
                pitch: 1.,
                length: 3.,
                ..Default::default()
            };
            app.apply_threads(vec![t], params).unwrap();
            assert_ne!(app.doc.body(id).unwrap().shape, Some(shape));
            assert!(app.undo());
            assert_eq!(app.doc.body(id).unwrap().shape, Some(shape));
        }
    }

    #[test]
    fn thread_remaining_surfaces_survive_save_and_consecutive_apply() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let r = 3. - 0.61343;
        let shape = app
            .kernel
            .revolve_region(
                &Region2D::new(vec![[r, 0.], [6., 0.], [6., 10.], [r, 10.]]),
                &DatumPlane::Front.def(),
                &Axis2D {
                    point: [0., 0.],
                    dir: [0., 1.],
                },
                360.,
            )
            .unwrap();
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Tube".into(),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: "test tube".into(),
                operation: None,
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        let targets = app.thread_targets(&[id]).unwrap();
        assert_eq!(targets.len(), 2);
        let inner = targets.iter().find(|t| t.internal).unwrap().clone();
        agent_call(
            &mut app,
            "thread.apply",
            json!({"targets":[inner],"params":crate::threads::Params { length:10., ..Default::default() }}),
        );
        let project = encode_s33d_project(
            &app.doc,
            &app.kernel,
            &app.history,
            &app.selected,
            &app.camera,
            app.show_grid,
            app.snap_enabled,
            app.snap_step,
        )
        .unwrap();
        app.load_s33d_project(&project).unwrap();
        let remaining = app.thread_targets(&[id]).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(!remaining[0].internal);
        let mut p = crate::threads::preset_params("M12", false).unwrap();
        p.length = 10.;
        app.apply_threads(remaining, p).unwrap();
        assert!(app.thread_targets(&[id]).unwrap().is_empty());
        let Feature::Derived { source, .. } = &app.doc.body(id).unwrap().feature else {
            panic!("Thread provenance missing");
        };
        let saved: crate::threads::Saved =
            serde_json::from_str(source.strip_prefix("Thread v1: ").unwrap()).unwrap();
        assert_eq!(saved.annotations.len(), 2);
        assert!(saved.annotations.iter().any(|a| a.target.internal));
        assert!(saved.annotations.iter().any(|a| !a.target.internal));
        let shape = app.doc.body(id).unwrap().shape.unwrap();
        let mut mesh = app.kernel.tessellate(shape).unwrap();
        mesh.positions[0][0] += 1.;
        app.doc.body_mut(id).unwrap().shape = Some(app.kernel.insert_mesh(mesh));
        // A changed mesh is re-certified, never trusted from its old saved
        // record. Any recovered short uncut band must not resurrect either
        // original full-height wall after both have been threaded.
        let recovered = app.thread_targets(&[id]).unwrap();
        assert!(!recovered
            .iter()
            .any(|t| targets.iter().any(|old| t.same_wall(old))));
    }
    #[test]
    fn thread_batch_is_atomic_and_undoable() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let mut ids = Vec::new();
        for x in [0., 12.] {
            ids.push(
                agent_call(
                    &mut app,
                    "primitive.create",
                    json!({"kind":"cylinder","radius":3.,"height":10.,"position":[x,0.,0.]}),
                )["id"]
                    .as_u64()
                    .unwrap(),
            );
        }
        let targets = app.thread_targets(&ids).unwrap();
        assert_eq!(targets.len(), 2);
        // The first valid cutter must not leak a mutation when a later stock
        // diameter is incompatible with the same batch parameters.
        let bad_id = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"cylinder","radius":4.,"height":10.,"position":[24.,0.,0.]}),
        )["id"]
            .as_u64()
            .unwrap();
        let mixed = app.thread_targets(&[ids[0], bad_id]).unwrap();
        let mixed_before = app.agent_document_value();
        assert!(app.apply_threads(mixed, Default::default()).is_err());
        assert_eq!(app.agent_document_value(), mixed_before);
        app.doc.bodies.retain(|b| b.id != bad_id);
        let before = app.agent_document_value();
        assert!(app
            .apply_threads(
                targets.clone(),
                crate::threads::Params {
                    diameter: 8.,
                    ..Default::default()
                }
            )
            .is_err());
        assert_eq!(app.agent_document_value(), before);
        app.apply_threads(
            targets,
            crate::threads::Params {
                length: 10.,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(app.doc.bodies.len(), 2);
        assert!(app.thread_targets(&ids).unwrap().is_empty());
        agent_call(&mut app, "history.undo", json!({}));
        assert_eq!(app.thread_targets(&ids).unwrap().len(), 2);
        agent_call(&mut app, "history.redo", json!({}));
        assert!(app.thread_targets(&ids).unwrap().is_empty());
    }

    #[test]
    fn sketch_list_range_preserves_tool_and_anchor() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.sketch = Some(SketchSession::new(
            PlaneDef::from_normal([0., 0., 1.]),
            (0..5)
                .map(|i| SketchEntity::Line {
                    a: [i as f64, 0.],
                    b: [i as f64, 1.],
                })
                .collect(),
        ));
        for tool in [
            SketchTool::Pick,
            SketchTool::Align,
            SketchTool::Move,
            SketchTool::Rotate,
        ] {
            app.sketch.as_mut().unwrap().set_tool(tool);
            app.select_sketch_list_item(1, false, false);
            app.select_sketch_list_item(4, false, true);
            app.select_sketch_list_item(2, false, true);
            let s = app.sketch.as_ref().unwrap();
            assert_eq!(s.tool, tool);
            let selected = match tool {
                SketchTool::Align => s.align_selection.clone(),
                SketchTool::Move | SketchTool::Rotate => s.transform_selection.clone(),
                _ => s.selected_entities(),
            };
            assert_eq!(selected, vec![1, 2]);
            assert_eq!(s.list_selection_anchor, Some(1));
        }
    }

    #[test]
    fn objects_context_batch_actions_use_captured_range_and_one_undo_step() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let ids: Vec<_> = (0..4)
            .map(|_| {
                agent_call(
                    &mut app,
                    "sketch.create",
                    json!({"entities":[{"type":"line","start":[0,0],"end":[1,0]}]}),
                )["id"]
                    .as_u64()
                    .unwrap()
            })
            .collect();
        app.object_list_anchor = Some(ids[1]);
        app.select_object_range(ids[3], false);
        let targets = app.objects_context_targets(ids[0]);
        assert_eq!(targets, ids[1..]);
        assert_eq!(app.objects_context_targets(ids[2]), targets);
        let color = [0.9, 0.2, 0.3, 1.];
        let before_color = app.doc.body(ids[1]).unwrap().material.color;
        let before_steps = app.undo_selection_stack.len();
        app.selected = vec![ids[0]];
        app.set_bodies_color(&targets, color);
        assert_eq!(app.undo_selection_stack.len(), before_steps + 1);
        assert_eq!(app.doc.body(ids[0]).unwrap().material.color, before_color);
        assert!(targets
            .iter()
            .all(|&id| app.doc.body(id).unwrap().material.color == color));
        assert!(app.undo());
        assert!(targets
            .iter()
            .all(|&id| app.doc.body(id).unwrap().material.color == before_color));
        app.set_bodies_visible(&targets, false);
        assert!(targets.iter().all(|&id| !app.doc.body(id).unwrap().visible));
        assert!(app.doc.body(ids[0]).unwrap().visible);
        assert!(app.undo());
        assert!(targets.iter().all(|&id| app.doc.body(id).unwrap().visible));
        app.delete_bodies_from_objects(&targets);
        assert_eq!(app.doc.bodies.len(), 1);
        assert!(app.doc.body(ids[0]).is_some());
        assert_eq!(app.selected, vec![ids[0]]);
        assert!(app.undo());
        assert_eq!(app.doc.bodies.len(), 4);
        assert!(app.redo());
        assert_eq!(app.doc.bodies.len(), 1);
        app.selected = vec![ids[0]];
        assert_eq!(app.objects_context_targets(ids[0]), vec![ids[0]]);
        let steps = app.undo_selection_stack.len();
        app.set_bodies_color(&targets, color);
        app.set_bodies_visible(&targets, false);
        assert_eq!(
            steps,
            app.undo_selection_stack.len(),
            "Deleted menu targets must not retarget or create Undo entries"
        );
    }

    #[test]
    fn objects_context_visibility_is_atomic_with_locked_mirror_sources() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let ids: Vec<_> = (0..2)
            .map(|_| {
                agent_call(&mut app, "primitive.create", json!({"kind":"box"}))["id"]
                    .as_u64()
                    .unwrap()
            })
            .collect();
        app.selected = vec![ids[0]];
        app.start_mirror();
        assert_eq!(app.mode, Mode::Mirror);
        let steps = app.undo_selection_stack.len();
        app.set_bodies_visible(&ids, false);
        assert!(ids.iter().all(|&id| app.doc.body(id).unwrap().visible));
        assert_eq!(steps, app.undo_selection_stack.len());
    }

    #[test]
    fn object_list_range_uses_fixed_anchor_and_list_order() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let mut ids = Vec::new();
        for _ in 0..5 {
            ids.push(
                agent_call(
                    &mut app,
                    "sketch.create",
                    json!({"entities":[{"type":"line","start":[0,0],"end":[1,0]}]}),
                )["id"]
                    .as_u64()
                    .unwrap(),
            );
        }
        app.object_list_anchor = Some(ids[1]);
        app.select_object_range(ids[4], false);
        assert_eq!(app.selected, ids[1..=4]);
        app.select_object_range(ids[2], false);
        assert_eq!(app.selected, ids[1..=2]);
        app.select_object_range(ids[0], false);
        assert_eq!(app.selected, ids[0..=1]);
        app.select_object_range(ids[4], true);
        assert_eq!(app.selected, ids);
        app.doc.body_mut(ids[2]).unwrap().visible = false;
        app.select_object_range(ids[4], false);
        assert_eq!(app.selected, ids[1..=4]);
        app.select_object_range(ids[2], false);
        assert_eq!(app.selected, ids[1..=2]);
        app.select_from_objects(ids[2], false);
        assert_eq!(app.selected, vec![ids[2]]);
        assert!(!app.doc.body(ids[2]).unwrap().visible);
        app.object_list_anchor = Some(ids[2]);
        app.select_object_range(ids[4], false);
        assert_eq!(app.selected, ids[2..=4]);
        app.select_from_objects(ids[2], true);
        assert_eq!(app.selected, ids[3..=4]);
        app.object_list_anchor = Some(ids[1]);
        app.doc.bodies.reverse();
        app.select_object_range(ids[4], false);
        assert_eq!(app.selected, vec![ids[4], ids[3], ids[2], ids[1]]);
        assert!(!app.doc.body(ids[2]).unwrap().visible);
        app.set_bodies_visible(&app.selected.clone(), true);
        assert!(app.doc.bodies.iter().all(|body| body.visible));
        app.reset_document_tools();
        assert_eq!(app.object_list_anchor, None);
    }

    #[test]
    fn agent_recenter_and_catalog_expose_new_capabilities() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let catalog = agent_call(&mut app, "system.describe", json!({}));
        for name in ["sketch.trim.preview", "sketch.trim.apply", "view.recenter"] {
            assert!(catalog["methods"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["name"] == name));
        }
        agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"box","position":[100,0,0]}),
        );
        app.camera.ortho = true;
        let before = app.agent_document_value();
        agent_call(&mut app, "view.recenter", json!({}));
        assert!(!app.camera.ortho);
        assert!((app.camera.target.x - 100.0).abs() < 0.01);
        assert_eq!(app.agent_document_value(), before);
    }

    #[test]
    fn agent_control_sketch_region_extrudes_parametrically() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let sketch = agent_call(
            &mut app,
            "sketch.create",
            json!({
                "plane":"xy",
                "name":"Agent Profile",
                "entities":[{
                    "type":"rectangle",
                    "corner1":[-10.0,-5.0],
                    "corner2":[10.0,5.0]
                }]
            }),
        );
        let sketch_id = sketch["id"].as_u64().unwrap();
        assert_eq!(sketch["sketch"]["regions"][0]["area"], 200.0);

        let extrude = agent_call(
            &mut app,
            "feature.extrude",
            json!({
                "sketch_id":sketch_id,
                "region_indices":[0],
                "depth":8.0,
                "name":"Agent Extrude"
            }),
        );
        assert_eq!(extrude["feature_kind"], "extrude");
        assert_eq!(extrude["bounds"]["size"], json!([20.0, 10.0, 8.0]));
        assert!(matches!(
            app.doc.body(extrude["id"].as_u64().unwrap()).unwrap().feature,
            Feature::Extrude { source_sketch: Some(id), .. } if id == sketch_id
        ));
    }

    #[test]
    fn agent_control_boolean_keeps_stable_operand_ids() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let first = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"box", "position":[0.0,0.0,0.0], "width":20.0, "depth":20.0, "height":20.0}),
        )["id"]
            .as_u64()
            .unwrap();
        let second = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"box", "position":[8.0,0.0,0.0], "width":20.0, "depth":20.0, "height":20.0}),
        )["id"]
            .as_u64()
            .unwrap();
        let result = agent_call(
            &mut app,
            "boolean.apply",
            json!({
                "operation":"subtract",
                "body_ids":[first, second],
                "keep_originals":true,
                "name":"Agent Cut"
            }),
        );
        assert_eq!(result["name"], "Agent Cut");
        assert!(app.doc.body(first).unwrap().visible);
        assert!(app.doc.body(second).unwrap().visible);
        assert!(matches!(
            app.doc.body(result["id"].as_u64().unwrap()).unwrap().feature,
            Feature::Derived {
                operation: Some(DerivedOperation::Boolean { ref operands, .. }),
                ..
            } if operands == &[first, second]
        ));
    }

    #[test]
    fn ui_settings_load_missing_fields_with_stable_defaults() {
        let settings: AppSettings = serde_json::from_str(r#"{"beginner_mode":true}"#).unwrap();
        assert!(settings.beginner_mode);
        assert_eq!(settings.language, UiLanguage::English);
        assert_eq!(settings.font, UiFontChoice::Proportional);
        assert_eq!(settings.color_theme, UiColorTheme::Blue);
        assert_eq!(settings.unit, UiUnit::Millimeter);
        assert!(settings.show_first_use_tips);
        assert!(settings.agent_control_enabled);
        assert_eq!(settings.agent_control_port, DEFAULT_AGENT_PORT);
        assert!(!settings.calculator_plugin_enabled);
        assert!(!settings.gear_plugin_enabled);
        assert!(!settings.surface_texture_plugin_enabled);
        assert!(!settings.tire_plugin_enabled);
        assert!(!settings.wheel_rim_plugin_enabled);

        let disabled = AppSettings {
            agent_control_enabled: false,
            ..AppSettings::default()
        };
        let restored: AppSettings =
            serde_json::from_str(&serde_json::to_string(&disabled).unwrap()).unwrap();
        assert!(!restored.agent_control_enabled);
    }

    #[test]
    fn legacy_ui_settings_migrate_to_one_stable_user_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "s33d-ui-state-migration-{}-{unique}",
            std::process::id()
        ));
        let legacy_path = root.join("old-working-directory").join(UI_STATE_FILE_NAME);
        let stable_path = root.join("user-config").join(UI_STATE_FILE_NAME);
        let state = UiStateFile {
            objects_panel_pos_v2: Some([12.0, 34.0]),
            settings: AppSettings {
                agent_control_port: 53_434,
                ..AppSettings::default()
            },
        };
        write_ui_state(&legacy_path, &state).unwrap();

        let loaded = load_ui_state_from_paths(&stable_path, [legacy_path]);
        assert_eq!(loaded.settings.agent_control_port, 53_434);
        assert_eq!(read_ui_state(&stable_path), Some(state));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_registration_commands_are_scoped_to_only_the_s33d_entry() {
        let bridge = Path::new("<S33D_INSTALL_DIR>").join("s33d-agent.exe");
        let bridge_string = bridge.to_string_lossy().into_owned();
        let strings = |args: Vec<OsString>| {
            args.into_iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            strings(agent_cli_args(
                AgentIntegration::Codex,
                AgentConfigAction::Register,
                &bridge,
            )),
            vec![
                "mcp".to_string(),
                "add".to_string(),
                "s33d".to_string(),
                "--".to_string(),
                bridge_string.clone(),
                "mcp".to_string(),
            ]
        );
        assert_eq!(
            strings(agent_cli_args(
                AgentIntegration::Codex,
                AgentConfigAction::Unregister,
                &bridge,
            )),
            ["mcp", "remove", "s33d"]
        );
        assert_eq!(
            strings(agent_cli_args(
                AgentIntegration::ClaudeCode,
                AgentConfigAction::Register,
                &bridge,
            )),
            vec![
                "mcp".to_string(),
                "add".to_string(),
                "--transport".to_string(),
                "stdio".to_string(),
                "s33d".to_string(),
                "--scope".to_string(),
                "user".to_string(),
                "--".to_string(),
                bridge_string,
                "mcp".to_string(),
            ]
        );
        assert_eq!(
            strings(agent_cli_args(
                AgentIntegration::ClaudeCode,
                AgentConfigAction::Unregister,
                &bridge,
            )),
            ["mcp", "remove", "s33d", "--scope", "user"]
        );
    }

    #[test]
    fn agent_connection_test_uses_the_active_loopback_listener() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let working = AgentServer::start("127.0.0.1:0").unwrap();
        let working_address = working.address().unwrap();
        app.agent_server = working;
        app.settings.language = UiLanguage::English;
        app.settings.agent_control_enabled = true;
        app.settings.agent_control_port = working_address.port();
        app.agent_control_port_input = working_address.port();

        assert!(app.test_agent_control_connection());
        assert!(app
            .agent_control_status
            .as_deref()
            .unwrap()
            .contains("Connection test passed"));
    }

    #[test]
    fn occupied_agent_port_keeps_previous_listener_and_suggests_retry() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let blocker = AgentServer::start("127.0.0.1:0").unwrap();
        let occupied_port = blocker.address().unwrap().port();
        let working = AgentServer::start("127.0.0.1:0").unwrap();
        let working_address = working.address();
        app.agent_server = working;
        app.settings.language = UiLanguage::English;
        app.settings.agent_control_enabled = true;
        app.settings.agent_control_port = working_address.unwrap().port();

        assert!(!app.apply_agent_control_port(occupied_port));
        assert_eq!(app.agent_server.address(), working_address);
        assert_eq!(
            app.settings.agent_control_port,
            working_address.unwrap().port()
        );
        assert!(app
            .toasts
            .last()
            .unwrap()
            .0
            .contains("choose another port and try again"));
    }

    #[test]
    fn gear_plugin_creates_updates_and_undoes_one_parametric_object() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.generate_gear(GearParams::default(), None);
        assert_eq!(app.doc.bodies.len(), 1);
        let body_id = app.doc.bodies[0].id;
        let first_shape = app.doc.bodies[0].shape.unwrap();
        assert!(matches!(
            app.doc.bodies[0].feature,
            Feature::Gear { params } if params.teeth == 24
        ));

        let updated = GearParams {
            teeth: 30,
            ..GearParams::default()
        };
        app.generate_gear(updated, Some(body_id));
        assert_eq!(app.doc.bodies.len(), 1);
        assert_eq!(app.doc.bodies[0].id, body_id);
        assert_ne!(app.doc.bodies[0].shape, Some(first_shape));
        assert!(matches!(
            app.doc.bodies[0].feature,
            Feature::Gear { params } if params.teeth == 30
        ));

        assert!(app.undo());
        assert_eq!(app.doc.bodies[0].id, body_id);
        assert_eq!(app.doc.bodies[0].shape, Some(first_shape));
        assert!(matches!(
            app.doc.bodies[0].feature,
            Feature::Gear { params } if params.teeth == 24
        ));
    }

    #[test]
    fn tire_plugin_creates_updates_and_undoes_one_parametric_object() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.generate_tire(TireParams::default(), None);
        assert_eq!(app.doc.bodies.len(), 1);
        let body_id = app.doc.bodies[0].id;
        let first_shape = app.doc.bodies[0].shape.unwrap();
        assert!(matches!(
            app.doc.bodies[0].feature,
            Feature::Tire { params } if params.pattern == s33d_model::TireTreadPattern::HighwayRib
        ));

        let updated = TireParams {
            pattern: s33d_model::TireTreadPattern::Chevron,
            tread_repeats: 42,
            ..TireParams::default()
        };
        app.generate_tire(updated, Some(body_id));
        assert_eq!(app.doc.bodies.len(), 1);
        assert_eq!(app.doc.bodies[0].id, body_id);
        assert_ne!(app.doc.bodies[0].shape, Some(first_shape));
        assert!(matches!(
            app.doc.bodies[0].feature,
            Feature::Tire { params } if params.pattern == s33d_model::TireTreadPattern::Chevron
        ));

        assert!(app.undo());
        assert_eq!(app.doc.bodies[0].shape, Some(first_shape));
    }

    #[test]
    fn empty_document_scene_refresh_never_indexes_an_empty_selection() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.selected.clear();
        app.refresh_scene();
    }

    #[test]
    fn surface_extrude_accepts_two_disconnected_extensions_on_one_base() {
        let mut kernel = TruckKernel::new();
        let base_plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let base = kernel
            .extrude_region(
                &Region2D::new(vec![[0.0, 0.0], [30.0, 0.0], [30.0, 20.0], [0.0, 20.0]]),
                &base_plane,
                5.0,
            )
            .unwrap();
        let mut surface_plane = base_plane;
        surface_plane.origin = [0.0, 0.0, 5.0];
        let left = kernel
            .extrude_region(
                &Region2D::new(vec![[2.0, 2.0], [8.0, 2.0], [8.0, 8.0], [2.0, 8.0]]),
                &surface_plane,
                4.0,
            )
            .unwrap();
        let right = kernel
            .extrude_region(
                &Region2D::new(vec![[20.0, 10.0], [27.0, 10.0], [27.0, 17.0], [20.0, 17.0]]),
                &surface_plane,
                4.0,
            )
            .unwrap();
        let result = union_extensions_into_base(&mut kernel, base, vec![left, right], None)
            .expect("disconnected surface extensions should be applied one by one");
        let mesh = kernel.tessellate(result).unwrap();
        let (_, max) = mesh.bounds().unwrap();
        assert!((max.z - 9.0).abs() < 1e-3, "max={max:?}");
    }

    #[test]
    fn ui_units_roundtrip_without_changing_internal_millimeters() {
        let samples = [0.0, 1.0, 25.4, 304.8, 1_000.0, -12.75];
        for unit in [
            UiUnit::Millimeter,
            UiUnit::Centimeter,
            UiUnit::Meter,
            UiUnit::Inch,
            UiUnit::Foot,
        ] {
            for millimeters in samples {
                let roundtrip = unit.to_mm(unit.from_mm(millimeters));
                assert!((roundtrip - millimeters).abs() < 1e-10);
            }
        }
        assert!((UiUnit::Inch.from_mm(25.4) - 1.0).abs() < 1e-12);
        assert!((UiUnit::Foot.from_mm(304.8) - 1.0).abs() < 1e-12);
        assert!((UiUnit::Centimeter.from_mm(10.0) - 1.0).abs() < 1e-12);
        assert!((UiUnit::Meter.from_mm(1_000.0) - 1.0).abs() < 1e-12);
    }

    fn mat_apply(mat: glam::Mat4, p: [f32; 3]) -> [f32; 3] {
        mat.transform_point3(glam::Vec3::from_array(p)).to_array()
    }

    fn chamfer_body(id: BodyId, target: BodyId, base: BodyId, visible: bool) -> Body {
        Body {
            id,
            name: format!("Chamfer {id}"),
            kind: BodyKind::Solid,
            feature: Feature::Chamfer {
                target,
                base: Some(base),
                edges: Vec::new(),
                reference_bounds: None,
                chamfer: None,
                fillet: Some(2.0),
            },
            shape: None,
            material: Material::default(),
            visible,
        }
    }

    #[test]
    fn linear_plane_equation_builds_the_requested_offset_plane() {
        let equation = [2.0, 1.0, 3.0, 5.0];
        let plane = plane_from_equation(equation).unwrap();
        let origin = plane.origin();
        assert!((2.0 * origin.x + origin.y + 3.0 * origin.z - 5.0).abs() < 1e-12);
        assert!(
            plane
                .normal()
                .dot(glam::DVec3::new(2.0, 1.0, 3.0).normalize())
                > 0.999999
        );
        assert!(plane_from_equation([0.0, 0.0, 0.0, 5.0]).is_err());
    }

    #[test]
    fn s33d_project_roundtrip_restores_geometry_history_selection_and_undo() {
        let mut kernel = TruckKernel::new();
        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let region = Region2D::new(vec![[-5.0, -4.0], [5.0, -4.0], [5.0, 4.0], [-5.0, 4.0]]);
        let shape = kernel.extrude_region(&region, &plane, 12.0).unwrap();
        let mut document = Document::new("roundtrip");
        document.begin_change();
        let body_id = document.alloc_id();
        document.add_body(Body {
            id: body_id,
            name: "Box before rename".into(),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: "test solid".into(),
                operation: None,
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        document.begin_change();
        document.body_mut(body_id).unwrap().name = "Saved box".into();
        let camera = Camera::default();
        let encoded = encode_s33d_project(
            &document,
            &kernel,
            &["Created box".into(), "Renamed box".into()],
            &[body_id],
            &camera,
            true,
            true,
            2.5,
        )
        .unwrap();

        let (mut restored, restored_kernel, history, selected, _, grid, snap, step) =
            decode_s33d_project(&encoded).unwrap();
        assert_eq!(restored.name(), "roundtrip");
        assert_eq!(restored.body(body_id).unwrap().name, "Saved box");
        let restored_shape = restored.body(body_id).unwrap().shape.unwrap();
        assert!(!restored_kernel
            .tessellate(restored_shape)
            .unwrap()
            .is_empty());
        assert_eq!(history, vec!["Created box", "Renamed box"]);
        assert_eq!(selected, vec![body_id]);
        assert!(grid && snap);
        assert_eq!(step, 2.5);
        assert!(restored.undo());
        assert_eq!(restored.body(body_id).unwrap().name, "Box before rename");
        assert!(restored_kernel
            .tessellate(restored.body(body_id).unwrap().shape.unwrap())
            .is_ok());
    }

    #[test]
    fn exact_project_and_shell_keep_brep_through_undo_and_reopen() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let region = Region2D::new(vec![[0.0, 0.0], [20.0, 0.0], [20.0, 30.0], [0.0, 30.0]]);
        let mesh_shape = app.kernel.extrude_region(&region, &plane, 40.0).unwrap();
        let path = std::env::temp_dir().join(format!(
            "s33d-exact-project-{}-{}.step",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        app.kernel
            .export_step(mesh_shape, path.to_str().unwrap())
            .unwrap();
        let exact = app.kernel.import_step(path.to_str().unwrap()).unwrap()[0];
        std::fs::remove_file(path).unwrap();
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Exact input".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "fixture.step".into(),
            },
            shape: Some(exact),
            material: Material::default(),
            visible: true,
        });
        app.selected = vec![id];
        app.start_offset_shell();
        let ctx = app.offset_ctx.as_mut().unwrap();
        ctx.faces = vec![0];
        ctx.distance = -1.0;
        ctx.preview = Some(
            ctx.input
                .offset(ctx.distance, ctx.tolerance, &ctx.faces, ctx.operation)
                .unwrap(),
        );
        app.commit_offset_shell();
        assert_eq!(app.doc.bodies.len(), 2);
        let result_id = app.selected[0];
        let payload = encode_s33d_project(
            &app.doc,
            &app.kernel,
            &[],
            &app.selected,
            &app.camera,
            true,
            true,
            1.0,
        )
        .unwrap();
        assert!(payload.contains("occt_brep"));
        let (mut doc, kernel, ..) = decode_s33d_project(&payload).unwrap();
        assert!(kernel
            .exact_shape(doc.body(result_id).unwrap().shape.unwrap())
            .is_some());
        assert!(doc.undo());
        assert_eq!(doc.bodies.len(), 1);
        assert!(kernel
            .exact_shape(doc.body(id).unwrap().shape.unwrap())
            .is_some());
        assert!(doc.redo());
        assert_eq!(doc.bodies.len(), 2);
        let mut legacy: Value = serde_json::from_str(&payload).unwrap();
        legacy["version"] = json!(1);
        for shape in legacy["shapes"].as_array_mut().unwrap() {
            shape.as_object_mut().unwrap().remove("occt_brep");
        }
        let (_, legacy_kernel, ..) = decode_s33d_project(&legacy.to_string()).unwrap();
        assert!(legacy_kernel.shape_count() > 0);
        let mut corrupt: Value = serde_json::from_str(&payload).unwrap();
        corrupt["shapes"][0]["occt_brep"] = json!("invalid B-Rep");
        assert!(
            decode_s33d_project(&corrupt.to_string()).is_err(),
            "must not silently fall back to the cached display mesh"
        );
    }

    #[test]
    fn section_view_partitions_cap_from_surface_without_duplicate_triangles() {
        let mut kernel = TruckKernel::new();
        let base_plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let region = Region2D::new(vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]);
        let shape = kernel.extrude_region(&region, &base_plane, 10.0).unwrap();
        let mesh = kernel.tessellate(shape).unwrap();
        let mut section_plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        section_plane.origin = [0.0, 0.0, 5.0];
        let (positive, _) = s33d_kernel::mesh_bool::split_by_plane(
            &mesh,
            section_plane.origin,
            section_plane.normal().to_array(),
        )
        .unwrap();
        let (surface, cap) = section_surface_and_cap_indices(&positive, &section_plane);
        assert!(!surface.is_empty() && !cap.is_empty());
        assert_eq!(surface.len() + cap.len(), positive.indices.len());
        let keys = |indices: &[u32]| {
            indices
                .chunks_exact(3)
                .map(|triangle| {
                    let mut key = [triangle[0], triangle[1], triangle[2]];
                    key.sort_unstable();
                    key
                })
                .collect::<std::collections::HashSet<_>>()
        };
        assert!(keys(&surface).is_disjoint(&keys(&cap)));
    }

    #[test]
    fn section_cut_face_keeps_object_hue_without_becoming_black() {
        let color = [0.52, 0.68, 0.91];
        let cut = section_cut_face_color(color);
        for axis in 0..3 {
            assert!(cut[axis] > 0.20);
            assert!(cut[axis] < color[axis]);
            assert!((cut[axis] / color[axis] - 0.72).abs() < 1e-6);
        }
    }

    #[test]
    fn rebuild_dependencies_follow_boolean_operands_and_chamfer_base() {
        let boolean = Feature::Derived {
            source: "Union of 3, 8".into(),
            operation: Some(DerivedOperation::Boolean {
                op: DerivedBooleanOp::Union,
                operands: vec![3, 8],
            }),
        };
        assert_eq!(rebuild_feature_dependencies(&boolean), vec![3, 8]);
        let legacy_boolean = Feature::Derived {
            source: "Subtract of 11 and 12".into(),
            operation: None,
        };
        assert_eq!(
            derived_boolean_recipe(&legacy_boolean),
            Some((DerivedBooleanOp::Subtract, vec![11, 12]))
        );

        let chamfer = Feature::Chamfer {
            target: 3,
            base: Some(9),
            edges: Vec::new(),
            reference_bounds: None,
            chamfer: Some(1.0),
            fillet: None,
        };
        assert_eq!(rebuild_feature_dependencies(&chamfer), vec![9]);

        let plane = PlaneDef::from_normal([1.0, 0.0, 0.0]);
        for feature in [
            Feature::Derived {
                source: "mirror of 12".into(),
                operation: Some(DerivedOperation::Mirror { source: 12, plane }),
            },
            Feature::Derived {
                source: "copy of 12".into(),
                operation: Some(DerivedOperation::Copy {
                    source: 12,
                    offset: [20.0, 0.0, 0.0],
                    primitive_snapshot: None,
                }),
            },
            Feature::Derived {
                source: "split of 12".into(),
                operation: Some(DerivedOperation::Split {
                    source: 12,
                    plane,
                    piece_index: 0,
                }),
            },
        ] {
            assert_eq!(rebuild_feature_dependencies(&feature), vec![12]);
        }
    }

    #[test]
    fn sketch_edit_rebuilds_multiple_boolean_levels_transactionally() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.doc = Document::new("dependency graph");
        app.kernel = TruckKernel::new();

        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let original_sketch = SketchFeature {
            vertex_locks: Default::default(),
            plane,
            entities: vec![SketchEntity::Rect {
                c1: [-2.0, -2.0],
                c2: [2.0, 2.0],
            }],
            world_xyz_coordinates: false,
            on_body: None,
        };
        let updated_sketch = SketchFeature {
            entities: vec![SketchEntity::Rect {
                c1: [-5.0, -3.0],
                c2: [2.0, 3.0],
            }],
            ..original_sketch.clone()
        };
        let params = ExtrudeParams {
            depth: 5.0,
            symmetric: false,
            bevel: 0.0,
        };

        let sketch_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: sketch_id,
            name: "Sketch 1".into(),
            kind: BodyKind::Sketch,
            feature: Feature::Sketch(original_sketch.clone()),
            shape: None,
            material: Material::default(),
            visible: true,
        });
        let extrude_shape = build_extrude_shape(
            &mut app.kernel,
            &original_sketch,
            &s33d_model::compute_regions(&original_sketch.entities),
            &params,
        )
        .unwrap();
        let extrude_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: extrude_id,
            name: "Extrude 2".into(),
            kind: BodyKind::Solid,
            feature: Feature::Extrude {
                sketch: original_sketch,
                params: params.clone(),
                source_sketch: Some(sketch_id),
                pocket_source_sketches: Vec::new(),
                region_indices: vec![0],
            },
            shape: Some(extrude_shape),
            material: Material::default(),
            visible: false,
        });

        let make_fixed = |kernel: &mut TruckKernel, min_x: f64, max_x: f64| {
            kernel
                .extrude_region(
                    &Region2D::new(vec![
                        [min_x, -2.0],
                        [max_x, -2.0],
                        [max_x, 2.0],
                        [min_x, 2.0],
                    ]),
                    &plane,
                    5.0,
                )
                .unwrap()
        };
        let tool_a_shape = make_fixed(&mut app.kernel, 1.0, 4.0);
        let tool_a = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: tool_a,
            name: "Tool A".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported { path: "A".into() },
            shape: Some(tool_a_shape),
            material: Material::default(),
            visible: false,
        });
        let first_union_shape = app
            .kernel
            .boolean_union(extrude_shape, tool_a_shape)
            .unwrap();
        let first_union = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: first_union,
            name: "Union A".into(),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: "Union of 2, 3".into(),
                operation: Some(DerivedOperation::Boolean {
                    op: DerivedBooleanOp::Union,
                    operands: vec![extrude_id, tool_a],
                }),
            },
            shape: Some(first_union_shape),
            material: Material::default(),
            visible: false,
        });

        let tool_b_shape = make_fixed(&mut app.kernel, 3.0, 7.0);
        let tool_b = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: tool_b,
            name: "Tool B".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported { path: "B".into() },
            shape: Some(tool_b_shape),
            material: Material::default(),
            visible: false,
        });
        let second_union_shape = app
            .kernel
            .boolean_union(first_union_shape, tool_b_shape)
            .unwrap();
        let second_union = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: second_union,
            name: "Union B".into(),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: "Union of 4, 5".into(),
                operation: Some(DerivedOperation::Boolean {
                    op: DerivedBooleanOp::Union,
                    operands: vec![first_union, tool_b],
                }),
            },
            shape: Some(second_union_shape),
            material: Material::default(),
            visible: true,
        });

        let updates = app
            .rebuild_sketch_dependents(sketch_id, &updated_sketch)
            .unwrap();
        assert_eq!(
            updates.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
            vec![extrude_id, first_union, second_union]
        );
        assert_eq!(
            app.doc.body(second_union).unwrap().shape,
            Some(second_union_shape),
            "rebuild preparation must not mutate the document"
        );
        let rebuilt_final = updates.last().unwrap().1;
        let (min, _) = app
            .kernel
            .tessellate(rebuilt_final)
            .unwrap()
            .bounds()
            .unwrap();
        assert!(min.x < -4.9, "edited sketch must reach the final union");
    }

    #[test]
    fn live_sketch_edit_updates_dependents_cancel_restores_and_done_is_one_undo() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.doc = Document::new("live sketch transaction");
        app.kernel = TruckKernel::new();

        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let original_sketch = SketchFeature {
            vertex_locks: Default::default(),
            plane,
            entities: vec![SketchEntity::Rect {
                c1: [-2.0, -2.0],
                c2: [2.0, 2.0],
            }],
            world_xyz_coordinates: false,
            on_body: None,
        };
        let params = ExtrudeParams {
            depth: 5.0,
            symmetric: false,
            bevel: 0.0,
        };
        let sketch_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: sketch_id,
            name: "Sketch 1".into(),
            kind: BodyKind::Sketch,
            feature: Feature::Sketch(original_sketch.clone()),
            shape: None,
            material: Material::default(),
            visible: true,
        });
        let original_shape = build_extrude_shape(
            &mut app.kernel,
            &original_sketch,
            &s33d_model::compute_regions(&original_sketch.entities),
            &params,
        )
        .unwrap();
        let extrude_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: extrude_id,
            name: "Extrude 2".into(),
            kind: BodyKind::Solid,
            feature: Feature::Extrude {
                sketch: original_sketch,
                params,
                source_sketch: Some(sketch_id),
                pocket_source_sketches: Vec::new(),
                region_indices: vec![0],
            },
            shape: Some(original_shape),
            material: Material::default(),
            visible: true,
        });
        app.selected = vec![sketch_id];

        let resize = |app: &mut S33DApp| {
            let sketch = app.sketch.as_mut().expect("edit session must be open");
            let SketchEntity::Rect { c2, .. } = &mut sketch.entities[0] else {
                panic!("test sketch must remain a rectangle");
            };
            c2[0] = 7.0;
        };

        app.edit_sketch(sketch_id);
        resize(&mut app);
        assert!(app.sync_active_sketch_edit());
        let live_shape = app.doc.body(extrude_id).unwrap().shape.unwrap();
        assert_ne!(live_shape, original_shape);
        let (_, live_max) = app.kernel.tessellate(live_shape).unwrap().bounds().unwrap();
        assert!(live_max.x > 6.9, "dependent solid must resize before Done");

        app.cancel_sketch();
        assert_eq!(
            app.doc.body(extrude_id).unwrap().shape,
            Some(original_shape),
            "Cancel must restore the pre-edit document"
        );
        assert!(app.kernel.tessellate(original_shape).is_ok());

        app.selected = vec![sketch_id];
        app.edit_sketch(sketch_id);
        resize(&mut app);
        assert!(app.sync_active_sketch_edit());
        app.finish_sketch();
        let committed_shape = app.doc.body(extrude_id).unwrap().shape.unwrap();
        assert_ne!(committed_shape, original_shape);
        assert!(app.undo(), "Done must create one undoable document change");
        assert_eq!(
            app.doc.body(extrude_id).unwrap().shape,
            Some(original_shape)
        );
    }

    #[test]
    fn sketch_trim_supports_preview_repeat_undo_redo_and_document_history() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
        let initial = vec![
            SketchEntity::Rect {
                c1: [0.0, 0.0],
                c2: [10.0, 8.0],
            },
            SketchEntity::Line {
                a: [4.0, -3.0],
                b: [4.0, 11.0],
            },
        ];
        app.sketch.as_mut().unwrap().entities = initial.clone();
        app.sync_sketch_history();
        app.set_sketch_tool(SketchTool::Trim);
        let sk = app.sketch.as_mut().unwrap();
        sk.pointer_move_with_snap([4.0, 9.0], true, true, 100.0, 0.4);
        assert!(
            sk.trim_preview.is_some(),
            "grid must not displace the trim cursor"
        );
        assert!(!sk.begin_entity_drag([4.0, 9.0], 0.4, false));
        assert!(sk.trim_at([4.0, 9.0], 0.4));
        assert!(sk.trim_preview.is_none());
        assert_eq!(sk.tool, SketchTool::Trim);
        app.sync_sketch_history();
        let once = app.sketch.as_ref().unwrap().entities.clone();
        assert!(app.sketch.as_mut().unwrap().trim_at([4.0, -2.0], 0.4));
        app.sync_sketch_history();
        let twice = app.sketch.as_ref().unwrap().entities.clone();
        assert_eq!(app.sketch.as_ref().unwrap().regions().len(), 2);
        assert!(app.undo());
        assert_eq!(app.sketch.as_ref().unwrap().entities, once);
        assert!(app.undo());
        assert_eq!(app.sketch.as_ref().unwrap().entities, initial);
        assert!(app.redo());
        assert!(app.redo());
        assert_eq!(app.sketch.as_ref().unwrap().entities, twice);
        app.finish_sketch();
        let id = app.selected[0];
        assert!(app.undo());
        assert_eq!(app.doc.body(id).unwrap().sketch().unwrap().entities, once);
        assert!(app.redo());
        assert_eq!(app.doc.body(id).unwrap().sketch().unwrap().entities, twice);
        assert_eq!(app.regions_of(id).len(), 2);
        app.selected = vec![id];
        assert!(app.set_region_selection(app.selected[0], vec![0]));
        app.start_extrude();
        assert!(app.extrude_ctx.as_ref().unwrap().preview_error.is_none());
        app.set_extrude_depth(4.0);
        app.commit_extrude();
        assert_eq!(
            app.doc.bodies.len(),
            2,
            "trimmed closed region must actually commit an extrusion"
        );
    }

    #[test]
    fn sketch_trim_viewport_uses_real_pointer_and_never_drags() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0));
        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        app.start_sketch(plane);
        let initial = vec![SketchEntity::Line {
            a: [-20.0, 3.0],
            b: [20.0, 3.0],
        }];
        app.sketch.as_mut().unwrap().entities = initial.clone();
        app.sync_sketch_history();
        app.set_sketch_tool(SketchTool::Trim);
        app.snap_step = 100.0;
        app.snap_enabled = true;
        let world = glam::Vec3::from_array(plane.to_world(0.0, 3.0).map(|v| v as f32));
        let pointer = app.world_to_viewport(world, app.viewport_rect).unwrap();
        app.on_pointer_move(pointer, app.viewport_rect);
        assert!(app.sketch.as_ref().unwrap().trim_preview.is_some());
        app.sketch_pointer_down_at(pointer, app.viewport_rect, false, true);
        assert_eq!(app.sketch.as_ref().unwrap().entities, initial);
        app.sketch_pointer_down_at(pointer, app.viewport_rect, false, false);
        assert!(app.sketch.as_ref().unwrap().entities.is_empty());
        assert!(app.undo());
        assert_eq!(app.sketch.as_ref().unwrap().entities, initial);
        assert!(app.sketch.as_ref().unwrap().trim_preview.is_none());
    }

    #[test]
    fn sketch_trim_last_entity_of_existing_sketch_stays_deleted_on_exit() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
        app.sketch
            .as_mut()
            .unwrap()
            .entities
            .push(SketchEntity::Line {
                a: [0.0, 0.0],
                b: [10.0, 0.0],
            });
        app.finish_sketch();
        let id = app.selected[0];
        app.edit_sketch(id);
        app.set_sketch_tool(SketchTool::Trim);
        assert!(app.sketch.as_mut().unwrap().trim_at([5.0, 0.0], 0.2));
        app.sync_sketch_history();
        app.exit_sketch();
        assert_eq!(app.mode, Mode::Idle);
        assert!(app
            .doc
            .body(id)
            .unwrap()
            .sketch()
            .unwrap()
            .entities
            .is_empty());
        assert!(app.undo());
        assert_eq!(
            app.doc.body(id).unwrap().sketch().unwrap().entities.len(),
            1
        );
    }

    #[test]
    fn completed_new_sketch_keeps_each_drawing_step_in_document_undo() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));

        app.sketch
            .as_mut()
            .unwrap()
            .entities
            .push(SketchEntity::Rect {
                c1: [0.0, 0.0],
                c2: [10.0, 8.0],
            });
        app.sync_sketch_history();
        app.sketch
            .as_mut()
            .unwrap()
            .entities
            .push(SketchEntity::Circle {
                c: [20.0, 4.0],
                r: 3.0,
            });
        app.sync_sketch_history();
        app.finish_sketch();

        let sketch_id = app.selected[0];
        assert_eq!(
            app.doc
                .body(sketch_id)
                .unwrap()
                .sketch()
                .unwrap()
                .entities
                .len(),
            2
        );
        assert!(app.undo());
        assert_eq!(
            app.doc
                .body(sketch_id)
                .unwrap()
                .sketch()
                .unwrap()
                .entities
                .len(),
            1,
            "first Undo after Done must remove only the last drawing step"
        );
        assert!(app.undo());
        assert!(
            app.doc.body(sketch_id).is_none(),
            "only the final retained Undo removes the newly created Sketch"
        );
        assert!(app.redo());
        assert_eq!(
            app.doc
                .body(sketch_id)
                .unwrap()
                .sketch()
                .unwrap()
                .entities
                .len(),
            1
        );
        assert!(app.redo());
        assert_eq!(
            app.doc
                .body(sketch_id)
                .unwrap()
                .sketch()
                .unwrap()
                .entities
                .len(),
            2
        );
    }

    #[test]
    fn sketch_history_is_bounded_to_two_hundred_steps() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));

        for index in 0..215 {
            app.sketch
                .as_mut()
                .unwrap()
                .entities
                .push(SketchEntity::Circle {
                    c: [index as f64 * 3.0, 0.0],
                    r: 1.0,
                });
            app.sync_sketch_history();
        }
        assert_eq!(Document::MAX_UNDO_STEPS, 200);
        assert_eq!(app.sketch_undo_stack.len(), 200);
        assert_eq!(app.history.len(), 200);
    }

    #[test]
    fn extrude_undo_reselects_every_nested_sketch_region_consistently() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
        for entity in [
            SketchEntity::Circle {
                c: [0.0, 0.0],
                r: 40.0,
            },
            SketchEntity::Polygon {
                c: [0.0, 0.0],
                r: 53.0,
                n: 6,
                rot: 0.0,
            },
            SketchEntity::Rect {
                c1: [-70.0, -80.0],
                c2: [70.0, 80.0],
            },
        ] {
            app.sketch.as_mut().unwrap().entities.push(entity);
            app.sync_sketch_history();
        }
        app.finish_sketch();
        let sketch_id = app.selected[0];
        let expected = app.regions_of(sketch_id);
        assert_eq!(expected.len(), 3);

        assert!(app.set_region_selection(app.selected[0], vec![1]));
        app.start_extrude();
        let first = app.extrude_ctx.as_ref().unwrap();
        assert_eq!(first.regions, vec![expected[1].clone()]);
        assert!(first.preview_error.is_none());
        assert_eq!(first.ghost_shapes.len(), 1);
        app.commit_extrude();
        assert_eq!(app.doc.bodies.len(), 2);

        assert!(app.undo());
        assert_eq!(app.selected, vec![sketch_id]);
        assert_eq!(app.doc.bodies.len(), 1);
        assert_eq!(app.regions_of(sketch_id), expected);

        for (region_index, region) in expected.iter().enumerate() {
            app.selected = vec![sketch_id];
            assert!(app.set_region_selection(app.selected[0], vec![region_index]));
            app.start_extrude();
            let context = app.extrude_ctx.as_ref().unwrap();
            assert_eq!(context.regions, vec![region.clone()]);
            assert!(
                context.preview_error.is_none(),
                "region {region_index} preview failed: {:?}",
                context.preview_error
            );
            assert_eq!(context.ghost_shapes.len(), 1);
            app.cancel_extrude();
        }
    }

    #[test]
    fn sketch_edit_rebuilds_copy_mirror_and_each_split_piece() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.doc = Document::new("derived rebuild chain");
        app.kernel = TruckKernel::new();

        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let original_sketch = SketchFeature {
            vertex_locks: Default::default(),
            plane,
            entities: vec![SketchEntity::Rect {
                c1: [-2.0, -2.0],
                c2: [2.0, 2.0],
            }],
            world_xyz_coordinates: false,
            on_body: None,
        };
        let updated_sketch = SketchFeature {
            entities: vec![SketchEntity::Rect {
                c1: [-2.0, -2.0],
                c2: [4.0, 2.0],
            }],
            ..original_sketch.clone()
        };
        let params = ExtrudeParams {
            depth: 5.0,
            symmetric: false,
            bevel: 0.0,
        };
        let sketch_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: sketch_id,
            name: "Sketch".into(),
            kind: BodyKind::Sketch,
            feature: Feature::Sketch(original_sketch.clone()),
            shape: None,
            material: Material::default(),
            visible: true,
        });
        let extrude_shape = build_extrude_shape(
            &mut app.kernel,
            &original_sketch,
            &s33d_model::compute_regions(&original_sketch.entities),
            &params,
        )
        .unwrap();
        let extrude_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: extrude_id,
            name: "Extrude".into(),
            kind: BodyKind::Solid,
            feature: Feature::Extrude {
                sketch: original_sketch,
                params,
                source_sketch: Some(sketch_id),
                pocket_source_sketches: Vec::new(),
                region_indices: vec![0],
            },
            shape: Some(extrude_shape),
            material: Material::default(),
            visible: false,
        });

        let offset = [10.0, 0.0, 0.0];
        let copy_shape = app
            .kernel
            .transform(
                extrude_shape,
                &glam::Mat4::from_translation(glam::Vec3::new(10.0, 0.0, 0.0)),
            )
            .unwrap();
        let copy_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: copy_id,
            name: "Copy".into(),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: format!("copy of {extrude_id}"),
                operation: Some(DerivedOperation::Copy {
                    source: extrude_id,
                    offset,
                    primitive_snapshot: None,
                }),
            },
            shape: Some(copy_shape),
            material: Material::default(),
            visible: false,
        });

        let mirror_plane = PlaneDef::from_normal([1.0, 0.0, 0.0]);
        let mirror_shape = app.kernel.mirror(copy_shape, &mirror_plane).unwrap();
        let mirror_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: mirror_id,
            name: "Mirror".into(),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: format!("mirror of {copy_id}"),
                operation: Some(DerivedOperation::Mirror {
                    source: copy_id,
                    plane: mirror_plane,
                }),
            },
            shape: Some(mirror_shape),
            material: Material::default(),
            visible: false,
        });

        let mut split_plane = PlaneDef::from_normal([1.0, 0.0, 0.0]);
        split_plane.origin = [-10.0, 0.0, 0.0];
        let split_shapes = app
            .kernel
            .split_by_plane(mirror_shape, &split_plane)
            .unwrap();
        assert_eq!(split_shapes.len(), 2);
        let mut split_ids = Vec::new();
        for (piece_index, shape) in split_shapes.into_iter().enumerate() {
            let id = app.doc.alloc_id();
            app.doc.add_body(Body {
                id,
                name: format!("Split {piece_index}"),
                kind: BodyKind::Solid,
                feature: Feature::Derived {
                    source: format!("split of {mirror_id}"),
                    operation: Some(DerivedOperation::Split {
                        source: mirror_id,
                        plane: split_plane,
                        piece_index,
                    }),
                },
                shape: Some(shape),
                material: Material::default(),
                visible: true,
            });
            split_ids.push(id);
        }

        let rebuilt = app
            .rebuild_sketch_dependents(sketch_id, &updated_sketch)
            .unwrap();
        let rebuilt_ids: Vec<_> = rebuilt.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(
            rebuilt_ids,
            vec![extrude_id, copy_id, mirror_id, split_ids[0], split_ids[1]]
        );
        for id in [copy_id, mirror_id, split_ids[0], split_ids[1]] {
            let shape = rebuilt
                .iter()
                .find(|(body_id, _, _)| *body_id == id)
                .unwrap()
                .1;
            assert!(!app.kernel.tessellate(shape).unwrap().is_empty());
        }
    }

    #[test]
    fn chamfer_edge_signature_resolves_after_base_dimensions_change() {
        let mut kernel = TruckKernel::new();
        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let old_shape = kernel
            .extrude_region(
                &Region2D::new(vec![[-5.0, -4.0], [5.0, -4.0], [5.0, 4.0], [-5.0, 4.0]]),
                &plane,
                8.0,
            )
            .unwrap();
        let new_shape = kernel
            .extrude_region(
                &Region2D::new(vec![[-8.0, -6.0], [8.0, -6.0], [8.0, 6.0], [-8.0, 6.0]]),
                &plane,
                12.0,
            )
            .unwrap();
        let old_mesh = kernel.tessellate(old_shape).unwrap();
        let new_mesh = kernel.tessellate(new_shape).unwrap();
        let old_bounds = mesh_reference_bounds(&old_mesh).unwrap();
        let stored = mesh_sharp_edge_segments(&old_mesh)
            .into_iter()
            .next()
            .expect("box must expose at least one sharp chain");
        let remapped = remap_chamfer_segments(&new_mesh, &stored, old_bounds)
            .expect("the same topological chain should resolve on the resized box");
        let old_signature = edge_chain_signature(&stored, old_bounds).unwrap();
        let new_signature =
            edge_chain_signature(&remapped, mesh_reference_bounds(&new_mesh).unwrap()).unwrap();
        assert_eq!(old_signature.closed, new_signature.closed);
        assert!(old_signature.center.distance(new_signature.center) < 0.2);
    }

    #[test]
    fn creation_switch_commits_preview_and_preserves_invalid_session() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.begin_plane_pick();
        app.start_insert_3d();
        assert_eq!(app.mode, Mode::Insert3D);
        app.choose_insert_3d_primitive(Primitive3DKind::Box);
        app.begin_plane_pick();
        assert_eq!(app.mode, Mode::PickPlane);
        assert!(
            app.doc.bodies.is_empty(),
            "unplaced defaults are not objects"
        );
        app.start_insert_3d();
        app.choose_insert_3d_primitive(Primitive3DKind::Box);
        app.insert_3d_ctx.as_mut().unwrap().stage = Draw3DPlacementStage::Sizing;
        app.update_insert_3d_params(Primitive3DParams::defaults(Primitive3DKind::Box));
        app.choose_insert_3d_primitive(Primitive3DKind::Sphere);
        assert_eq!(app.doc.bodies.len(), 1);
        let first = app.doc.bodies[0].shape.unwrap();
        app.insert_3d_ctx.as_mut().unwrap().stage = Draw3DPlacementStage::Precision;
        let mut params = Primitive3DParams::defaults(Primitive3DKind::Sphere);
        params.radius = -1.0;
        app.update_insert_3d_params(params);
        app.begin_plane_pick();
        assert_eq!(app.mode, Mode::Insert3D, "failed Apply must not exit");
        assert_eq!(
            app.insert_3d_ctx.as_ref().unwrap().params.unwrap().radius,
            -1.0
        );
        assert_eq!(app.doc.bodies.len(), 1);
        assert!(app.kernel.tessellate(first).is_ok());
        params.radius = 5.0;
        app.update_insert_3d_params(params);
        app.toggle_align_tool();
        assert_eq!(app.mode, Mode::Idle);
        assert!(app.align_active);
        assert_eq!(app.doc.bodies.len(), 2);
        assert!(app.undo());
        assert_eq!(app.doc.bodies.len(), 1);
    }

    #[test]
    fn creation_switch_keeps_sketch_and_exits_empty_sketch() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
        app.start_insert_3d();
        assert_eq!(app.mode, Mode::Insert3D);
        assert!(app.doc.bodies.is_empty());
        app.begin_plane_pick();
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
        app.sketch
            .as_mut()
            .unwrap()
            .entities
            .push(SketchEntity::Circle {
                c: [0.0, 0.0],
                r: 5.0,
            });
        app.start_insert_3d();
        assert_eq!(app.mode, Mode::Insert3D);
        assert_eq!(app.doc.bodies.len(), 1);
        assert_eq!(app.doc.bodies[0].sketch().unwrap().entities.len(), 1);
    }

    #[test]
    fn insert_3d_builds_all_default_primitives_as_finite_solids() {
        let mut kernel = TruckKernel::new();
        for kind in Primitive3DKind::ALL {
            let params = Primitive3DParams::defaults(kind);
            let shape = S33DApp::build_insert_3d_shape(&mut kernel, &params)
                .unwrap_or_else(|error| panic!("{} failed: {error}", kind.name()));
            let mesh = kernel.tessellate(shape).unwrap();
            assert!(!mesh.is_empty(), "{} mesh is empty", kind.name());
            assert!(
                mesh.positions
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite()),
                "{} contains non-finite vertices",
                kind.name()
            );
            let (min, max) = mesh.bounds().unwrap();
            assert!(max.x > min.x && max.y > min.y && max.z > min.z);
        }
    }

    #[test]
    fn draw_3d_two_point_sizing_produces_stable_initial_dimensions() {
        let anchor = [10.0, 2.0, -4.0];
        let current = [13.0, 2.0, 0.0];

        let box_params = draw_3d_drag_params(
            Primitive3DParams::defaults(Primitive3DKind::Box),
            anchor,
            current,
            0.1,
        );
        assert_eq!(box_params.position, anchor);
        assert!((box_params.width - 6.0).abs() < 1e-12);
        assert!((box_params.depth - 8.0).abs() < 1e-12);
        assert!((box_params.height - 8.0).abs() < 1e-12);

        let cylinder = draw_3d_drag_params(
            Primitive3DParams::defaults(Primitive3DKind::Cylinder),
            anchor,
            current,
            0.1,
        );
        assert!((cylinder.radius - 5.0).abs() < 1e-12);
        assert!((cylinder.height - 10.0).abs() < 1e-12);

        let torus = draw_3d_drag_params(
            Primitive3DParams::defaults(Primitive3DKind::Torus),
            anchor,
            current,
            0.1,
        );
        assert!(torus.major_radius > torus.minor_radius);
    }

    #[test]
    fn repeated_sphere_edits_keep_center_and_commit_only_exact_shapes() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_insert_3d();
        for center in [[0.0, 0.0, 0.0], [40.0, 15.0, -5.0], [80.0, -10.0, 30.0]] {
            app.choose_insert_3d_primitive(Primitive3DKind::Sphere);
            app.insert_3d_ctx.as_mut().unwrap().stage = Draw3DPlacementStage::Precision;
            for radius in [1.0, 12.0, 12.5, 20.0, 10.0] {
                let mut p = app.insert_3d_ctx.as_ref().unwrap().params.unwrap();
                p.radius = radius;
                set_draw_3d_reference_point(&mut p, center);
                app.update_insert_3d_params(p);
                let ctx = app.insert_3d_ctx.as_ref().unwrap();
                assert_eq!(draw_3d_reference_point(&ctx.params.unwrap()), center);
                assert!(!ctx.parameters_applied);
                assert!(app.kernel.exact_shape(ctx.preview_shape.unwrap()).is_none());
                let mesh = app.kernel.tessellate(ctx.preview_shape.unwrap()).unwrap();
                let (min, max) = mesh.bounds().unwrap();
                assert!(
                    ((min + max) * 0.5 - glam::DVec3::from_array(center).as_vec3()).length()
                        < 0.001
                );
                assert!(!app.done_insert_3d());
            }
            assert!(app.apply_insert_3d_params());
            let exact = app.insert_3d_ctx.as_ref().unwrap().preview_shape.unwrap();
            assert_eq!(app.kernel.exact_shape(exact).unwrap().solid_count, 1);
            assert!(app.done_insert_3d());
            assert_eq!(app.body(app.selected[0]).unwrap().shape, Some(exact));
        }
        assert_eq!(app.doc.bodies.len(), 3);
        let mut p = Primitive3DParams::defaults(Primitive3DKind::Sphere);
        p.position = [2.0, 3.0, 4.0];
        p.radius = 10.0;
        let center = draw_3d_reference_point(&p);
        let resized = draw_3d_dragged_params(p, Draw3DDragKind::Radius, [5.0, 0.0], 0.0, 0.1);
        assert_eq!(draw_3d_reference_point(&resized), center);
        let placed = draw_3d_drag_params(p, [1.0, 2.0, 3.0], [4.0, 2.0, 7.0], 0.1);
        assert_eq!(draw_3d_reference_point(&placed), [1.0, 2.0, 3.0]);
        assert_eq!(placed.radius, 5.0);
    }

    #[test]
    fn draw_3d_precision_handles_cover_move_and_kind_specific_sizes() {
        let box_handles =
            draw_3d_control_points(&Primitive3DParams::defaults(Primitive3DKind::Box));
        for expected in [
            Draw3DDragKind::Move,
            Draw3DDragKind::Width,
            Draw3DDragKind::Depth,
            Draw3DDragKind::PlanarSize,
            Draw3DDragKind::Height,
        ] {
            assert!(box_handles.iter().any(|(kind, _)| *kind == expected));
        }

        let torus_handles =
            draw_3d_control_points(&Primitive3DParams::defaults(Primitive3DKind::Torus));
        assert!(torus_handles
            .iter()
            .any(|(kind, _)| *kind == Draw3DDragKind::MajorRadius));
        assert!(torus_handles
            .iter()
            .any(|(kind, _)| *kind == Draw3DDragKind::MinorRadius));
    }

    #[test]
    fn draw_3d_direct_drag_is_repeatable_and_commits_only_on_done() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0));
        app.start_insert_3d();
        app.choose_insert_3d_primitive(Primitive3DKind::Box);
        app.insert_3d_ctx.as_mut().unwrap().stage = Draw3DPlacementStage::Precision;
        app.update_insert_3d_params(Primitive3DParams::defaults(Primitive3DKind::Box));
        for _ in 0..2 {
            let params = app.insert_3d_ctx.as_ref().unwrap().params.unwrap();
            let point = draw_3d_control_points(&params)
                .into_iter()
                .find(|(k, _)| *k == Draw3DDragKind::Width)
                .unwrap()
                .1;
            let world = glam::Vec3::from_array(point.map(|v| v as f32));
            let start = app.world_to_viewport(world, app.viewport_rect).unwrap();
            let end = app
                .world_to_viewport(world + glam::Vec3::X * 8.0, app.viewport_rect)
                .unwrap();
            assert!(app.begin_draw_3d_direct_drag(start, app.viewport_rect));
            assert_eq!(
                app.insert_3d_ctx
                    .as_ref()
                    .unwrap()
                    .direct_drag
                    .unwrap()
                    .kind,
                Draw3DDragKind::Width
            );
            assert!(app.update_draw_3d_direct_drag(end, app.viewport_rect));
            app.on_pointer_up();
            let ctx = app.insert_3d_ctx.as_ref().unwrap();
            assert!(ctx.direct_drag.is_none());
            assert!(ctx.params.unwrap().width > params.width);
            assert!(app.doc.bodies.is_empty());
            assert!(!app.done_insert_3d(), "Apply is required after resizing");
        }
        assert!(app.apply_insert_3d_params());
        assert!(app.done_insert_3d());
        assert_eq!(app.doc.bodies.len(), 1);
        let id = app.selected[0];
        app.edit_draw_3d_body(id);
        let mut params = app.insert_3d_ctx.as_ref().unwrap().params.unwrap();
        params.position[0] += 3.0;
        app.update_insert_3d_params(params);
        assert!(app.apply_insert_3d_params());
        assert!(app.done_insert_3d());
        assert_eq!(app.doc.bodies.len(), 1);
        assert_eq!(app.selected, vec![id]);
    }

    #[test]
    fn freehand_continues_over_existing_geometry_without_selecting_or_moving_it() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.mode = Mode::Sketch;
        app.viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0));
        let plane = DatumPlane::Top.def();
        let first = SketchEntity::Rect {
            c1: [-10.0, -10.0],
            c2: [10.0, 10.0],
        };
        let mut sketch = SketchSession::new(plane.clone(), vec![first.clone()]);
        sketch.set_tool(SketchTool::Freehand);
        app.sketch = Some(sketch);
        for (index, point) in [[-10.0, 0.0], [10.0, 10.0], [0.0, 0.0]]
            .into_iter()
            .enumerate()
        {
            let world =
                glam::Vec3::from_array(plane.to_world(point[0], point[1]).map(|v| v as f32));
            let start = app.world_to_viewport(world, app.viewport_rect).unwrap();
            app.sketch_pointer_down_at(start, app.viewport_rect, index == 1, true);
            let end_world = glam::Vec3::from_array(
                plane
                    .to_world(point[0] + 8.0, point[1] + 12.0)
                    .map(|v| v as f32),
            );
            let end = app.world_to_viewport(end_world, app.viewport_rect).unwrap();
            app.on_pointer_move(end, app.viewport_rect);
            app.on_pointer_up();
            let sketch = app.sketch.as_ref().unwrap();
            assert_eq!(sketch.tool, SketchTool::Freehand);
            assert_eq!(sketch.entities.len(), index + 2);
            assert_eq!(sketch.entities[0], first);
            assert!(sketch.selected_entities().is_empty());
            assert!(sketch.active_drag_entity().is_none());
            assert!(!sketch.hovering_resize_handle());
        }
    }

    #[test]
    fn draw_3d_preview_has_strong_fill_and_outlines_for_all_primitives() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        for kind in Primitive3DKind::ALL {
            if app.mode != Mode::Insert3D {
                app.start_insert_3d();
            }
            app.choose_insert_3d_primitive(kind);
            app.update_insert_3d_params(Primitive3DParams::defaults(kind));
            let shape = app.insert_3d_ctx.as_ref().unwrap().preview_shape.unwrap();
            let mesh = app.kernel.tessellate(shape).unwrap();
            assert!(
                !draw_3d_preview_edges(&mesh, &app.camera).is_empty(),
                "{} outline",
                kind.name()
            );
            app.refresh_scene();
            let scene = app.scene.lock().unwrap();
            assert_eq!(scene.preview_surfaces.len(), 1);
            assert_eq!(scene.preview_surfaces[0].opacity, 0.88);
            assert!(scene
                .body_edge_lines
                .iter()
                .any(|lines| lines.color == [0.52, 0.98, 0.76]
                    && lines.positions.len()
                        == draw_3d_preview_edges(&mesh, &app.camera).len() * 2));
            assert!(
                app.doc.bodies.is_empty(),
                "preview must not commit geometry"
            );
        }
    }

    #[test]
    fn draw_3d_pointer_press_lifecycle_repeats_without_drag_threshold() {
        let egui_context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(egui_context.clone());
        let mut app = S33DApp::new(&creation);
        app.start_insert_3d();
        app.choose_insert_3d_primitive(Primitive3DKind::Box);
        app.insert_3d_ctx.as_mut().unwrap().stage = Draw3DPlacementStage::Precision;
        app.update_insert_3d_params(Primitive3DParams::defaults(Primitive3DKind::Box));
        let frame = |app: &mut S33DApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let _ = egui_context.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (_, response) =
                        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                    app.viewport_rect = response.rect;
                    if let Some(pos) = response.interact_pointer_pos() {
                        app.on_pointer_move(pos, response.rect);
                    }
                    if ctx.input(|i| i.pointer.primary_released()) {
                        app.on_pointer_up();
                    }
                    app.viewport_pointer(&response);
                });
            });
        };
        frame(&mut app, vec![]);
        for _ in 0..2 {
            let before = app.insert_3d_ctx.as_ref().unwrap().params.unwrap();
            let start = app
                .world_to_viewport(
                    glam::Vec3::from_array(before.position.map(|v| v as f32)),
                    app.viewport_rect,
                )
                .unwrap();
            frame(&mut app, vec![egui::Event::PointerMoved(start)]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
            );
            assert!(app.insert_3d_ctx.as_ref().unwrap().direct_drag.is_some());
            let end = start + egui::vec2(30.0, 20.0);
            frame(&mut app, vec![egui::Event::PointerMoved(end)]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            let after = app.insert_3d_ctx.as_ref().unwrap();
            assert!(after.direct_drag.is_none());
            assert_ne!(after.params.unwrap().position, before.position);
        }
    }

    #[test]
    fn draw_3d_preview_surface_can_start_a_move() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.viewport_rect =
            egui::Rect::from_min_max(egui::pos2(95.0, 51.0), egui::pos2(1021.0, 895.0));
        app.start_insert_3d();
        app.choose_insert_3d_primitive(Primitive3DKind::Box);
        app.insert_3d_ctx.as_mut().unwrap().stage = Draw3DPlacementStage::Precision;
        let mut params = Primitive3DParams::defaults(Primitive3DKind::Box);
        params.position = [62.0, 0.0, 35.0];
        params.width = 193.166;
        params.depth = 14.4;
        params.height = 110.0;
        app.update_insert_3d_params(params);
        let world = glam::vec3(80.0, 60.0, 42.2);
        let start = app.world_to_viewport(world, app.viewport_rect).unwrap();
        assert!(
            app.begin_draw_3d_direct_drag(start, app.viewport_rect),
            "surface {start:?}"
        );
        assert_eq!(
            app.insert_3d_ctx
                .as_ref()
                .unwrap()
                .direct_drag
                .unwrap()
                .kind,
            Draw3DDragKind::Move
        );
        assert!(app.update_draw_3d_direct_drag(start + egui::vec2(80.0, 30.0), app.viewport_rect));
        assert_ne!(
            app.insert_3d_ctx.as_ref().unwrap().params.unwrap().position,
            params.position
        );
    }

    #[test]
    fn revolved_draw_3d_handles_follow_the_profile_axis() {
        for kind in [
            Primitive3DKind::Cone,
            Primitive3DKind::Sphere,
            Primitive3DKind::Torus,
        ] {
            let mut params = Primitive3DParams::defaults(kind);
            params.frame = Some(PlaneDef::from_normal([1.0, 2.0, 3.0]));
            params.position = [5.0, 6.0, 7.0];
            let base = draw_3d_base_frame(&params);
            assert!((base.normal() - params.frame.unwrap().y_axis()).length() < 1e-10);
            assert_eq!(base.origin, params.position);
        }
    }

    #[test]
    fn text_placement_live_edit_and_group_extrusion() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
        let sk = app.sketch.as_mut().unwrap();
        sk.set_tool(SketchTool::Text);
        assert!(sk.pointer_down([2.0, 3.0], 1.0));
        assert!(sk.text_focus_requested);
        assert!(sk.entities[0].to_chains().is_empty());
        app.sketch_replace_text(0, "OB8", crate::sketch_text::FONTS[0])
            .unwrap();
        assert_eq!(app.sketch.as_ref().unwrap().entities.len(), 1);
        app.sketch_replace_text(0, "", crate::sketch_text::FONTS[0])
            .unwrap();
        app.sketch_replace_text(0, "OB8", crate::sketch_text::FONTS[0])
            .unwrap();
        app.start_extrude();
        assert!(app.sketch.is_none());
        assert_eq!(app.selected_regions.len(), 3);
        assert!(app.extrude_ctx.as_ref().unwrap().preview_error.is_none());
        app.commit_extrude();
        assert!(
            app.doc.bodies.iter().any(|b| b.shape.is_some()),
            "Text extrusion must commit a real solid, not just a preview"
        );
        assert!(app.undo());
        let id = app.doc.bodies.iter().find(|b| b.is_sketch()).unwrap().id;
        let regions = app.regions_of(id);
        let hit = regions.iter().position(|r| r.holes.len() == 2).unwrap();
        let group = app.text_region_group(id, hit).unwrap().unwrap();
        assert_eq!(group.len(), 3);
        app.select_region_group(id, group.clone(), false);
        assert_eq!(app.selected_regions.len(), 3);
        app.select_region_group(id, group.clone(), true);
        assert!(app.selected_regions.is_empty());
        app.select_region_group(id, group, true);
        app.start_extrude();
        assert!(app.extrude_ctx.as_ref().unwrap().preview_error.is_none());
    }

    #[test]
    fn text_commits_multiple_strokes_in_all_fonts() {
        for (content, font) in [
            ("你是猪猪", "Noto Sans SC"),
            ("中文测试", "Noto Sans SC"),
            ("S33D", "Noto Sans SC"),
            ("Hello world", "Noto Sans SC"),
            ("AB8", "Ubuntu Light"),
            ("AB8", "Hack"),
        ] {
            let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
            let mut app = S33DApp::new(&creation);
            app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
            let sk = app.sketch.as_mut().unwrap();
            sk.set_tool(SketchTool::Text);
            sk.text_content = content.into();
            sk.text_font = font.into();
            sk.pointer_down([-52.0, 18.0], 1.0);
            app.start_extrude();
            app.commit_extrude();
            assert!(
                app.doc.bodies.iter().any(|b| b.shape.is_some()),
                "{content}/{font}: {:?}",
                app.history
            );
        }
    }

    #[test]
    fn freehand_and_text_keep_individual_undo_steps_and_text_counters_after_extrude_undo() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_sketch(PlaneDef::from_normal([0.0, 0.0, 1.0]));
        let sk = app.sketch.as_mut().unwrap();
        sk.set_tool(SketchTool::Freehand);
        sk.begin_freehand([30.0, 30.0], 0.05);
        for p in [[40.0, 30.0], [40.0, 40.0], [30.0, 40.0], [30.0, 30.0]] {
            sk.pointer_move_with_snap(p, false, false, 1.0, 0.5);
        }
        sk.pointer_up([0.0; 2]);
        app.sync_sketch_history();
        let sk = app.sketch.as_mut().unwrap();
        sk.set_tool(SketchTool::Text);
        sk.text_content = "B".into();
        assert!(sk.pointer_down([0.0; 2], 1.0));
        app.sync_sketch_history();
        assert_eq!(app.sketch.as_ref().unwrap().entities.len(), 2);
        assert!(app.undo());
        assert_eq!(app.sketch.as_ref().unwrap().entities.len(), 1);
        assert!(app.redo());
        assert_eq!(app.sketch.as_ref().unwrap().entities.len(), 2);
        app.finish_sketch();
        let id = app.selected[0];
        let regions = app.regions_of(id);
        let index = regions.iter().position(|r| r.holes.len() == 2).unwrap();
        assert!(app.set_region_selection(app.selected[0], vec![index]));
        app.start_extrude();
        assert!(app.extrude_ctx.as_ref().unwrap().preview_error.is_none());
        app.commit_extrude();
        assert!(app.undo());
        assert_eq!(app.regions_of(id), regions);
    }

    #[test]
    fn draw_3d_precision_drag_updates_parameters_with_safe_limits() {
        let box_params = draw_3d_dragged_params(
            Primitive3DParams::defaults(Primitive3DKind::Box),
            Draw3DDragKind::PlanarSize,
            [4.0, -2.0],
            0.0,
            0.1,
        );
        assert!((box_params.width - 28.0).abs() < 1e-12);
        assert!((box_params.depth - 16.0).abs() < 1e-12);

        let cylinder = draw_3d_dragged_params(
            Primitive3DParams::defaults(Primitive3DKind::Cylinder),
            Draw3DDragKind::Height,
            [0.0, 0.0],
            -1_000.0,
            0.25,
        );
        assert!((cylinder.height - 0.25).abs() < 1e-12);

        let torus = draw_3d_dragged_params(
            Primitive3DParams::defaults(Primitive3DKind::Torus),
            Draw3DDragKind::MinorRadius,
            [1_000.0, 0.0],
            0.0,
            0.2,
        );
        assert!(torus.minor_radius < torus.major_radius);
    }

    #[test]
    fn draw_3d_recognizes_simple_extrude_box_and_cylinder() {
        let plane = DatumPlane::Right.def();
        for (entity, expected) in [
            (
                SketchEntity::Rect {
                    c1: [-3.0, -5.0],
                    c2: [3.0, 5.0],
                },
                Primitive3DKind::Box,
            ),
            (
                SketchEntity::RotatedRect {
                    c: [2.0, 4.0],
                    width: 6.0,
                    height: 10.0,
                    rot: 30.0_f64.to_radians(),
                },
                Primitive3DKind::Box,
            ),
            (
                SketchEntity::Circle {
                    c: [4.0, -2.0],
                    r: 3.5,
                },
                Primitive3DKind::Cylinder,
            ),
        ] {
            let feature = Feature::Extrude {
                sketch: SketchFeature {
                    vertex_locks: Default::default(),
                    plane,
                    entities: vec![entity],
                    world_xyz_coordinates: false,
                    on_body: None,
                },
                params: ExtrudeParams {
                    depth: 12.0,
                    symmetric: false,
                    bevel: 0.0,
                },
                source_sketch: None,
                pocket_source_sketches: Vec::new(),
                region_indices: vec![0],
            };
            let params =
                draw_3d_params_from_feature(&feature).expect("primitive should be recognized");
            assert_eq!(params.kind, expected);
            assert_eq!(params.height, 12.0);
            assert!(params.frame.is_some());
        }
    }

    #[test]
    fn draw_3d_recognizes_full_revolve_cylinder_cone_frustum_and_sphere() {
        let plane = DatumPlane::Front.def();
        let revolve = RevolveParams {
            axis_point: [0.0, 0.0],
            axis_dir: [0.0, 1.0],
            angle_deg: 360.0,
        };
        let cases = [
            (
                vec![SketchEntity::Rect {
                    c1: [0.0, 0.0],
                    c2: [4.0, 8.0],
                }],
                Primitive3DKind::Cylinder,
                4.0,
                4.0,
            ),
            (
                closed_line_entities(&[[0.0, 0.0], [5.0, 0.0], [0.0, 9.0]]),
                Primitive3DKind::Cone,
                5.0,
                0.0,
            ),
            (
                closed_line_entities(&[[0.0, 0.0], [5.0, 0.0], [2.0, 9.0], [0.0, 9.0]]),
                Primitive3DKind::Cone,
                5.0,
                2.0,
            ),
            (
                vec![
                    SketchEntity::Arc {
                        c: [0.0, 5.0],
                        r: 5.0,
                        a0: -std::f64::consts::FRAC_PI_2,
                        a1: std::f64::consts::FRAC_PI_2,
                        ccw: true,
                    },
                    SketchEntity::Line {
                        a: [0.0, 10.0],
                        b: [0.0, 0.0],
                    },
                ],
                Primitive3DKind::Sphere,
                5.0,
                0.0,
            ),
        ];

        for (entities, expected, bottom_radius, top_radius) in cases {
            let feature = Feature::Revolve {
                sketch: SketchFeature {
                    vertex_locks: Default::default(),
                    plane,
                    entities,
                    world_xyz_coordinates: false,
                    on_body: None,
                },
                params: revolve.clone(),
                source_sketch: None,
                region_index: Some(0),
            };
            let params = draw_3d_params_from_feature(&feature)
                .unwrap_or_else(|| panic!("{expected:?} should be recognized"));
            assert_eq!(params.kind, expected);
            if expected == Primitive3DKind::Sphere {
                assert!((params.radius - 5.0).abs() < 1e-5);
            } else {
                assert!((params.radius - bottom_radius).abs() < 1e-5);
                assert!((params.top_radius - top_radius).abs() < 1e-5);
            }
            let mut kernel = TruckKernel::new();
            let original_region = match &feature {
                Feature::Revolve { sketch, .. } => {
                    s33d_model::compute_regions(&sketch.entities).remove(0)
                }
                _ => unreachable!(),
            };
            let original = kernel
                .revolve_region(
                    &original_region,
                    &plane,
                    &Axis2D {
                        point: revolve.axis_point,
                        dir: revolve.axis_dir,
                    },
                    360.0,
                )
                .unwrap();
            let shape = S33DApp::build_insert_3d_shape(&mut kernel, &params).unwrap();
            let original_bounds = kernel.tessellate(original).unwrap().bounds().unwrap();
            let rebuilt_bounds = kernel.tessellate(shape).unwrap().bounds().unwrap();
            for axis in 0..3 {
                assert!(
                    (original_bounds.0[axis] - rebuilt_bounds.0[axis]).abs() < 1e-2,
                    "{expected:?} min axis {axis}: original={original_bounds:?} rebuilt={rebuilt_bounds:?}"
                );
                assert!(
                    (original_bounds.1[axis] - rebuilt_bounds.1[axis]).abs() < 1e-2,
                    "{expected:?} max axis {axis}: original={original_bounds:?} rebuilt={rebuilt_bounds:?}"
                );
            }
        }
    }

    #[test]
    fn align_moves_objects_to_locked_reference_without_moving_reference() {
        let reference_min = [10.0, 20.0, 30.0];
        let reference_max = [14.0, 28.0, 40.0];
        let object_min = [-2.0, 0.0, 4.0];
        let object_max = [2.0, 4.0, 8.0];

        assert_eq!(
            alignment_translation(
                &reference_min,
                &reference_max,
                &reference_min,
                &reference_max,
                1,
                AlignMode::Center,
            ),
            [0.0, 0.0, 0.0]
        );
        assert_eq!(
            alignment_translation(
                &reference_min,
                &reference_max,
                &object_min,
                &object_max,
                0,
                AlignMode::Min,
            ),
            [12.0, 0.0, 0.0]
        );
        assert_eq!(
            alignment_translation(
                &reference_min,
                &reference_max,
                &object_min,
                &object_max,
                1,
                AlignMode::Center,
            ),
            [0.0, 22.0, 0.0]
        );
        assert_eq!(
            alignment_translation(
                &reference_min,
                &reference_max,
                &object_min,
                &object_max,
                2,
                AlignMode::Max,
            ),
            [0.0, 0.0, 32.0]
        );
    }

    #[test]
    fn align_started_empty_locks_first_added_solid_and_keeps_it_while_collecting() {
        assert_eq!(align_reference_after_selection(None, &[], &[], None), None);
        assert_eq!(
            align_reference_after_selection(None, &[11], &[11], Some(11)),
            Some(11)
        );
        assert_eq!(
            align_reference_after_selection(Some(11), &[11, 22], &[11, 22], Some(22)),
            Some(11),
            "adding another object must not replace the locked reference"
        );
        assert_eq!(
            align_reference_after_selection(Some(11), &[22], &[22], Some(11)),
            Some(22),
            "removing the locked object should fall back to a remaining solid"
        );
    }

    #[test]
    fn third_chamfer_uses_selected_latest_result_instead_of_first() {
        let mut doc = Document::new("chain");
        doc.add_body(chamfer_body(2, 1, 1, false));
        doc.add_body(chamfer_body(3, 1, 2, true));

        // 第三次选边发生在第二个结果体上，必须继续使用 #3；旧实现会正序
        // 返回 #2，从而丢掉第二次（或更早的）fillet。
        assert_eq!(active_chamfer_body(&doc, &[3], 1), Some(3));
    }

    #[test]
    fn chamfer_chain_prefers_newest_visible_result_when_selection_is_original() {
        let mut doc = Document::new("chain");
        doc.add_body(chamfer_body(2, 1, 1, false));
        doc.add_body(chamfer_body(3, 1, 2, true));
        doc.add_body(chamfer_body(4, 1, 3, true)); // 模拟旧缺陷留下的重叠体

        assert_eq!(active_chamfer_body(&doc, &[1], 1), Some(4));
    }

    #[test]
    fn deleting_middle_chamfer_only_splices_that_operation_out() {
        let mut doc = Document::new("chain");
        doc.add_body(chamfer_body(2, 1, 1, false));
        doc.add_body(chamfer_body(3, 1, 2, false));
        doc.add_body(chamfer_body(4, 1, 3, true));

        let (target, replacement, order) = chamfer_delete_plan(&doc, 3).unwrap();
        assert_eq!(target, 1);
        assert_eq!(replacement, 2);
        assert_eq!(order, vec![(4, 2)]);
    }

    #[test]
    fn editing_middle_chamfer_keeps_it_as_base_and_rebuilds_later_operations() {
        let mut doc = Document::new("chain");
        doc.add_body(chamfer_body(2, 1, 1, false));
        doc.add_body(chamfer_body(3, 1, 2, false));
        doc.add_body(chamfer_body(4, 1, 3, true));

        let order = chamfer_descendant_plan(&doc, 1, 3, 3).unwrap();
        assert_eq!(order, vec![(4, 3)]);
        assert_eq!(latest_visible_chamfer_body(&doc, 1), Some(4));
    }

    #[test]
    fn deleting_first_chamfer_rebuilds_every_later_operation_in_order() {
        let mut doc = Document::new("chain");
        doc.add_body(chamfer_body(2, 1, 1, false));
        doc.add_body(chamfer_body(3, 1, 2, false));
        doc.add_body(chamfer_body(4, 1, 3, true));

        let (_, replacement, order) = chamfer_delete_plan(&doc, 2).unwrap();
        assert_eq!(replacement, 1);
        assert_eq!(order, vec![(3, 1), (4, 3)]);
    }

    #[test]
    fn translate_moves_along_camera_plane() {
        let m = gizmo_step_matrix_static(
            GizmoMode::Translate,
            egui::vec2(10.0, 5.0),
            glam::Vec3::ZERO,
            0.5, // 每像素 0.5mm
            glam::Vec3::X,
            glam::Vec3::Y,
        );
        // right*dx - up*dy = (10*0.5, -5*0.5, 0) = (5, -2.5, 0)
        let p = mat_apply(m, [0.0, 0.0, 0.0]);
        assert!((p[0] - 5.0).abs() < 1e-4, "p={p:?}");
        assert!((p[1] + 2.5).abs() < 1e-4, "p={p:?}");
        assert!(p[2].abs() < 1e-4);
    }

    #[test]
    fn rotate_spins_around_center() {
        let center = glam::Vec3::new(10.0, 0.0, 0.0);
        let m = gizmo_step_matrix_static(
            GizmoMode::Rotate,
            egui::vec2(-157.0796, 0.0), // ~π/2 弧度（-dx*0.01 = +1.5708）
            center,
            1.0,
            glam::Vec3::X,
            glam::Vec3::Y,
        );
        // 绕 Y 轴 +90°：体中心不动
        let c = mat_apply(m, [10.0, 0.0, 0.0]);
        assert!((c[0] - 10.0).abs() < 1e-3, "center moved: {c:?}");
        // 体上一点 (20,0,0) → 绕中心旋转 +90°（右手系，绕 Y）后变 (10,0,-10)
        let p = mat_apply(m, [20.0, 0.0, 0.0]);
        assert!((p[0] - 10.0).abs() < 1e-2, "p={p:?}");
        assert!((p[2] + 10.0).abs() < 1e-2, "p={p:?}");
    }

    #[test]
    fn rotate_around_picked_space_axis_keeps_axis_fixed() {
        let axis_point = glam::Vec3::new(1.0, 0.0, 0.0);
        let matrix = rotation_about_axis(axis_point, glam::Vec3::Z, 90.0_f32.to_radians());
        let fixed = mat_apply(matrix, [1.0, 0.0, 5.0]);
        assert!((fixed[0] - 1.0).abs() < 1e-4, "fixed={fixed:?}");
        assert!(fixed[1].abs() < 1e-4, "fixed={fixed:?}");
        let rotated = mat_apply(matrix, [2.0, 0.0, 0.0]);
        assert!((rotated[0] - 1.0).abs() < 1e-4, "rotated={rotated:?}");
        assert!((rotated[1] - 1.0).abs() < 1e-4, "rotated={rotated:?}");
    }

    #[test]
    fn rotation_axis_accepts_collinear_chain_and_rejects_curved_chain() {
        let segment = |a, b| EdgeSeg {
            a,
            b,
            n1: [0.0, 1.0, 0.0],
            n2: [0.0, 0.0, 1.0],
        };
        let straight = EdgeRef {
            body_id: 1,
            segments: vec![
                segment([0.0, 0.0, 0.0], [2.0, 0.0, 0.0]),
                segment([2.0, 0.0, 0.0], [5.0, 0.0, 0.0]),
            ],
        };
        let axis = straight_axis_from_edge(&straight).unwrap();
        assert!(
            glam::Vec3::from_array(axis.direction)
                .dot(glam::Vec3::X)
                .abs()
                > 0.999
        );
        assert_eq!(axis.display_start, [0.0, 0.0, 0.0]);
        assert_eq!(axis.display_end, [5.0, 0.0, 0.0]);

        let curved = EdgeRef {
            body_id: 1,
            segments: vec![
                segment([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
                segment([1.0, 0.0, 0.0], [1.0, 1.0, 0.0]),
            ],
        };
        assert!(straight_axis_from_edge(&curved).is_none());
    }

    #[test]
    fn picked_rotation_axis_preview_is_dashed_and_keeps_endpoints() {
        let mut lines = Vec::new();
        append_dashed_world_segment(
            &mut lines,
            glam::Vec3::ZERO,
            glam::Vec3::new(20.0, 0.0, 0.0),
        );
        assert!(lines.len() >= 6, "preview should contain several dashes");
        assert_eq!(lines.first().copied(), Some([0.0, 0.0, 0.0]));
        assert!(lines
            .chunks_exact(2)
            .all(|dash| dash[1][0] > dash[0][0] && dash[0][1] == 0.0 && dash[0][2] == 0.0));
        assert!(lines.windows(2).step_by(2).all(|gap| gap[1][0] > gap[0][0]));

        let mut wide_lines = Vec::new();
        append_dashed_world_segment_2px(
            &mut wide_lines,
            glam::Vec3::ZERO,
            glam::Vec3::new(20.0, 0.0, 0.0),
            glam::Vec3::Z,
            1.0,
        );
        assert_eq!(wide_lines.len(), lines.len() * 2);
        assert!(wide_lines.iter().any(|point| point[1] == -0.5));
        assert!(wide_lines.iter().any(|point| point[1] == 0.5));
    }

    #[test]
    fn selected_edge_width_builds_parallel_depth_tested_strokes() {
        let mut lines = Vec::new();
        append_solid_world_segment_width(
            &mut lines,
            glam::Vec3::ZERO,
            glam::Vec3::X * 10.0,
            glam::Vec3::Z,
            1.0,
            3,
        );
        assert_eq!(lines.len(), 6);
        let mut y_offsets: Vec<i32> = lines
            .chunks_exact(2)
            .map(|segment| segment[0][1].round() as i32)
            .collect();
        y_offsets.sort_unstable();
        assert_eq!(y_offsets, vec![-1, 0, 1]);
        assert!(lines.iter().all(|point| (point[2] + 0.2).abs() < 1e-6));
    }

    #[test]
    fn selected_surface_outline_omits_internal_triangle_edges() {
        let boundary = face_patch_boundary_edges(&[0, 1, 2, 0, 2, 3]);
        assert_eq!(boundary, vec![(0, 1), (0, 3), (1, 2), (2, 3)]);
        assert!(!boundary.contains(&(0, 2)));
    }

    #[test]
    fn triple_click_multi_select_uses_sequence_base_not_temporary_edge_owner() {
        let sequence_base = vec![1];

        // 第一击可能临时把选择显示成棱所属的 object 2；第三击必须先恢复基线。
        let mut selected = vec![2];
        selected.clone_from(&sequence_base);
        assert!(apply_workspace_owner_selection(&mut selected, 2, true));
        assert_eq!(selected, vec![1, 2]);

        // Ctrl/Shift 三击已选 object 时按 Objects 列表相同语义切换为未选。
        selected.clone_from(&sequence_base);
        assert!(!apply_workspace_owner_selection(&mut selected, 1, true));
        assert!(selected.is_empty());
    }

    #[test]
    fn union_multi_selection_matches_objects_and_workspace_toggle_semantics() {
        let mut selected = Vec::new();
        assert!(apply_workspace_owner_selection(&mut selected, 11, false));
        assert!(apply_workspace_owner_selection(&mut selected, 22, true));
        assert!(apply_workspace_owner_selection(&mut selected, 33, true));
        assert_eq!(selected, vec![11, 22, 33]);

        assert!(!apply_workspace_owner_selection(&mut selected, 22, true));
        assert_eq!(selected, vec![11, 33]);
    }

    #[test]
    fn global_select_all_is_available_before_multi_object_tools() {
        assert_eq!(
            select_all_policy(Mode::Idle, None, None, false, false),
            SelectAllPolicy::SelectVisibleSolids
        );
        assert_eq!(
            select_all_policy(Mode::Boolean, Some(BoolOp::Union), None, false, false),
            SelectAllPolicy::SelectVisibleSolids
        );
        for gizmo in [GizmoMode::Translate, GizmoMode::Rotate, GizmoMode::Scale] {
            assert_eq!(
                select_all_policy(Mode::Idle, None, Some(gizmo), false, false),
                SelectAllPolicy::SelectVisibleSolids
            );
        }
    }

    #[test]
    fn global_select_all_reports_remaining_single_object_and_ordered_operand_conflicts() {
        assert!(matches!(
            select_all_policy(Mode::Idle, None, None, true, false),
            SelectAllPolicy::ClearWithNotice(_)
        ));
        assert!(matches!(
            select_all_policy(Mode::Boolean, Some(BoolOp::Subtract), None, false, false),
            SelectAllPolicy::ClearWithNotice(_)
        ));
    }

    #[test]
    fn group_rotate_and_scale_use_one_shared_pivot() {
        let pivot = glam::Vec3::ZERO;
        let rotate = rotation_about_axis(pivot, glam::Vec3::Y, 90.0_f32.to_radians());
        let left = rotate.transform_point3(glam::Vec3::new(-10.0, 0.0, 0.0));
        let right = rotate.transform_point3(glam::Vec3::new(10.0, 0.0, 0.0));
        assert!((left.z - 10.0).abs() < 1e-4, "left={left:?}");
        assert!((right.z + 10.0).abs() < 1e-4, "right={right:?}");

        let scale = glam::Mat4::from_translation(pivot)
            * glam::Mat4::from_scale(glam::Vec3::splat(2.0))
            * glam::Mat4::from_translation(-pivot);
        let scaled_left = scale.transform_point3(glam::Vec3::new(-10.0, 0.0, 0.0));
        let scaled_right = scale.transform_point3(glam::Vec3::new(10.0, 0.0, 0.0));
        assert!((scaled_left.x + 20.0).abs() < 1e-4);
        assert!((scaled_right.x - 20.0).abs() < 1e-4);
    }

    #[test]
    fn boolean_operand_transfer_and_swap_only_apply_when_requested() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let a = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"box", "position":[0,0,0], "width":20,"depth":20,"height":20}),
        )["id"]
            .as_u64()
            .unwrap();
        let b = agent_call(
            &mut app,
            "primitive.create",
            json!({"kind":"box", "position":[8,0,0], "width":20,"depth":20,"height":20}),
        )["id"]
            .as_u64()
            .unwrap();
        app.selected = vec![a, b];
        app.start_boolean(BoolOp::Subtract);
        let count = app.doc.bodies.len();
        app.swap_boolean_operands();
        assert_eq!(app.selected, vec![b, a]);
        app.swap_boolean_operands();
        assert_eq!(app.selected, vec![a, b]);
        app.begin_boolean_operand_pick(BooleanOperandSlot::Target);
        app.boolean_click(b, false);
        let ctx = app.boolean_ctx.as_ref().unwrap();
        assert_eq!(
            (ctx.target, ctx.tool, ctx.picking),
            (Some(b), None, Some(BooleanOperandSlot::Tool))
        );
        assert_eq!(app.selected, vec![b]);
        app.apply_boolean_pair();
        assert_eq!(app.doc.bodies.len(), count);
        app.swap_boolean_operands();
        let ctx = app.boolean_ctx.as_ref().unwrap();
        assert_eq!(
            (ctx.target, ctx.tool, ctx.picking),
            (None, Some(b), Some(BooleanOperandSlot::Target))
        );
        app.boolean_click(a, false);
        app.begin_boolean_operand_pick(BooleanOperandSlot::Tool);
        app.boolean_click(a, false);
        let ctx = app.boolean_ctx.as_ref().unwrap();
        assert_eq!(
            (ctx.target, ctx.tool, ctx.picking),
            (None, Some(a), Some(BooleanOperandSlot::Target))
        );
        app.boolean_click(b, false);
        assert_eq!(app.doc.bodies.len(), count);
        assert!(app.doc.body(a).unwrap().visible && app.doc.body(b).unwrap().visible);
        app.boolean_ctx.as_mut().unwrap().keep_originals = true;
        app.apply_boolean_pair();
        assert_eq!(app.doc.bodies.len(), count + 1);
        assert!(matches!(&app.doc.bodies.last().unwrap().feature,
            Feature::Derived { operation: Some(DerivedOperation::Boolean { operands, .. }), .. }
                if operands == &[b,a]));
        assert!(app.doc.body(a).unwrap().visible && app.doc.body(b).unwrap().visible);
    }

    #[test]
    fn boolean_pair_waits_for_each_operand_then_requires_explicit_apply() {
        assert_eq!(
            next_boolean_operand_slot(None, None),
            Some(BooleanOperandSlot::Target)
        );
        assert_eq!(
            next_boolean_operand_slot(Some(11), None),
            Some(BooleanOperandSlot::Tool)
        );
        assert_eq!(next_boolean_operand_slot(Some(11), Some(22)), None);
    }

    #[test]
    fn through_pocket_crosses_target_from_a_bottom_sketch_plane() {
        let mut kernel = TruckKernel::new();
        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let target_region = Region2D::new(vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]);
        let target = kernel.extrude_region(&target_region, &plane, 17.0).unwrap();
        let target_mesh = kernel.tessellate(target).unwrap();

        // 草图面与目标底面重合。旧算法从 z=2 只向负向切，
        // 因此会留下 z=2..17 的实体；新算法必须从顶面之上切到底面之下。
        let (cutter_plane, cutter_depth) =
            through_pocket_cutter_spec(&plane, &target_mesh).unwrap();
        let start = cutter_plane.origin()[2];
        let end = start + cutter_depth;
        assert!(start > 17.0, "start={start}");
        assert!(end < 0.0, "end={end}");

        let pocket_region = Region2D::new(vec![[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]]);
        let cutter = kernel
            .extrude_region(&pocket_region, &cutter_plane, cutter_depth)
            .unwrap();
        let result = kernel.boolean_subtract(target, cutter).unwrap();
        let result_mesh = kernel.tessellate(result).unwrap();
        assert!(
            ray_mesh_intersect([0.0, 0.0, 30.0], [0.0, 0.0, -1.0], &result_mesh).is_none(),
            "a through pocket must not leave a cap or triangle across its center"
        );
        assert!(
            ray_mesh_intersect([4.0, 0.0, 30.0], [0.0, 0.0, -1.0], &result_mesh).is_some(),
            "material outside the pocket must remain"
        );
    }

    #[test]
    fn through_pocket_uses_real_vertex_projection_for_oblique_axes() {
        let mesh = MeshData::new(
            vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [10.0, 10.0, 0.0],
                [0.0, 10.0, 0.0],
            ],
            vec![0, 1, 2, 0, 2, 3],
        );
        // 该方向下 dot(AABB.max)-dot(AABB.min)=0，而真实顶点投影跨度为 10√2。
        let plane = PlaneDef::from_normal([1.0, -1.0, 0.0]);
        let (cutter_plane, cutter_depth) = through_pocket_cutter_spec(&plane, &mesh).unwrap();
        let normal = plane.normal();
        let start = (cutter_plane.origin() - plane.origin()).dot(normal);
        let end = start + cutter_depth;
        assert!(start > 2.0_f64.sqrt() * 5.0);
        assert!(end < -2.0_f64.sqrt() * 5.0);
    }

    #[test]
    fn regression_new_project_clears_tools_and_stale_surface_links() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let id = app.doc.alloc_id();
        let profile = SketchFeature {
            vertex_locks: Default::default(),
            plane: DatumPlane::Top.def(),
            entities: vec![SketchEntity::Rect {
                c1: [-5.0, -5.0],
                c2: [5.0, 5.0],
            }],
            on_body: Some(id),
            world_xyz_coordinates: false,
        };
        app.doc.add_body(Body {
            id,
            name: "Stale surface sketch".into(),
            kind: BodyKind::Sketch,
            feature: Feature::Sketch(profile),
            shape: None,
            visible: true,
            material: Material::default(),
        });
        app.selected = vec![id];
        app.start_extrude();
        app.commit_extrude();
        assert!(
            app.doc.bodies.iter().any(|b| b.shape.is_some()),
            "Stale source must not prevent standalone extrusion"
        );
        app.start_equation_surface(None);
        app.chamfer_active = true;
        app.sketch_on_body = Some(id);
        app.chamfer_base_shapes.insert(id, ShapeId(77));
        app.selected_edge = Some(EdgeRef {
            body_id: id,
            segments: vec![EdgeSeg {
                a: [0.0; 3],
                b: [1.0, 0.0, 0.0],
                n1: [0.0, 1.0, 0.0],
                n2: [0.0, 0.0, 1.0],
            }],
        });
        app.rotate_custom_axis = Some(RotationAxis {
            point: [0.0; 3],
            direction: [1.0, 0.0, 0.0],
            display_start: [0.0; 3],
            display_end: [1.0, 0.0, 0.0],
        });
        app.new_project();
        assert!(app.doc.bodies.is_empty() && app.history.is_empty());
        assert!(app.surface_ctx.is_none() && app.curve_ctx.is_none() && app.offset_ctx.is_none());
        assert!(app.sketch_on_body.is_none() && app.rotate_custom_axis.is_none());
        assert!(app.chamfer_base_shapes.is_empty() && !app.chamfer_active);
        assert!(app.selected_edge.is_none() && app.face_pick.is_none());
        assert!(!app.undo());
    }

    #[test]
    fn regression_draw_sphere_is_closed_and_boolean_ready() {
        let mut kernel = TruckKernel::new();
        let mut params = Primitive3DParams::defaults(Primitive3DKind::Sphere);
        params.radius = 10.0;
        let sphere = S33DApp::build_insert_3d_shape(&mut kernel, &params).unwrap();
        let mesh = kernel.tessellate(sphere).unwrap();
        let volume = |mesh: &MeshData| {
            mesh.indices
                .chunks_exact(3)
                .map(|t| {
                    let a = glam::DVec3::from_array(mesh.positions[t[0] as usize].map(f64::from));
                    let b = glam::DVec3::from_array(mesh.positions[t[1] as usize].map(f64::from));
                    let c = glam::DVec3::from_array(mesh.positions[t[2] as usize].map(f64::from));
                    a.dot(b.cross(c)) / 6.0
                })
                .sum::<f64>()
                .abs()
        };
        let expected = 4.0 / 3.0 * std::f64::consts::PI * 1000.0;
        assert!((volume(&mesh) / expected - 1.0).abs() < 0.03);
        let moved = kernel
            .transform(sphere, &glam::Mat4::from_translation(glam::Vec3::X * 8.0))
            .unwrap();
        let cut = kernel.boolean_subtract(sphere, moved).unwrap();
        let cut_volume = volume(&kernel.tessellate(cut).unwrap());
        assert!(cut_volume > 100.0 && cut_volume < expected);
        assert!(kernel.exact_shape(cut).is_some_and(|s| s.solid_count == 1));
        let joined = kernel.boolean_union(sphere, moved).unwrap();
        assert!(volume(&kernel.tessellate(joined).unwrap()) > expected);
    }

    #[test]
    fn regression_mesh_surface_extrudes_and_commits() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let params = Primitive3DParams::defaults(Primitive3DKind::Box);
        let original = S33DApp::build_insert_3d_shape(&mut app.kernel, &params).unwrap();
        let mesh = app.kernel.tessellate(original).unwrap();
        let shape = app.kernel.insert_mesh(mesh);
        let mesh = app.mesh_for(shape);
        let indices: Vec<u32> = mesh
            .indices
            .chunks_exact(3)
            .filter(|t| {
                t.iter()
                    .all(|i| (mesh.positions[*i as usize][1] - params.height as f32).abs() < 1e-4)
            })
            .flatten()
            .copied()
            .collect();
        assert!(!indices.is_empty());
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Imported STL".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "test.stl".into(),
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        app.selected = vec![id];
        app.face_pick = Some(FaceHit {
            body_id: id,
            normal: [0.0, 1.0, 0.0],
            center: [0.0, params.height as f32, 0.0],
            triangles: indices,
            area_mm2: params.width * params.depth,
        });
        app.start_extrude();
        assert_eq!(app.mode, Mode::Extrude);
        app.set_extrude_depth(3.0);
        app.commit_extrude();
        assert_eq!(app.mode, Mode::Idle);
        let mesh = app
            .kernel
            .tessellate(app.body(id).unwrap().shape.unwrap())
            .unwrap();
        assert!((mesh.bounds().unwrap().1.y as f64 - params.height - 3.0).abs() < 0.01);
    }

    #[test]
    fn exact_split_sphere_shell_rim_extrudes_commits_and_reopens() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let mut sphere = |radius: f64| {
            let mut params = Primitive3DParams::defaults(Primitive3DKind::Sphere);
            params.radius = radius;
            let shape = S33DApp::build_insert_3d_shape(&mut app.kernel, &params).unwrap();
            app.kernel
                .transform(
                    shape,
                    &glam::Mat4::from_translation(glam::Vec3::new(0.0, -radius as f32, 0.0)),
                )
                .unwrap()
        };
        let outer = sphere(95.0);
        let inner = sphere(90.0);
        let shell = app.kernel.boolean_subtract(outer, inner).unwrap();
        let halves = app
            .kernel
            .split_by_plane(shell, &PlaneDef::from_normal([0.0, 1.0, 0.0]))
            .unwrap();
        let half = halves
            .into_iter()
            .find(|s| app.kernel.tessellate(*s).unwrap().bounds().unwrap().0.y > -0.01)
            .unwrap();
        let mut cut_plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        cut_plane.origin = [0.0, 0.0, -15.0];
        let parts = app.kernel.split_by_plane(half, &cut_plane).unwrap();
        let base = parts
            .into_iter()
            .find(|s| app.kernel.tessellate(*s).unwrap().bounds().unwrap().1.z < -14.99)
            .unwrap();
        let display = app.mesh_for(base);
        let triangles: Vec<_> = display
            .indices
            .chunks_exact(3)
            .filter(|t| {
                t.iter()
                    .all(|&i| (display.positions[i as usize][2] + 15.0).abs() < 1e-4)
            })
            .flatten()
            .copied()
            .collect();
        assert!(!triangles.is_empty());
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Split sphere shell".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "fixture".into(),
            },
            shape: Some(base),
            material: Material::default(),
            visible: true,
        });
        app.selected = vec![id];
        app.face_pick = Some(FaceHit {
            body_id: id,
            normal: [0.0, 0.0, 1.0],
            center: [0.0, 58.0, -15.0],
            triangles,
            area_mm2: 1452.4,
        });
        let hit = app.face_pick.clone().unwrap();
        let face =
            exact_face_for_hit(app.kernel.exact_shape(base).unwrap(), &display, &hit).unwrap();
        // A fragment must never be silently promoted to the whole topology face.
        let mut fragment = hit.clone();
        fragment.triangles.truncate(3);
        assert!(
            exact_face_for_hit(app.kernel.exact_shape(base).unwrap(), &display, &fragment)
                .is_none()
        );
        app.start_extrude();
        assert_eq!(
            app.extrude_ctx.as_ref().unwrap().exact_face,
            Some((base, face))
        );
        app.set_extrude_depth(20.0);
        let ctx = app.extrude_ctx.as_ref().unwrap();
        assert!(ctx.preview_error.is_none(), "{:?}", ctx.preview_error);
        assert!(
            app.kernel.exact_shape(ctx.ghost_shapes[0]).is_none(),
            "Dragging uses display-only geometry; Done must still commit Exact"
        );
        app.commit_extrude();
        assert_eq!(
            app.body(id).unwrap().shape,
            Some(base),
            "Worker must not replace the source before validation"
        );
        wait_face_commit(&mut app);
        let result = app.body(id).unwrap().shape.unwrap();
        assert_ne!(result, base, "Surface extrusion did not commit");
        assert_eq!(app.kernel.exact_shape(result).unwrap().solid_count, 1);
        let bounds = app.kernel.tessellate(result).unwrap().bounds().unwrap();
        assert!((bounds.1.z - 5.0).abs() < 0.01);
        assert!(app.undo());
        assert_eq!(app.body(id).unwrap().shape, Some(base));
        assert!(app.redo());
        assert_eq!(app.body(id).unwrap().shape, Some(result));
        let payload = app.project_to_s33d().unwrap();
        let (doc, kernel, ..) = decode_s33d_project(&payload).unwrap();
        assert_eq!(
            kernel
                .exact_shape(doc.body(id).unwrap().shape.unwrap())
                .unwrap()
                .solid_count,
            1
        );
        // Same source face in symmetric mode: total depth, not half the depth.
        app.selected = vec![id];
        app.doc.body_mut(id).unwrap().shape = Some(base);
        app.face_pick = Some(hit);
        app.start_extrude();
        app.extrude_ctx.as_mut().unwrap().symmetric = true;
        app.set_extrude_depth(20.0);
        let ghost = app.extrude_ctx.as_ref().unwrap().ghost_shapes[0];
        let bounds = app.kernel.tessellate(ghost).unwrap().bounds().unwrap();
        assert!((bounds.0.z + 25.0).abs() < 0.01 && (bounds.1.z + 5.0).abs() < 0.01);
        app.commit_extrude();
        wait_face_commit(&mut app);
        let result = app.body(id).unwrap().shape.unwrap();
        assert_ne!(result, base);
        assert!((app.kernel.tessellate(result).unwrap().bounds().unwrap().1.z + 5.0).abs() < 0.01);
    }

    #[test]
    #[ignore = "Requires the user's copied helmet fixture; run explicitly"]
    fn regression_user_helmet_chamfer_fillet() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../debug/regression-2026-09-09/skate-helmet.s33d");
        app.load_s33d_project(&std::fs::read_to_string(path).unwrap())
            .unwrap();
        let base = app.body(38).unwrap().shape.unwrap();
        let mesh = app.kernel.tessellate(base).unwrap();
        let chains = mesh_sharp_edge_segments(&mesh);
        eprintln!(
            "helmet chains: {:?}",
            chains.iter().map(|c| c.len()).collect::<Vec<_>>()
        );
        let chain = chains.iter().find(|c| c.len() == 153).unwrap();
        for (chamfer, fillet) in [(Some(3.0), None), (None, Some(3.0))] {
            let (result, count) = app
                .evaluate_chamfer_feature(base, chain, chamfer, fillet)
                .unwrap();
            let out = app.kernel.tessellate(result).unwrap();
            assert!(!out.is_empty());
            eprintln!(
                "helmet processed {count} segments; {} triangles",
                out.triangle_count()
            );
        }
    }

    #[test]
    fn bounded_planar_surface_recovers_one_closed_extrude_profile() {
        let mesh = MeshData::new(
            vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [10.0, 5.0, 0.0],
                [0.0, 5.0, 0.0],
            ],
            vec![0, 1, 2, 0, 2, 3],
        );
        let face = FaceHit {
            body_id: 7,
            normal: [0.0, 0.0, 1.0],
            center: [5.0, 2.5, 0.0],
            triangles: mesh.indices.clone(),
            area_mm2: 50.0,
        };

        let (plane, region) = bounded_planar_face_profile(&mesh, &face).unwrap();
        assert_eq!(region.outer.len(), 4);
        assert!(region.holes.is_empty());
        assert!((region.signed_area() - 50.0).abs() < 1e-8);
        assert!(plane.normal().dot(glam::DVec3::Z) > 0.999);

        let sketch = sketch_from_surface_region(plane, &region, face.body_id);
        assert_eq!(sketch.on_body, Some(7));
        assert_eq!(sketch.entities.len(), 4);

        let mut kernel = TruckKernel::new();
        let extruded = kernel.extrude_region(&region, &sketch.plane, 8.0).unwrap();
        assert!(!kernel.tessellate(extruded).unwrap().is_empty());

        let min_u = region
            .outer
            .iter()
            .map(|point| point[0])
            .fold(f64::INFINITY, f64::min);
        let revolved = kernel
            .revolve_region(
                &region,
                &sketch.plane,
                &s33d_kernel::Axis2D {
                    point: [min_u, 0.0],
                    dir: [0.0, 1.0],
                },
                360.0,
            )
            .unwrap();
        assert!(!kernel.tessellate(revolved).unwrap().is_empty());
    }

    #[test]
    fn surface_extrude_extension_unions_with_original_solid() {
        let mut kernel = TruckKernel::new();
        let base_plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let base_region = Region2D::new(vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]);
        let base = kernel
            .extrude_region(&base_region, &base_plane, 5.0)
            .unwrap();

        let mut surface_plane = base_plane.clone();
        surface_plane.origin = [0.0, 0.0, 5.0];
        let extension_region =
            Region2D::new(vec![[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]]);
        let extension = kernel
            .extrude_region(&extension_region, &surface_plane, 3.0)
            .unwrap();
        let merged = kernel.boolean_union(base, extension).unwrap();
        let mesh = kernel.tessellate(merged).unwrap();
        let (min, max) = mesh.bounds().unwrap();
        assert!((min.z - 0.0).abs() < 1e-3, "min={min:?}");
        assert!((max.z - 8.0).abs() < 1e-3, "max={max:?}");
        assert!((min.x + 5.0).abs() < 1e-3 && (max.x - 5.0).abs() < 1e-3);
    }

    #[test]
    fn sketch_reference_edges_separate_visible_and_hidden_features() {
        let mut kernel = TruckKernel::new();
        let plane = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let region = Region2D::new(vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]);
        let solid = kernel.extrude_region(&region, &plane, 10.0).unwrap();
        let mesh = kernel.tessellate(solid).unwrap();
        let (visible, hidden) =
            sketch_reference_edges(&mesh, glam::Vec3::new(20.0, 20.0, 20.0), 22.0);
        assert!(!visible.is_empty(), "front-facing edges should be solid");
        assert!(!hidden.is_empty(), "back-facing edges should be dashed");
        assert_eq!(visible.len() % 2, 0);
        assert_eq!(hidden.len() % 2, 0);
    }

    #[test]
    fn array_copies_count_includes_original_object() {
        let linear = array_step_ranges([true, false, false], [3, 1, 1]);
        assert_eq!(linear, [2, 0, 0]);
        assert_eq!(array_total_objects(linear), 3);

        let grid = array_step_ranges([true, true, false], [3, 2, 1]);
        assert_eq!(grid, [2, 1, 0]);
        assert_eq!(array_total_objects(grid), 6);
    }

    #[test]
    fn scale_around_center_with_clamp() {
        let center = glam::Vec3::new(5.0, 5.0, 5.0);
        let m = gizmo_step_matrix_static(
            GizmoMode::Scale,
            egui::vec2(0.0, 200.0), // f = 1 + 200*0.005 = 2.0
            center,
            1.0,
            glam::Vec3::X,
            glam::Vec3::Y,
        );
        let c = mat_apply(m, [5.0, 5.0, 5.0]);
        assert!((c[0] - 5.0).abs() < 1e-4 && (c[1] - 5.0).abs() < 1e-4);
        let p = mat_apply(m, [15.0, 5.0, 5.0]);
        assert!((p[0] - 25.0).abs() < 1e-3, "p={p:?}");
        // 缩小方向 + clamp
        let m2 = gizmo_step_matrix_static(
            GizmoMode::Scale,
            egui::vec2(0.0, -100000.0), // clamp 到 0.05
            center,
            1.0,
            glam::Vec3::X,
            glam::Vec3::Y,
        );
        let p2 = mat_apply(m2, [15.0, 5.0, 5.0]);
        let d = ((p2[0] - 5.0).powi(2) + (p2[1] - 5.0).powi(2) + (p2[2] - 5.0).powi(2)).sqrt();
        assert!((d - 10.0 * 0.05).abs() < 1e-2, "d={d}");
    }

    #[test]
    fn uniform_percent_syncs_from_the_axis_the_user_changed() {
        assert_eq!(
            synchronized_scale_percent([100.0, 135.0, 80.0], 1, true),
            [135.0; 3]
        );
        assert_eq!(
            synchronized_scale_percent([100.0, 135.0, 80.0], 1, false),
            [100.0, 135.0, 80.0]
        );
    }

    #[test]
    fn uniform_size_preserves_current_selection_proportions() {
        assert_eq!(
            synchronized_scale_size([10.0, 60.0, 40.0], [10.0, 20.0, 40.0], 1, true),
            [30.0, 60.0, 120.0]
        );
        assert_eq!(
            synchronized_scale_size([10.0, 60.0, 40.0], [10.0, 20.0, 40.0], 1, false),
            [10.0, 60.0, 40.0]
        );
    }

    #[test]
    fn scale_to_size_computes_world_axis_factors_and_rejects_zero_axes() {
        let factors = scale_to_size_factors(
            glam::Vec3::new(10.0, 20.0, 40.0),
            glam::Vec3::new(25.0, 10.0, 80.0),
        )
        .unwrap();
        assert!((factors - glam::Vec3::new(2.5, 0.5, 2.0)).length() < 1e-6);
        assert!(scale_to_size_factors(
            glam::Vec3::new(10.0, 0.0, 40.0),
            glam::Vec3::new(25.0, 10.0, 80.0),
        )
        .is_err());
        assert!(scale_to_size_factors(
            glam::Vec3::new(10.0, 20.0, 40.0),
            glam::Vec3::new(25.0, -10.0, 80.0),
        )
        .is_err());
    }
}
fn build_extrude_shape(
    kernel: &mut TruckKernel,
    sketch: &SketchFeature,
    regions: &[Region2D],
    params: &ExtrudeParams,
) -> Result<ShapeId, String> {
    if regions.is_empty() {
        return Err("the source sketch has no selected closed region".to_string());
    }
    // Validate every cap before allocating any temporary solid. This also
    // protects Agent and dependent-rebuild paths that do not pass the picker.
    for (index, region) in regions.iter().enumerate() {
        s33d_model::triangulate_region(&region.outer, &region.holes)
            .map_err(|error| format!("Extrude region {index}: {error}"))?;
    }
    let depth = params.depth;
    let mut shapes = Vec::with_capacity(regions.len());
    for region in regions {
        match kernel.extrude_region(region, &sketch.plane, depth) {
            Ok(shape) => shapes.push(shape),
            Err(error) => {
                for shape in shapes {
                    kernel.delete(shape);
                }
                return Err(error.to_string());
            }
        }
    }
    let shape = merge_shapes_checked(kernel, shapes)?;
    if !params.symmetric {
        return Ok(shape);
    }

    let normal = sketch.plane.normal();
    let shift = glam::Mat4::from_translation(glam::Vec3::new(
        normal.x as f32 * (-params.depth / 2.0) as f32,
        normal.y as f32 * (-params.depth / 2.0) as f32,
        normal.z as f32 * (-params.depth / 2.0) as f32,
    ));
    match kernel.transform(shape, &shift) {
        Ok(transformed) => {
            kernel.delete(shape);
            Ok(transformed)
        }
        Err(error) => {
            kernel.delete(shape);
            Err(error.to_string())
        }
    }
}

/// Merge temporary shapes without silently dropping a failed region. All
/// consumed inputs are deleted because the returned immutable shape replaces
/// them and no document snapshot can reference these construction temporaries.
fn merge_shapes_checked(kernel: &mut TruckKernel, shapes: Vec<ShapeId>) -> Result<ShapeId, String> {
    let mut iter = shapes.into_iter();
    let first = iter
        .next()
        .ok_or_else(|| "no shape was generated".to_string())?;
    let mut acc = first;
    while let Some(s) = iter.next() {
        match kernel.union_profile_parts(acc, s) {
            Ok(union) => {
                kernel.delete(acc);
                kernel.delete(s);
                acc = union;
            }
            Err(error) => {
                kernel.delete(acc);
                kernel.delete(s);
                for remaining in iter {
                    kernel.delete(remaining);
                }
                return Err(error.to_string());
            }
        }
    }
    Ok(acc)
}

/// Extend a persistent base solid with each temporary extrusion separately.
/// Disconnected extensions cannot be unioned with one another, while each is a
/// valid extension of the shared base body. Inputs and intermediate results are
/// cleaned up on both success and failure; the persistent `base` is never
/// deleted because document undo snapshots may still reference it.
fn union_extensions_into_base(
    kernel: &mut TruckKernel,
    base: ShapeId,
    extensions: Vec<ShapeId>,
    symmetric_shift: Option<glam::Vec3>,
) -> Result<ShapeId, String> {
    if extensions.is_empty() {
        return Err("no selected region produced geometry".to_string());
    }
    let mut current = base;
    let mut current_is_temporary = false;
    let mut iter = extensions.into_iter();
    while let Some(raw_extension) = iter.next() {
        let extension = if let Some(shift) = symmetric_shift {
            let matrix = glam::Mat4::from_translation(shift);
            match kernel.transform(raw_extension, &matrix) {
                Ok(transformed) => {
                    kernel.delete(raw_extension);
                    transformed
                }
                Err(error) => {
                    kernel.delete(raw_extension);
                    if current_is_temporary {
                        kernel.delete(current);
                    }
                    for remaining in iter {
                        kernel.delete(remaining);
                    }
                    return Err(error.to_string());
                }
            }
        } else {
            raw_extension
        };
        match kernel.boolean_union(current, extension) {
            Ok(result) => {
                if current_is_temporary {
                    kernel.delete(current);
                }
                kernel.delete(extension);
                current = result;
                current_is_temporary = true;
            }
            Err(error) => {
                kernel.delete(extension);
                if current_is_temporary {
                    kernel.delete(current);
                }
                for remaining in iter {
                    kernel.delete(remaining);
                }
                return Err(error.to_string());
            }
        }
    }
    Ok(current)
}

/// 合并多个 shape（union 或取第一个——拉伸单区域时直接返回）。
fn merge_shapes(kernel: &mut TruckKernel, shapes: Vec<ShapeId>) -> Option<ShapeId> {
    merge_shapes_checked(kernel, shapes).ok()
}

/// 调色板（与 Super3D PALETTE 一致）。
pub fn palette_color(i: usize) -> [f32; 4] {
    const PALETTE: [[f32; 3]; 8] = [
        [0.60, 0.65, 0.72],
        [0.50, 0.62, 0.77],
        [0.77, 0.65, 0.50],
        [0.56, 0.75, 0.62],
        [0.71, 0.56, 0.77],
        [0.77, 0.56, 0.56],
        [0.50, 0.75, 0.77],
        [0.71, 0.71, 0.56],
    ];
    let c = PALETTE[i % PALETTE.len()];
    [c[0], c[1], c[2], 1.0]
}

/// 屏幕坐标 → NDC。
pub fn screen_to_ndc(pos: egui::Pos2, rect: egui::Rect) -> [f32; 2] {
    let x = ((pos.x - rect.left()) / rect.width().max(1.0)) * 2.0 - 1.0;
    let y = -(((pos.y - rect.top()) / rect.height().max(1.0)) * 2.0 - 1.0);
    [x as f32, y as f32]
}

/// 射线与平面求交。
pub fn ray_plane_intersect(ro: [f64; 3], rd: [f64; 3], plane: &PlaneDef) -> Option<[f64; 3]> {
    let n = plane.normal();
    let den = n.dot(glam::DVec3::from_array(rd));
    if den.abs() < 1e-12 {
        return None;
    }
    let rel = plane.origin() - glam::DVec3::from_array(ro);
    let t = rel.dot(n) / den;
    if t < 0.0 {
        return None;
    }
    Some((glam::DVec3::from_array(ro) + t * glam::DVec3::from_array(rd)).to_array())
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = glam::DVec3::from_array(a) - glam::DVec3::from_array(b);
    d.length()
}

/// 射线与网格求交（Möller–Trumbore），返回 (三角形索引, 最近距离)。
fn ray_mesh_tri(ro: [f64; 3], rd: [f64; 3], mesh: &MeshData) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (ti, t) in mesh.indices.chunks_exact(3).enumerate() {
        let a = glam::DVec3::from_array([
            mesh.positions[t[0] as usize][0] as f64,
            mesh.positions[t[0] as usize][1] as f64,
            mesh.positions[t[0] as usize][2] as f64,
        ]);
        let b = glam::DVec3::from_array([
            mesh.positions[t[1] as usize][0] as f64,
            mesh.positions[t[1] as usize][1] as f64,
            mesh.positions[t[1] as usize][2] as f64,
        ]);
        let c = glam::DVec3::from_array([
            mesh.positions[t[2] as usize][0] as f64,
            mesh.positions[t[2] as usize][1] as f64,
            mesh.positions[t[2] as usize][2] as f64,
        ]);
        let e1 = b - a;
        let e2 = c - a;
        let p = glam::DVec3::from_array(rd).cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-12 {
            continue;
        }
        let inv = 1.0 / det;
        let s = glam::DVec3::from_array(ro) - a;
        let u = s.dot(p) * inv;
        if u < 0.0 || u > 1.0 {
            continue;
        }
        let q = s.cross(e1);
        let v = glam::DVec3::from_array(rd).dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let d = e2.dot(q) * inv;
        if d > 1e-6 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((ti, d));
        }
    }
    best
}
/// 射线与网格求交（Möller–Trumbore），返回最近距离。
pub fn ray_mesh_intersect(ro: [f64; 3], rd: [f64; 3], mesh: &MeshData) -> Option<f64> {
    let mut best: Option<f64> = None;
    for t in mesh.indices.chunks_exact(3) {
        let a = glam::DVec3::from_array([
            mesh.positions[t[0] as usize][0] as f64,
            mesh.positions[t[0] as usize][1] as f64,
            mesh.positions[t[0] as usize][2] as f64,
        ]);
        let b = glam::DVec3::from_array([
            mesh.positions[t[1] as usize][0] as f64,
            mesh.positions[t[1] as usize][1] as f64,
            mesh.positions[t[1] as usize][2] as f64,
        ]);
        let c = glam::DVec3::from_array([
            mesh.positions[t[2] as usize][0] as f64,
            mesh.positions[t[2] as usize][1] as f64,
            mesh.positions[t[2] as usize][2] as f64,
        ]);
        let e1 = b - a;
        let e2 = c - a;
        let p = glam::DVec3::from_array(rd).cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-12 {
            continue;
        }
        let inv = 1.0 / det;
        let s = glam::DVec3::from_array(ro) - a;
        let u = s.dot(p) * inv;
        if u < 0.0 || u > 1.0 {
            continue;
        }
        let q = s.cross(e1);
        let v = glam::DVec3::from_array(rd).dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let d = e2.dot(q) * inv;
        if d > 1e-6 && best.map(|bd| d < bd).unwrap_or(true) {
            best = Some(d);
        }
    }
    best
}

/// 草图实体 → 世界空间线段（折线近似）。
pub fn sketch_lines(sk: &SketchFeature) -> Vec<[f32; 3]> {
    let mut lines = Vec::new();
    for (pts, closed) in sk.entities.iter().flat_map(|e| e.to_chains()) {
        append_chain_world(&mut lines, &pts, &sk.plane, closed, 0.3);
    }
    lines
}

fn section_surface_and_cap_indices(mesh: &MeshData, plane: &PlaneDef) -> (Vec<u32>, Vec<u32>) {
    let cap =
        s33d_kernel::mesh_bool::cap_triangle_indices(mesh, plane.origin, plane.normal().to_array());
    let cap_triangles: std::collections::HashSet<[u32; 3]> = cap
        .chunks_exact(3)
        .map(|triangle| {
            let mut key = [triangle[0], triangle[1], triangle[2]];
            key.sort_unstable();
            key
        })
        .collect();
    let surface = mesh
        .indices
        .chunks_exact(3)
        .filter(|triangle| {
            let mut key = [triangle[0], triangle[1], triangle[2]];
            key.sort_unstable();
            !cap_triangles.contains(&key)
        })
        .flat_map(|triangle| triangle.iter().copied())
        .collect();
    (surface, cap)
}

/// Preserve the object's hue on the cut face while making it visibly distinct.
/// This replaces the old near-black cap and contains no triangle-grid overlay.
fn section_cut_face_color(color: [f32; 3]) -> [f32; 3] {
    [color[0] * 0.72, color[1] * 0.72, color[2] * 0.72]
}

/// Split / Section View 切割平面可视化：半透明矩形 + 边框。
fn append_plane_quad(scene: &mut SceneData, plane: &PlaneDef, half_extent: f32) {
    let origin = glam::Vec3::new(
        plane.origin[0] as f32,
        plane.origin[1] as f32,
        plane.origin[2] as f32,
    );
    let x_axis = glam::Vec3::new(
        plane.x_axis[0] as f32,
        plane.x_axis[1] as f32,
        plane.x_axis[2] as f32,
    );
    let y_axis = glam::Vec3::new(
        plane.y_axis[0] as f32,
        plane.y_axis[1] as f32,
        plane.y_axis[2] as f32,
    );
    let corners = [
        origin + x_axis * half_extent + y_axis * half_extent,
        origin - x_axis * half_extent + y_axis * half_extent,
        origin - x_axis * half_extent - y_axis * half_extent,
        origin + x_axis * half_extent - y_axis * half_extent,
    ];
    let positions: Vec<[f32; 3]> = corners.iter().map(|p| p.to_array()).collect();
    let color = [0.38, 0.66, 0.98];
    scene.planes.push(ScenePlane {
        positions: positions.clone(),
        indices: vec![0, 1, 2, 0, 2, 3],
        color,
        opacity: 0.13,
    });
    let mut border = Vec::new();
    for k in 0..4 {
        border.push(corners[k].to_array());
        border.push(corners[(k + 1) % 4].to_array());
    }
    scene.plane_borders.push(SceneLines {
        positions: border,
        color,
    });
}

fn append_plane_border(scene: &mut SceneData, plane: &PlaneDef, half_extent: f32) {
    let origin = glam::Vec3::from_array(plane.origin.map(|value| value as f32));
    let x_axis = glam::Vec3::from_array(plane.x_axis.map(|value| value as f32));
    let y_axis = glam::Vec3::from_array(plane.y_axis.map(|value| value as f32));
    let corners = [
        origin + x_axis * half_extent + y_axis * half_extent,
        origin - x_axis * half_extent + y_axis * half_extent,
        origin - x_axis * half_extent - y_axis * half_extent,
        origin + x_axis * half_extent - y_axis * half_extent,
    ];
    let mut border = Vec::with_capacity(8);
    for index in 0..4 {
        border.push(corners[index].to_array());
        border.push(corners[(index + 1) % 4].to_array());
    }
    scene.plane_borders.push(SceneLines {
        positions: border,
        color: [0.38, 0.66, 0.98],
    });
}

fn append_chain_world(
    lines: &mut Vec<[f32; 3]>,
    pts: &[[f64; 2]],
    plane: &PlaneDef,
    closed: bool,
    offset: f64,
) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let off = plane.normal() * offset; // 沿法线偏移，避免与实体表面 z-fighting
    let cnt = if closed { n } else { n - 1 };
    for i in 0..cnt {
        let a = plane.to_world(pts[i][0], pts[i][1]);
        let b = plane.to_world(pts[(i + 1) % n][0], pts[(i + 1) % n][1]);
        lines.push([
            (a[0] + off.x) as f32,
            (a[1] + off.y) as f32,
            (a[2] + off.z) as f32,
        ]);
        lines.push([
            (b[0] + off.x) as f32,
            (b[1] + off.y) as f32,
            (b[2] + off.z) as f32,
        ]);
    }
}

/// 用小型八边环和十字心线模拟屏幕尺寸稳定的红色关键点。
fn append_sketch_point_marker(
    lines: &mut Vec<[f32; 3]>,
    center: glam::Vec3,
    x_axis: glam::Vec3,
    y_axis: glam::Vec3,
    radius: f32,
) {
    const SIDES: usize = 8;
    let point_at = |index: usize| {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        center + (x_axis * angle.cos() + y_axis * angle.sin()) * radius
    };
    for index in 0..SIDES {
        lines.push(point_at(index).to_array());
        lines.push(point_at((index + 1) % SIDES).to_array());
    }
    lines.push((center - x_axis * radius * 0.65).to_array());
    lines.push((center + x_axis * radius * 0.65).to_array());
    lines.push((center - y_axis * radius * 0.65).to_array());
    lines.push((center + y_axis * radius * 0.65).to_array());
}

/// 将本地折线转成连续相位的 4/3 世界单位虚线；跨折线段保持 dash phase，
/// 避免圆和圆弧因每个细分段重新起算而看起来像实线。
fn append_dashed_chain_world(
    lines: &mut Vec<[f32; 3]>,
    pts: &[[f64; 2]],
    plane: &PlaneDef,
    closed: bool,
    offset: f64,
) {
    const DASH: f64 = 4.0;
    const GAP: f64 = 3.0;
    const PERIOD: f64 = DASH + GAP;

    let n = pts.len();
    if n < 2 {
        return;
    }
    let off = plane.normal() * offset;
    let count = if closed { n } else { n - 1 };
    let mut phase = 0.0_f64;

    for i in 0..count {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx.hypot(dy);
        if length <= 1e-9 {
            continue;
        }

        let mut cursor = 0.0_f64;
        while cursor < length - 1e-9 {
            let phase_in_period = phase.rem_euclid(PERIOD);
            let drawing = phase_in_period < DASH;
            let run_left = if drawing {
                DASH - phase_in_period
            } else {
                PERIOD - phase_in_period
            };
            let run = run_left.min(length - cursor);
            if drawing && run > 1e-9 {
                let t0 = cursor / length;
                let t1 = (cursor + run) / length;
                let p0 = plane.to_world(a[0] + dx * t0, a[1] + dy * t0);
                let p1 = plane.to_world(a[0] + dx * t1, a[1] + dy * t1);
                lines.push([
                    (p0[0] + off.x) as f32,
                    (p0[1] + off.y) as f32,
                    (p0[2] + off.z) as f32,
                ]);
                lines.push([
                    (p1[0] + off.x) as f32,
                    (p1[1] + off.y) as f32,
                    (p1[2] + off.z) as f32,
                ]);
            }
            cursor += run;
            phase += run;
        }
    }
}

/// 将一条世界空间线段离散为醒目的虚线。短轴也至少保留多个 dash，
/// 以免拾取反馈退化成普通实线。
fn append_dashed_world_segment(lines: &mut Vec<[f32; 3]>, start: glam::Vec3, end: glam::Vec3) {
    let delta = end - start;
    let length = delta.length();
    if length <= 1e-5 {
        return;
    }
    let direction = delta / length;
    let dash = (length / 9.0).clamp(0.25, 4.0);
    let gap = dash * 0.65;
    let mut cursor = 0.0_f32;
    while cursor < length {
        let dash_end = (cursor + dash).min(length);
        lines.push((start + direction * cursor).to_array());
        lines.push((start + direction * dash_end).to_array());
        cursor += dash + gap;
    }
}

/// Visible sharp edges and silhouette of a Draw3D preview, without triangle diagonals.
fn draw_3d_preview_edges(mesh: &MeshData, camera: &Camera) -> Vec<(glam::Vec3, glam::Vec3)> {
    // Weld only this display copy: B-rep face seams must share adjacency for
    // crisp box edges and smooth-object silhouettes, without triangle wires.
    let mut mesh = mesh.clone();
    mesh.weld(1e-5);
    let mut edges: std::collections::BTreeMap<(u32, u32), Vec<glam::Vec3>> = Default::default();
    for t in mesh.indices.chunks_exact(3) {
        let a = glam::Vec3::from_array(mesh.positions[t[0] as usize]);
        let b = glam::Vec3::from_array(mesh.positions[t[1] as usize]);
        let c = glam::Vec3::from_array(mesh.positions[t[2] as usize]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.length_squared() < 0.5 {
            continue;
        }
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edges.entry((a.min(b), a.max(b))).or_default().push(normal);
        }
    }
    edges
        .into_iter()
        .filter_map(|((a, b), normals)| {
            let a = glam::Vec3::from_array(mesh.positions[a as usize]);
            let b = glam::Vec3::from_array(mesh.positions[b as usize]);
            let view = if camera.ortho {
                -camera.forward()
            } else {
                (camera.eye - (a + b) * 0.5).normalize_or_zero()
            };
            let front = normals.iter().any(|n| n.dot(view) > 0.0);
            let back = normals.iter().any(|n| n.dot(view) <= 0.0);
            let sharp = normals.iter().enumerate().any(|(i, n)| {
                normals
                    .iter()
                    .skip(i + 1)
                    .any(|m| n.dot(*m) < 25.0_f32.to_radians().cos())
            });
            (front && (back || sharp || normals.len() == 1)).then_some((a, b))
        })
        .collect()
}

/// 一个表面 patch 的边界是在 patch 三角形中仅出现一次的无向边。
fn face_patch_boundary_edges(triangle_indices: &[u32]) -> Vec<(u32, u32)> {
    use std::collections::HashMap;

    let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
    for triangle in triangle_indices.chunks_exact(3) {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let edge = if a < b { (a, b) } else { (b, a) };
            *counts.entry(edge).or_default() += 1;
        }
    }
    let mut boundary: Vec<_> = counts
        .into_iter()
        .filter_map(|(edge, count)| (count == 1).then_some(edge))
        .collect();
    boundary.sort_unstable();
    boundary
}

/// wgpu line-list 固定为 1 px；在屏幕横向复制平行线，得到稳定的选中线宽。
fn append_solid_world_segment_width(
    lines: &mut Vec<[f32; 3]>,
    start: glam::Vec3,
    end: glam::Vec3,
    view_direction: glam::Vec3,
    world_per_pixel: f32,
    width_pixels: usize,
) {
    if width_pixels == 0 {
        return;
    }
    let direction = (end - start).normalize_or_zero();
    if direction.length_squared() <= 1e-8 {
        return;
    }
    let view_direction = view_direction.normalize_or_zero();
    let mut screen_side = direction.cross(view_direction).normalize_or_zero();
    if screen_side.length_squared() <= 1e-8 {
        screen_side = direction.any_orthonormal_vector();
    }
    let pixel = world_per_pixel.max(1e-5);
    // 微小朝相机偏移消除与原棱的 z-fighting，仍保留深度遮挡。
    let depth_bias = -view_direction * pixel * 0.20;
    let center = (width_pixels as f32 - 1.0) * 0.5;
    for stripe in 0..width_pixels {
        let offset = screen_side * (stripe as f32 - center) * pixel + depth_bias;
        lines.push((start + offset).to_array());
        lines.push((end + offset).to_array());
    }
}

/// Sketch 参考实体的工程图式棱线：面向相机或构成轮廓的边为实线，
/// 两侧面都背向相机的边为虚线。开放网格的边界边也作为参考边保留。
fn sketch_reference_edges(
    mesh: &MeshData,
    camera_eye: glam::Vec3,
    min_angle_deg: f32,
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    use std::collections::HashMap;

    let cos_threshold = min_angle_deg.to_radians().cos();
    let mut edge_faces: HashMap<(u32, u32), Vec<glam::Vec3>> = HashMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
        let a = glam::Vec3::from_array(mesh.positions[triangle[0] as usize]);
        let b = glam::Vec3::from_array(mesh.positions[triangle[1] as usize]);
        let c = glam::Vec3::from_array(mesh.positions[triangle[2] as usize]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.length_squared() <= 1e-10 {
            continue;
        }
        for (left, right) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let edge = if triangle[left] < triangle[right] {
                (triangle[left], triangle[right])
            } else {
                (triangle[right], triangle[left])
            };
            edge_faces.entry(edge).or_default().push(normal);
        }
    }

    let mut visible = Vec::new();
    let mut hidden = Vec::new();
    for ((a_index, b_index), normals) in edge_faces {
        let feature_edge = normals.len() == 1
            || normals.iter().enumerate().any(|(i, left)| {
                normals
                    .iter()
                    .skip(i + 1)
                    .any(|right| left.dot(*right) < cos_threshold)
            });
        if !feature_edge {
            continue;
        }
        let a = glam::Vec3::from_array(mesh.positions[a_index as usize]);
        let b = glam::Vec3::from_array(mesh.positions[b_index as usize]);
        let midpoint = (a + b) * 0.5;
        let toward_camera = (camera_eye - midpoint).normalize_or_zero();
        let faces_camera = normals
            .iter()
            .any(|normal| normal.dot(toward_camera) > 0.01);
        if faces_camera {
            visible.extend_from_slice(&[a.to_array(), b.to_array()]);
        } else {
            append_dashed_world_segment(&mut hidden, a, b);
        }
    }
    (visible, hidden)
}

/// 以两个相邻、关于轴线对称的 1 px stroke 模拟稳定的 2 号线宽。
/// wgpu 的原生 line-list 固定为 1 px，因此在世界空间按当前相机像素尺度偏移。
fn append_dashed_world_segment_2px(
    lines: &mut Vec<[f32; 3]>,
    start: glam::Vec3,
    end: glam::Vec3,
    view_direction: glam::Vec3,
    world_per_pixel: f32,
) {
    let direction = (end - start).normalize_or_zero();
    if direction.length_squared() <= 1e-8 {
        return;
    }
    let mut screen_side = direction.cross(view_direction).normalize_or_zero();
    if screen_side.length_squared() <= 1e-8 {
        screen_side = direction.any_orthonormal_vector();
    }
    let half_pixel = world_per_pixel.max(1e-5) * 0.5;
    let offset = screen_side * half_pixel;
    append_dashed_world_segment(lines, start - offset, end - offset);
    append_dashed_world_segment(lines, start + offset, end + offset);
}

/// 追加本地链（不闭合）。
fn append_chain(lines: &mut Vec<[f32; 3]>, pts: &[[f64; 2]]) {
    for w in pts.windows(2) {
        lines.push([w[0][0] as f32, w[0][1] as f32, 0.0]);
        lines.push([w[1][0] as f32, w[1][1] as f32, 0.0]);
    }
}

/// 追加环（世界空间）。
fn append_ring(lines: &mut Vec<[f64; 3]>, ring: &[[f64; 2]], plane: &PlaneDef) {
    append_chain_world64(lines, ring, plane, true);
}

fn append_chain_world64(
    lines: &mut Vec<[f64; 3]>,
    points: &[[f64; 2]],
    plane: &PlaneDef,
    closed: bool,
) {
    if points.len() < 2 {
        return;
    }
    let count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for i in 0..count {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        lines.extend([plane.to_world(a[0], a[1]), plane.to_world(b[0], b[1])]);
    }
}

/// DVec3 -> Vec3（f32 视口空间）。
fn v3(d: glam::DVec3) -> glam::Vec3 {
    glam::Vec3::new(d.x as f32, d.y as f32, d.z as f32)
}

impl eframe::App for S33DApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.background_modeling = true;
        self.poll_model_job();
        self.poll_export_job();
        self.poll_step_import();
        self.poll_exact_face_commit();
        self.poll_offset_preview();
        self.poll_surface_preview();
        self.poll_tool_exit();
        self.process_agent_requests();
        let editor_was_active = self.mode != Mode::Idle;
        crate::ui::draw(self, ctx);
        if editor_was_active && self.mode == Mode::Idle {
            self.collapse_tool_menus();
        }
        self.project_lifetime_ui(ctx);
        ctx.request_repaint();
    }

    fn on_exit(&mut self) {
        self.agent_desktop_lifetime.close();
        self.cancel_export_job();
        save_ui_state(self.objects_panel_pos, &self.settings);
    }
}

impl S33DApp {
    // ---- sketch 操作封装（供 UI 调用，避免双重借用） ----

    pub fn sketch_on_dimension(&mut self, text: &str) -> bool {
        let ok = self
            .sketch
            .as_mut()
            .map(|sk| sk.on_dimension(text))
            .unwrap_or(false);
        if ok {
            self.bump_scene();
        }
        ok
    }

    pub fn sketch_selected_entity(&self) -> Option<usize> {
        self.sketch.as_ref().and_then(|sk| sk.selected_entity)
    }

    pub fn sketch_selected_entities(&self) -> Vec<usize> {
        self.sketch
            .as_ref()
            .map(|sketch| sketch.selected_entities())
            .unwrap_or_default()
    }

    /// Objects 树的子项与视口 Pick 共用同一草图元素选择状态。
    pub fn select_sketch_list_item(&mut self, index: usize, additive: bool, shift: bool) {
        if shift {
            if self
                .sketch
                .as_mut()
                .is_some_and(|s| s.select_list_range(index, additive))
            {
                self.bump_scene();
            }
        } else {
            if let Some(s) = self.sketch.as_mut() {
                s.list_selection_anchor = Some(index);
            }
            self.select_sketch_entity(index, additive);
        }
    }

    pub fn select_sketch_entity(&mut self, index: usize, additive: bool) {
        let selected = if let Some(sketch) = self.sketch.as_mut() {
            if sketch.tool == SketchTool::Align {
                sketch.toggle_align_entity(index);
                true
            } else if matches!(sketch.tool, SketchTool::Move | SketchTool::Rotate) {
                sketch.select_transform_entity(index, additive);
                true
            } else {
                sketch.select_entity(index, additive)
            }
        } else {
            false
        };
        if selected {
            self.set_hint("Sketch element selected; edit its parameters in Properties");
            self.bump_scene();
        }
    }

    pub fn activate_sketch_entity(&mut self, index: usize) {
        if self
            .sketch
            .as_mut()
            .is_some_and(|sketch| sketch.activate_entity(index))
        {
            self.bump_scene();
        }
    }

    pub fn toggle_sketch_entity_properties(&mut self, index: usize) {
        if self
            .sketch
            .as_mut()
            .is_some_and(|sketch| sketch.toggle_entity_expanded(index))
        {
            self.bump_scene();
        }
    }

    pub fn deselect_sketch_entity(&mut self, index: usize) {
        if self
            .sketch
            .as_mut()
            .is_some_and(|sketch| sketch.deselect_entity(index))
        {
            self.bump_scene();
        }
    }

    pub fn sketch_entity_kind(&self, idx: usize) -> Option<String> {
        self.sketch
            .as_ref()
            .and_then(|sk| sk.entities.get(idx))
            .map(|e| e.kind().to_string())
    }

    pub fn sketch_entity_params(
        &self,
        idx: usize,
    ) -> Option<Vec<(&'static str, String, f64, bool)>> {
        self.sketch.as_ref().and_then(|sk| sk.entity_params(idx))
    }

    pub fn sketch_apply_edit(
        &mut self,
        idx: usize,
        vals: &std::collections::HashMap<&str, f64>,
    ) -> Result<(), String> {
        let r = self
            .sketch
            .as_mut()
            .map(|sk| sk.apply_entity_edit(idx, vals));
        if matches!(r, Some(Ok(()))) {
            if !self.sync_active_sketch_edit() {
                self.bump_scene();
            }
        }
        r.unwrap_or(Ok(()))
    }

    pub fn sketch_replace_text(
        &mut self,
        idx: usize,
        content: &str,
        font: &str,
    ) -> Result<(), String> {
        let entity = self
            .sketch
            .as_ref()
            .and_then(|s| s.entities.get(idx))
            .cloned()
            .ok_or("Sketch entity no longer exists")?;
        let SketchEntity::Path {
            origin,
            scale,
            rot,
            text: Some(_),
            ..
        } = entity
        else {
            return Err("Not a text object".into());
        };
        let mut rebuilt = crate::sketch_text::editable_text(content, font, scale, origin)?;
        if let SketchEntity::Path { rot: angle, .. } = &mut rebuilt {
            *angle = rot;
        }
        self.sketch.as_mut().unwrap().entities[idx] = rebuilt;
        if !self.sync_active_sketch_edit() {
            self.bump_scene();
        }
        Ok(())
    }

    pub fn sketch_delete_selected(&mut self) -> bool {
        let r = self
            .sketch
            .as_mut()
            .map(|sk| sk.delete_selected())
            .unwrap_or(false);
        if r {
            if !self.sync_active_sketch_edit() {
                self.bump_scene();
            }
        }
        r
    }

    pub fn sketch_delete_entity(&mut self, index: usize) -> bool {
        let deleted = self
            .sketch
            .as_mut()
            .is_some_and(|sketch| sketch.delete_entity(index));
        if deleted {
            if !self.sync_active_sketch_edit() {
                self.bump_scene();
            }
        }
        deleted
    }

    pub fn sketch_apply_align(&mut self) {
        let result = self
            .sketch
            .as_mut()
            .map(|sketch| sketch.apply_align())
            .unwrap_or_else(|| Err("No active sketch".into()));
        match result {
            Ok(moved) => {
                self.record_history(format!("Sketch align: moved {moved} entities"));
                self.toast(format!("Aligned {moved} sketch entities"));
                if !self.sync_active_sketch_edit() {
                    self.bump_scene();
                }
            }
            Err(error) => self.toast(error),
        }
    }

    pub fn sketch_set_move_delta(&mut self, delta: [f64; 2]) {
        if let Some(sketch) = self.sketch.as_mut() {
            sketch.set_move_delta(delta);
            self.bump_scene();
        }
    }

    pub fn sketch_set_rotate_angle(&mut self, angle_deg: f64) {
        if let Some(sketch) = self.sketch.as_mut() {
            sketch.set_rotate_angle(angle_deg);
            self.bump_scene();
        }
    }

    pub fn sketch_set_rotate_custom_pivot_enabled(&mut self, enabled: bool) {
        if let Some(sketch) = self.sketch.as_mut() {
            sketch.set_rotate_custom_pivot_enabled(enabled);
            self.bump_scene();
        }
    }

    pub fn sketch_set_rotate_pivot(&mut self, pivot: [f64; 2]) {
        if let Some(sketch) = self.sketch.as_mut() {
            sketch.set_rotate_pivot(pivot);
            self.bump_scene();
        }
    }

    pub fn sketch_quick_move(&mut self, delta: [f64; 2], label: &str) {
        let result = self
            .sketch
            .as_mut()
            .map(|sketch| sketch.apply_quick_move(delta))
            .unwrap_or_else(|| Err("No active Sketch".into()));
        match result {
            Ok(count) => {
                self.record_history(format!("Sketch move {label}: {count} entities"));
                self.toast(format!("Moved {count} sketch entities {label}"));
                if !self.sync_active_sketch_edit() {
                    self.bump_scene();
                }
            }
            Err(error) => self.toast(error),
        }
    }

    pub fn sketch_quick_rotate(&mut self, angle_deg: f64) {
        let result = self
            .sketch
            .as_mut()
            .map(|sketch| sketch.apply_quick_rotate(angle_deg))
            .unwrap_or_else(|| Err("No active Sketch".into()));
        match result {
            Ok(count) => {
                let direction = if angle_deg >= 0.0 {
                    "clockwise"
                } else {
                    "counterclockwise"
                };
                self.record_history(format!(
                    "Sketch rotate {direction} {:.3}°: {count} entities",
                    angle_deg.abs()
                ));
                self.toast(format!(
                    "Rotated {count} sketch entities {direction} {:.0}°",
                    angle_deg.abs()
                ));
                if !self.sync_active_sketch_edit() {
                    self.bump_scene();
                }
            }
            Err(error) => self.toast(error),
        }
    }

    pub fn sketch_apply_transform(&mut self) {
        let result = self
            .sketch
            .as_mut()
            .map(|sketch| sketch.apply_transform())
            .unwrap_or_else(|| Err("No active Sketch".into()));
        match result {
            Ok(count) => {
                self.record_history(format!("Sketch transform: {count} entities"));
                self.toast(format!("Transformed {count} sketch entities"));
                if !self.sync_active_sketch_edit() {
                    self.bump_scene();
                }
            }
            Err(error) => self.toast(error),
        }
    }
}

/// 找距 p 最近的 line 实体，返回 (线上一点, 方向点)。
fn pick_sketch_line(
    entities: &[s33d_model::SketchEntity],
    p: [f64; 2],
) -> Option<([f64; 2], [f64; 2])> {
    let mut best: Option<(f64, [f64; 2], [f64; 2])> = None;
    for e in entities {
        if let s33d_model::SketchEntity::Line { a, b } = e {
            let d = dist_point_seg_f64(p, *a, *b);
            if best.map(|(bd, _, _)| d < bd).unwrap_or(true) {
                best = Some((d, *a, *b));
            }
        }
    }
    best.map(|(_, a, b)| (a, b))
}

fn dist_point_seg_f64(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let abx = b[0] - a[0];
    let aby = b[1] - a[1];
    let l2 = abx * abx + aby * aby;
    let mut t = if l2 > 1e-12 {
        ((p[0] - a[0]) * abx + (p[1] - a[1]) * aby) / l2
    } else {
        0.0
    };
    t = t.clamp(0.0, 1.0);
    let qx = a[0] + abx * t;
    let qy = a[1] + aby * t;
    ((p[0] - qx).powi(2) + (p[1] - qy).powi(2)).sqrt()
}
