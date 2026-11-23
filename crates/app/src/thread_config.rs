//! Thread editor preferences, compatible personal presets and print heuristics.
use super::{Params, Profile, Target};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PrintProcess {
    #[default]
    Fdm,
    Resin,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrintSettings {
    pub process: PrintProcess,
    pub nozzle: f64,
    pub fdm_layer: f64,
    pub resin_layer: f64,
}
impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            process: PrintProcess::Fdm,
            nozzle: 0.4,
            fdm_layer: 0.2,
            resin_layer: 0.05,
        }
    }
}
impl PrintSettings {
    pub fn layer(&self) -> f64 {
        match self.process {
            PrintProcess::Fdm => self.fdm_layer,
            PrintProcess::Resin => self.resin_layer,
        }
    }
    pub fn valid(&self) -> bool {
        self.nozzle.is_finite()
            && (0.1..=2.).contains(&self.nozzle)
            && self.fdm_layer.is_finite()
            && (0.02..=1.).contains(&self.fdm_layer)
            && self.resin_layer.is_finite()
            && (0.01..=0.3).contains(&self.resin_layer)
    }
    /// A starting point for calibration, not an ISO tolerance or printer guarantee.
    /// Nominal diameter and the requested axial range remain the user's choices.
    pub fn suggest(&self, current: &Params, _internal: bool) -> Result<Params, String> {
        if !self.valid() || !current.diameter.is_finite() || current.diameter <= 0. {
            return Err("Invalid diameter or print settings".into());
        }
        let resolution_pitch = match self.process {
            PrintProcess::Fdm => (self.nozzle * 4.).max(self.layer() * 6.),
            PrintProcess::Resin => (self.layer() * 8.).max(0.5),
        };
        let desired = (current.diameter / 7.).max(resolution_pitch);
        let pitch = [
            0.5, 0.7, 0.8, 1., 1.25, 1.5, 1.75, 2., 2.5, 3., 3.5, 4., 5., 6.,
        ]
        .into_iter()
        .find(|p| *p >= desired)
        .unwrap_or(desired.ceil());
        let allowance = match self.process {
            PrintProcess::Fdm => (self.nozzle * 0.25).max(self.layer() * 0.5),
            PrintProcess::Resin => (self.layer() * 1.5).max(0.05),
        }
        .min(pitch * 0.2);
        // Complementary trapezoids: crest and root flats are both P/4.
        // d*tan(angle/2) = P/4 makes screw and nut flanks coincide at
        // half-pitch phase before the independent radial fit offsets.
        // FDM uses 45-degree flanks to the build plane (90 included),
        // broad one-nozzle crests and >=6 layers per pitch. Resin retains
        // steeper 60-degree flanks but the same broad, truncated tips.
        let angle_deg: f64 = if self.process == PrintProcess::Fdm {
            90.
        } else {
            60.
        };
        Ok(Params {
            pitch,
            depth: pitch * 0.25 / (angle_deg.to_radians() * 0.5).tan(),
            angle_deg,
            profile: Profile::TruncatedV,
            flat_fraction: 0.25,
            clearance: 0.,
            radial_fit: allowance,
            taper: 0.,
            ..current.clone()
        })
    }
    pub fn warnings(&self, p: &Params, chinese: bool) -> Vec<String> {
        let mut out = Vec::new();
        let text = |en: &str, zh: &str| {
            if chinese {
                zh.to_owned()
            } else {
                en.to_owned()
            }
        };
        if !self.valid() {
            out.push(text(
                "Print settings are outside the supported range",
                "打印设置超出支持范围",
            ));
            return out;
        }
        if self.process == PrintProcess::Fdm && self.layer() > self.nozzle * 0.8 {
            out.push(text(
                "Layer height exceeds 80% of the nozzle diameter",
                "层高超过喷嘴直径的 80%",
            ));
        }
        if p.pitch < self.layer() * 4. {
            out.push(text(
                "Fewer than four layers per pitch; choose a coarser pitch or lower layers",
                "每个螺距不足四层，请增大螺距或降低层高",
            ));
        }
        if self.process == PrintProcess::Fdm && p.depth < self.nozzle {
            out.push(text(
                "Tooth depth is smaller than the nozzle; fine flanks may be lost",
                "牙深小于喷嘴直径，细牙侧面可能无法成形",
            ));
        }
        if p.radial_fit <= 0. {
            out.push(text(
                "No radial mating allowance is set",
                "尚未设置螺钉与螺帽的径向配合余量",
            ));
        }
        if self.process == PrintProcess::Fdm && p.pitch * p.flat_fraction < self.nozzle - 1e-6 {
            out.push(text(
                "Root flat is narrower than the nozzle; use a coarser pitch or broader flats",
                "牙底平段小于喷嘴直径，请增大螺距或平段宽度",
            ));
        }
        if p.depth <= 2. * p.radial_fit {
            out.push(text("Mating allowance consumes the full tooth engagement; reduce allowance or increase depth", "配合余量已耗尽牙齿啮合深度，请减小余量或增大牙深"));
        }
        let suggested_angle = if self.process == PrintProcess::Fdm {
            90.
        } else {
            60.
        };
        if p.taper != 0.
            || p.profile != Profile::TruncatedV
            || (p.angle_deg - suggested_angle).abs() > 1e-6
        {
            out.push(text(
                "This profile differs from the print suggestion; calibrate the mating pair separately",
                "当前牙型与此工艺的打印建议不同，请单独试打校准配合",
            ));
        }
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPreset {
    pub name: String,
    pub internal: bool,
    pub params: Params,
    pub printing: PrintSettings,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetFile {
    version: u32,
    presets: Vec<SavedPreset>,
}

pub fn preset_path() -> PathBuf {
    crate::agent_protocol::user_ui_state_path().with_file_name("thread_presets.json")
}
pub fn load(path: &Path) -> Result<Vec<SavedPreset>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let file: PresetFile = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if file.version != 1 {
        return Err("Unsupported thread preset file version".into());
    }
    Ok(file.presets)
}
pub fn save(path: &Path, presets: &[SavedPreset]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&PresetFile {
        version: 1,
        presets: presets.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, path).map_err(|e| e.to_string())
}

/// Fit uses the same blank relation and validation as the actual cut. Axial
/// values are retained for built-ins; saved presets retain their full settings.
pub fn compatible(t: &Target, p: &Params) -> bool {
    super::validate(t, p).is_ok()
}

#[derive(Default)]
pub struct UiState {
    pub target_key: Option<String>,
    pub expanded: bool,
    pub selected_preset: Option<String>,
    pub baseline: Option<Params>,
    pub printing: PrintSettings,
    pub print_open: bool,
    pub saved: Vec<SavedPreset>,
    pub save_open: bool,
    pub save_name: String,
    pub error: Option<String>,
    pub load_error: Option<String>,
}
impl UiState {
    pub fn load_user() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        match load(&preset_path()) {
            Ok(saved) => Self {
                saved,
                ..Self::default()
            },
            Err(e) => Self {
                load_error: Some(e),
                ..Self::default()
            },
        }
    }
    pub fn mark_edited(&mut self, p: &Params) {
        if self.baseline.as_ref().is_some_and(|before| before != p) {
            self.selected_preset = None;
            self.baseline = None;
        }
    }
    pub fn remember(&mut self, path: &Path, internal: bool, params: &Params) -> Result<(), String> {
        if self.load_error.is_some() {
            return Err("Existing preset file could not be read; it was preserved".into());
        }
        let name = self.save_name.trim();
        if name.is_empty()
            || name.chars().count() > 80
            || name.eq_ignore_ascii_case("custom")
            || super::presets()
                .iter()
                .any(|p| p.0.eq_ignore_ascii_case(name))
        {
            return Err(
                "Choose a unique preset name (1–80 characters; not Custom or a built-in name)"
                    .into(),
            );
        }
        if self.saved.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
            return Err("A preset with this name already exists".into());
        }
        if !self.printing.valid() {
            return Err("Invalid print settings".into());
        }
        let mut next = self.saved.clone();
        next.push(SavedPreset {
            name: name.into(),
            internal,
            params: params.clone(),
            printing: self.printing.clone(),
        });
        save(path, &next)?;
        self.saved = next;
        self.selected_preset = Some(name.to_owned());
        self.baseline = Some(params.clone());
        self.save_open = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn printing_defaults_and_process_keep_nominal_and_axial_choices() {
        let settings = PrintSettings::default();
        assert_eq!((settings.nozzle, settings.layer()), (0.4, 0.2));
        let current = Params {
            length: 4.,
            offset: 2.,
            left_hand: true,
            ..Params::default()
        };
        let male = settings.suggest(&current, false).unwrap();
        let female = settings.suggest(&current, true).unwrap();
        assert_eq!(
            (male.diameter, male.length, male.offset, male.left_hand),
            (6., 4., 2., true)
        );
        assert_eq!(
            (male.pitch, male.radial_fit),
            (female.pitch, female.radial_fit)
        );
        assert_eq!(
            (male.depth, male.flat_fraction, male.angle_deg),
            (female.depth, female.flat_fraction, female.angle_deg)
        );
        assert!(male.pitch * male.flat_fraction >= settings.nozzle);
        assert!(male.depth >= settings.nozzle);
        assert!(male.pitch >= settings.layer() * 6.);
        assert_eq!(male.angle_deg, 90.);
        let resin = PrintSettings {
            process: PrintProcess::Resin,
            ..settings.clone()
        };
        assert!(resin.suggest(&current, false).unwrap().radial_fit < male.radial_fit);
        let thick = PrintSettings {
            fdm_layer: 0.4,
            nozzle: 0.4,
            ..settings
        };
        assert!(!thick.warnings(&male, false).is_empty());
    }
    #[test]
    fn edit_changes_preset_to_custom_and_disk_round_trip_preserves_every_setting() {
        let path =
            std::env::temp_dir().join(format!("s33d-thread-presets-{}.json", std::process::id()));
        let params = PrintSettings::default()
            .suggest(&Params::default(), false)
            .unwrap();
        let mut state = UiState {
            save_name: "我的 FDM M6".into(),
            ..Default::default()
        };
        state.remember(&path, false, &params).unwrap();
        let restored = load(&path).unwrap();
        assert_eq!(restored[0].params, params);
        assert_eq!(restored[0].printing, state.printing);
        state.mark_edited(&params);
        assert!(state.selected_preset.is_some());
        for changed in [
            Params {
                pitch: 1.5,
                ..params.clone()
            },
            Params {
                radial_fit: 0.15,
                ..params.clone()
            },
            Params {
                left_hand: true,
                ..params.clone()
            },
        ] {
            state.selected_preset = Some("test".into());
            state.baseline = Some(params.clone());
            state.mark_edited(&changed);
            assert!(state.selected_preset.is_none());
        }
        assert!(state.remember(&path, false, &params).is_err());
        assert_eq!(load(&path).unwrap().len(), 1);
        state.save_name = "Resin M6".into();
        state.printing.process = PrintProcess::Resin;
        state.remember(&path, false, &params).unwrap();
        let updated = load(&path).unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[1].printing.process, PrintProcess::Resin);
        assert_eq!(updated[1].params, params);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn filtering_rejects_unrelated_sizes_short_walls_and_wrong_bores() {
        let mut t = Target {
            body_id: 1,
            face_id: 0,
            origin: [0.; 3],
            axis: [0., 1., 0.],
            radius: 3.,
            length: 10.,
            internal: false,
        };
        assert!(compatible(
            &t,
            &super::super::preset_params("M6", false).unwrap()
        ));
        assert!(!compatible(
            &t,
            &super::super::preset_params("M8", false).unwrap()
        ));
        t.length = 0.5;
        assert!(!compatible(
            &t,
            &super::super::preset_params("M6", false).unwrap()
        ));
        t.length = 10.;
        t.internal = true;
        assert!(!compatible(
            &t,
            &super::super::preset_params("M6", true).unwrap()
        ));
        assert!(!super::super::presets()
            .iter()
            .any(|p| p.0.starts_with("Print")));
    }

    #[test]
    fn legacy_params_default_to_zero_profile_shift_and_invalid_fit_is_rejected() {
        let mut encoded = serde_json::to_value(Params::default()).unwrap();
        encoded.as_object_mut().unwrap().remove("radial_fit");
        let old: Params = serde_json::from_value(encoded).unwrap();
        assert_eq!(old.radial_fit, 0.);
        let t = Target {
            body_id: 1,
            face_id: 0,
            origin: [0.; 3],
            axis: [0., 1., 0.],
            radius: 3.,
            length: 10.,
            internal: false,
        };
        for fit in [f64::NAN, -0.01, 0.3] {
            assert!(super::super::validate(
                &t,
                &Params {
                    radial_fit: fit,
                    ..old.clone()
                }
            )
            .is_err());
        }
    }
}
