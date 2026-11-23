//! Optional visual treatments for S33D's existing compact layout.
//!
//! `Original` deliberately leaves a supplied style untouched. Appearance and
//! the original Blue/Teal/Orange preference are separate settings, so choosing
//! a new treatment never overwrites the user's original accent preference.

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke};
use serde::{Deserialize, Serialize};

/// Stable, language-independent values persisted in the settings file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAppearance {
    Original,
    PrecisionClassic,
    #[default]
    GraphiteStudio,
    PorcelainWorkshop,
}

impl UiAppearance {
    pub const ALL: [Self; 4] = [
        Self::Original,
        Self::PrecisionClassic,
        Self::GraphiteStudio,
        Self::PorcelainWorkshop,
    ];

    pub const fn label_en(self) -> &'static str {
        match self {
            Self::Original => "Simple",
            Self::PrecisionClassic => "Precision Classic",
            Self::GraphiteStudio => "Graphite Studio",
            Self::PorcelainWorkshop => "Porcelain Workshop",
        }
    }

    pub const fn label_zh(self) -> &'static str {
        match self {
            Self::Original => "简洁",
            Self::PrecisionClassic => "精密经典",
            Self::GraphiteStudio => "石墨工作室",
            Self::PorcelainWorkshop => "瓷白工坊",
        }
    }

    pub const fn is_original(self) -> bool {
        matches!(self, Self::Original)
    }

    pub const fn icons_enabled(self) -> bool {
        !self.is_original()
    }

    pub const fn is_dark(self) -> bool {
        matches!(self, Self::GraphiteStudio)
    }

    /// Use these same colors for explicit egui frames/text and scene lines.
    /// The Original values mirror the legacy constants; its configurable
    /// accent remains owned by the existing color-theme setting.
    pub const fn palette(self) -> UiPalette {
        match self {
            Self::Original => UiPalette {
                text: Color32::from_rgb(36, 43, 52),
                muted: Color32::from_rgb(105, 114, 126),
                panel: Color32::from_rgb(249, 250, 252),
                raised: Color32::WHITE,
                border: Color32::from_rgb(211, 217, 225),
                accent: Color32::from_rgb(37, 118, 221),
                accent_soft: Color32::from_rgb(226, 238, 253),
                on_accent: Color32::WHITE,
                viewport: Color32::from_rgb(38, 40, 45),
                grid_minor: Color32::from_rgb(43, 46, 51),
                grid_major: Color32::from_rgb(64, 66, 77),
                sketch_line: Color32::from_rgb(191, 227, 255),
                outline: Color32::from_rgb(255, 174, 55),
                input: Color32::WHITE,
                button: Color32::from_rgb(245, 247, 250),
            },
            Self::PrecisionClassic => UiPalette {
                text: Color32::from_rgb(37, 44, 49),
                muted: Color32::from_rgb(93, 103, 111),
                panel: Color32::from_rgb(245, 246, 245),
                raised: Color32::from_rgb(253, 253, 251),
                border: Color32::from_rgb(207, 214, 214),
                accent: Color32::from_rgb(10, 110, 106),
                accent_soft: Color32::from_rgb(222, 239, 235),
                on_accent: Color32::WHITE,
                viewport: Color32::from_rgb(38, 41, 46),
                grid_minor: Color32::from_rgb(46, 50, 55),
                grid_major: Color32::from_rgb(62, 69, 75),
                sketch_line: Color32::from_rgb(165, 220, 233),
                outline: Color32::from_rgb(255, 180, 73),
                input: Color32::from_rgb(255, 255, 253),
                button: Color32::from_rgb(235, 239, 238),
            },
            Self::GraphiteStudio => UiPalette {
                text: Color32::from_rgb(235, 239, 244),
                muted: Color32::from_rgb(169, 180, 193),
                panel: Color32::from_rgb(31, 36, 43),
                raised: Color32::from_rgb(39, 45, 53),
                border: Color32::from_rgb(67, 77, 88),
                accent: Color32::from_rgb(247, 165, 78),
                accent_soft: Color32::from_rgb(77, 52, 34),
                on_accent: Color32::from_rgb(30, 28, 26),
                viewport: Color32::from_rgb(42, 47, 54),
                grid_minor: Color32::from_rgb(49, 55, 63),
                grid_major: Color32::from_rgb(66, 74, 84),
                sketch_line: Color32::from_rgb(175, 219, 240),
                outline: Color32::from_rgb(255, 181, 88),
                input: Color32::from_rgb(23, 28, 34),
                button: Color32::from_rgb(47, 55, 65),
            },
            Self::PorcelainWorkshop => UiPalette {
                text: Color32::from_rgb(35, 46, 62),
                muted: Color32::from_rgb(91, 105, 122),
                panel: Color32::from_rgb(247, 249, 252),
                raised: Color32::WHITE,
                border: Color32::from_rgb(207, 216, 228),
                accent: Color32::from_rgb(56, 83, 164),
                accent_soft: Color32::from_rgb(228, 235, 251),
                on_accent: Color32::WHITE,
                viewport: Color32::from_rgb(231, 234, 238),
                grid_minor: Color32::from_rgb(222, 227, 233),
                grid_major: Color32::from_rgb(201, 210, 221),
                sketch_line: Color32::from_rgb(52, 81, 116),
                outline: Color32::from_rgb(167, 80, 13),
                input: Color32::WHITE,
                button: Color32::from_rgb(235, 240, 247),
            },
        }
    }

    /// Replace only visuals; retain the user's fonts, sizes and compact layout.
    /// Call after the legacy style has been constructed so Original is exact.
    pub fn configure_visuals(self, style: &mut egui::Style) {
        if self.is_original() {
            return;
        }
        let p = self.palette();
        let mut visuals = if self.is_dark() {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        // Let widgets supply their own foreground, including contrasting text
        // on selected/Apply buttons; a global override would erase that state.
        visuals.override_text_color = None;
        visuals.panel_fill = p.panel;
        visuals.window_fill = p.raised;
        visuals.window_stroke = Stroke::new(1.0_f32, p.border);
        visuals.window_corner_radius = CornerRadius::same(4);
        visuals.menu_corner_radius = CornerRadius::same(4);
        visuals.window_shadow = Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(if self.is_dark() { 60 } else { 25 }),
        };
        visuals.popup_shadow = Shadow {
            offset: [0, 3],
            blur: 10,
            spread: 0,
            color: Color32::from_black_alpha(if self.is_dark() { 70 } else { 30 }),
        };
        visuals.faint_bg_color = p.button;
        visuals.extreme_bg_color = p.input;
        visuals.code_bg_color = p.button;
        visuals.selection.bg_fill = p.accent;
        visuals.selection.stroke = Stroke::new(1.0_f32, p.on_accent);
        visuals.hyperlink_color = p.accent;
        visuals.warn_fg_color = if self.is_dark() {
            Color32::from_rgb(255, 189, 100)
        } else {
            Color32::from_rgb(143, 76, 10)
        };
        visuals.error_fg_color = if self.is_dark() {
            Color32::from_rgb(255, 143, 143)
        } else {
            Color32::from_rgb(171, 45, 49)
        };
        visuals.text_cursor.stroke = Stroke::new(1.5_f32, p.accent);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = CornerRadius::same(4);
            widget.expansion = 0.0;
        }
        let normal = &mut visuals.widgets.noninteractive;
        normal.bg_fill = p.panel;
        // egui derives its weak/disabled text by tinting toward this color.
        // Use an opaque panel color so dark disabled controls fade toward the
        // dark panel instead of white, and normal text stays state-dependent.
        normal.weak_bg_fill = p.panel;
        normal.bg_stroke = Stroke::new(1.0_f32, p.border);
        normal.fg_stroke = Stroke::new(1.0_f32, p.text);
        let inactive = &mut visuals.widgets.inactive;
        inactive.bg_fill = p.button;
        inactive.weak_bg_fill = p.button;
        inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
        inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
        let hovered = &mut visuals.widgets.hovered;
        hovered.bg_fill = p.accent_soft;
        hovered.weak_bg_fill = p.accent_soft;
        hovered.bg_stroke = Stroke::new(1.0_f32, p.accent);
        hovered.fg_stroke = Stroke::new(1.0_f32, p.accent);
        let active = &mut visuals.widgets.active;
        active.bg_fill = p.accent;
        active.weak_bg_fill = p.accent;
        active.bg_stroke = Stroke::new(1.0_f32, p.accent);
        active.fg_stroke = Stroke::new(1.0_f32, p.on_accent);
        // Open menus remain neutral: their text is readable throughout the
        // entire menu surface instead of becoming an accent-colored slab.
        visuals.widgets.open = visuals.widgets.hovered;
        visuals.striped = true;
        visuals.slider_trailing_fill = true;
        style.visuals = visuals;
    }
}

/// Opaque colors shared by panels and the 3D workspace. Selection-outline
/// colors are intentionally independent of the theme's button accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiPalette {
    pub text: Color32,
    pub muted: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub viewport: Color32,
    pub grid_minor: Color32,
    pub grid_major: Color32,
    pub sketch_line: Color32,
    pub outline: Color32,
    pub input: Color32,
    pub button: Color32,
}

/// Convert opaque display colors to the existing SceneLines RGB convention.
pub fn rgb(color: Color32) -> [f32; 3] {
    [
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color32, b: Color32) -> f64 {
        fn luminance(c: Color32) -> f64 {
            let linear = |value: u8| {
                let v = value as f64 / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            linear(c.r()) * 0.2126 + linear(c.g()) * 0.7152 + linear(c.b()) * 0.0722
        }
        let x = luminance(a);
        let y = luminance(b);
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn missing_settings_default_to_graphite_and_explicit_names_round_trip() {
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct OldCompatibleSettings {
            appearance: UiAppearance,
        }
        let old: OldCompatibleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.appearance, UiAppearance::GraphiteStudio);
        for (appearance, name) in UiAppearance::ALL.into_iter().zip([
            "original",
            "precision_classic",
            "graphite_studio",
            "porcelain_workshop",
        ]) {
            let encoded = serde_json::to_string(&appearance).unwrap();
            assert_eq!(encoded, format!("\"{name}\""));
            assert_eq!(
                serde_json::from_str::<UiAppearance>(&encoded).unwrap(),
                appearance
            );
        }
    }

    #[test]
    fn new_palettes_keep_small_text_readable() {
        for appearance in UiAppearance::ALL.into_iter().filter(|a| !a.is_original()) {
            let p = appearance.palette();
            for (foreground, background) in [
                (p.text, p.panel),
                (p.text, p.raised),
                (p.text, p.input),
                (p.text, p.button),
                (p.muted, p.panel),
                (p.muted, p.raised),
                (p.accent, p.accent_soft),
                (p.on_accent, p.accent),
            ] {
                assert!(
                    contrast(foreground, background) >= 4.5,
                    "{appearance:?}: small text contrast is {:.2}",
                    contrast(foreground, background)
                );
            }
            assert!(contrast(p.sketch_line, p.viewport) >= 4.5);
            assert!(contrast(p.outline, p.viewport) >= 3.0);
        }
    }

    #[test]
    fn original_is_no_op_and_new_styles_keep_layout_and_fonts() {
        let mut original = egui::Style::default();
        original.spacing.item_spacing = egui::vec2(3.0, 5.0);
        original.visuals.panel_fill = Color32::from_rgb(17, 31, 53);
        let before = original.clone();
        UiAppearance::Original.configure_visuals(&mut original);
        assert_eq!(original, before);
        for appearance in UiAppearance::ALL.into_iter().filter(|a| !a.is_original()) {
            let mut style = before.clone();
            appearance.configure_visuals(&mut style);
            assert_eq!(style.spacing, before.spacing);
            assert_eq!(style.text_styles, before.text_styles);
            assert_eq!(style.visuals.dark_mode, appearance.is_dark());
            assert_eq!(style.visuals.panel_fill, appearance.palette().panel);
            assert_eq!(style.visuals.override_text_color, None);
            assert!(appearance.icons_enabled());
        }
    }

    #[test]
    fn concepts_are_visually_distinct() {
        let a = UiAppearance::PrecisionClassic.palette();
        let b = UiAppearance::GraphiteStudio.palette();
        let c = UiAppearance::PorcelainWorkshop.palette();
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert!(contrast(c.viewport, a.viewport) > 8.0);
        assert!(!UiAppearance::Original.icons_enabled());
        assert_eq!(rgb(Color32::WHITE), [1.0; 3]);
    }
}
