//! Native work-area placement. All fitting is in physical pixels; viewport
//! commands convert once to egui points, using its current pixels-per-point.
use egui::{pos2, vec2, Context, Pos2, Rect, Vec2, ViewportCommand as Cmd};

fn fitted(work: Rect, chrome: Vec2, desired: Vec2, center: Pos2) -> (Vec2, Pos2) {
    let available = (work.size() - chrome).max(Vec2::splat(1.0));
    let inner = desired.min(available).max(Vec2::splat(1.0));
    let outer = inner + chrome;
    let min = work.min;
    let max = (work.max - outer).max(min);
    (inner, (center - outer * 0.5).clamp(min, max))
}

// Preserve the current 768 × 1024 reference on a 3840 × 2160 display.
// Resolution controls the physical footprint; OS scaling is converted once.
fn splash_physical_size(available: Vec2) -> Vec2 {
    vec2(768., 1024.) * (available.x / 3840.).min(available.y / 2160.)
}

#[derive(Default)]
pub(crate) struct Placement {
    previous: Option<(Rect, f32, f32, bool)>,
    settle: u8,
}

impl Placement {
    pub fn update(&mut self, ctx: &Context, native: &impl NativeWindow, workspace: bool) {
        // Other backends expose monitor size, but not a native taskbar work
        // area. Keep their workspace restoration bounded to that monitor.
        let ppp = ctx.pixels_per_point();
        if !ppp.is_finite() || ppp <= 0.0 {
            return;
        }
        let fallback = || {
            ctx.input(|i| {
                let v = i.viewport();
                let size = v.monitor_size?;
                let outer = v.outer_rect?;
                let inner = v.inner_rect.map(|r| r.size()).unwrap_or(outer.size());
                Some(Bounds {
                    work: Rect::from_center_size(outer.center() * ppp, size * ppp),
                    outer: Rect::from_min_max(outer.min * ppp, outer.max * ppp),
                    inner: inner * ppp,
                    chrome: if workspace {
                        vec2(24., 64.) * ppp
                    } else {
                        Vec2::ZERO
                    },
                    dpi: v.native_pixels_per_point.unwrap_or(1.) * 96.,
                    minimized: v.minimized.unwrap_or(false),
                    maximized: v.maximized.unwrap_or(false),
                })
            })
        };
        let Some(bounds) = native.bounds(workspace).or_else(fallback) else {
            return;
        };
        if bounds.minimized {
            return;
        }
        let environment = (bounds.work, bounds.dpi, ppp, workspace);
        let changed = self.previous != Some(environment);
        let switching = self
            .previous
            .is_none_or(|(_, _, _, was_workspace)| was_workspace != workspace);
        if changed {
            self.previous = Some(environment);
            self.settle = 3;
            let available = (bounds.work.size() - bounds.chrome).max(Vec2::splat(1.0));
            let minimum = if workspace {
                vec2(900.0, 600.0) * ppp
            } else {
                Vec2::splat(1.0)
            };
            ctx.send_viewport_cmd(Cmd::MinInnerSize(minimum.min(available) / ppp));
            ctx.send_viewport_cmd(Cmd::MaxInnerSize(available / ppp));
            let preferred = if workspace {
                vec2(1440.0, 900.0)
            } else {
                vec2(768.0, 1024.0)
            };
            let preferred = if !workspace {
                splash_physical_size(available) / ppp
            } else {
                preferred
            };
            let desired = if switching {
                preferred * ppp
            } else {
                bounds.inner
            };
            let center = if switching {
                bounds.work.center()
            } else {
                bounds.outer.center()
            };
            let (inner, position) = fitted(bounds.work, bounds.chrome, desired, center);
            if !bounds.maximized {
                ctx.send_viewport_cmd(Cmd::InnerSize(inner / ppp));
                ctx.send_viewport_cmd(Cmd::OuterPosition(position / ppp));
            }
        } else if self.settle > 0 {
            if !bounds.maximized {
                // The OS may return a different size after decorations/DPI change.
                // Clamp that actual outer rectangle, rather than the requested size.
                let chrome = (bounds.outer.size() - bounds.inner).max(Vec2::ZERO);
                let (inner, position) =
                    fitted(bounds.work, chrome, bounds.inner, bounds.outer.center());
                if (inner - bounds.inner).length_sq() > 1.0 {
                    ctx.send_viewport_cmd(Cmd::InnerSize(inner / ppp));
                }
                if (position - bounds.outer.min).length_sq() > 1.0 {
                    ctx.send_viewport_cmd(Cmd::OuterPosition(position / ppp));
                }
            }
            self.settle -= 1;
        }
        if self.settle > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    work: Rect,
    outer: Rect,
    inner: Vec2,
    chrome: Vec2,
    dpi: f32,
    minimized: bool,
    maximized: bool,
}
pub(crate) trait NativeWindow {
    fn bounds(&self, workspace: bool) -> Option<Bounds>;
}

#[cfg(not(target_os = "windows"))]
impl NativeWindow for eframe::Frame {
    fn bounds(&self, _: bool) -> Option<Bounds> {
        None
    }
}
#[cfg(not(target_os = "windows"))]
impl NativeWindow for eframe::CreationContext<'_> {
    fn bounds(&self, _: bool) -> Option<Bounds> {
        None
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Default)]
    struct WinRect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    impl WinRect {
        fn rect(&self) -> Rect {
            Rect::from_min_max(
                pos2(self.left as f32, self.top as f32),
                pos2(self.right as f32, self.bottom as f32),
            )
        }
    }
    #[repr(C)]
    struct MonitorInfo {
        size: u32,
        monitor: WinRect,
        work: WinRect,
        flags: u32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MonitorFromWindow(window: *mut c_void, flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
        fn GetWindowRect(window: *mut c_void, rect: *mut WinRect) -> i32;
        fn GetClientRect(window: *mut c_void, rect: *mut WinRect) -> i32;
        fn GetDpiForWindow(window: *mut c_void) -> u32;
        fn GetWindowLongW(window: *mut c_void, index: i32) -> i32;
        fn AdjustWindowRectExForDpi(
            rect: *mut WinRect,
            style: u32,
            menu: i32,
            ex_style: u32,
            dpi: u32,
        ) -> i32;
        fn IsIconic(window: *mut c_void) -> i32;
        fn IsZoomed(window: *mut c_void) -> i32;
    }
    impl<T: HasWindowHandle> NativeWindow for T {
        fn bounds(&self, workspace: bool) -> Option<Bounds> {
            let RawWindowHandle::Win32(handle) = self.window_handle().ok()?.as_raw() else {
                return None;
            };
            let window = handle.hwnd.get() as *mut c_void;
            // Handles are borrowed from eframe, used synchronously on its UI
            // thread, and never stored or destroyed by this module.
            unsafe {
                let monitor = MonitorFromWindow(window, 2); // nearest, including disconnected-screen recovery
                let mut info = MonitorInfo {
                    size: std::mem::size_of::<MonitorInfo>() as u32,
                    monitor: WinRect::default(),
                    work: WinRect::default(),
                    flags: 0,
                };
                let mut outer = WinRect::default();
                let mut inner = WinRect::default();
                let dpi = GetDpiForWindow(window);
                if monitor.is_null()
                    || dpi == 0
                    || GetMonitorInfoW(monitor, &mut info) == 0
                    || GetWindowRect(window, &mut outer) == 0
                    || GetClientRect(window, &mut inner) == 0
                {
                    return None;
                }
                let work = info.work.rect();
                if !work.is_positive() {
                    return None;
                }
                // Predict the normal decorated workspace before its viewport
                // commands are processed; settle against actual borders later.
                let style = if workspace {
                    (GetWindowLongW(window, -16) as u32 & !0x80000000) | 0x00cf0000
                } else {
                    GetWindowLongW(window, -16) as u32
                };
                let mut border = WinRect::default();
                let chrome = if workspace
                    && AdjustWindowRectExForDpi(
                        &mut border,
                        style,
                        0,
                        GetWindowLongW(window, -20) as u32,
                        dpi,
                    ) != 0
                {
                    border.rect().size()
                } else {
                    (outer.rect().size() - inner.rect().size()).max(Vec2::ZERO)
                };
                Some(Bounds {
                    work,
                    outer: outer.rect(),
                    inner: inner.rect().size(),
                    chrome,
                    dpi: dpi as f32,
                    minimized: IsIconic(window) != 0,
                    maximized: IsZoomed(window) != 0,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fitting_preserves_borders_taskbars_negative_monitors_and_small_work_areas() {
        for (w, h, scale) in [
            (1366., 728., 1.),
            (1920., 1040., 1.25),
            (1920., 1040., 1.5),
            (3840., 2080., 2.),
        ] {
            for origin in [pos2(0., 0.), pos2(-3840., -600.)] {
                let work = Rect::from_min_size(origin, vec2(w, h));
                let chrome = vec2(16., 39.) * scale;
                let (size, position) =
                    fitted(work, chrome, vec2(1440., 900.) * scale, work.center());
                assert!(work
                    .expand(0.01)
                    .contains_rect(Rect::from_min_size(position, size + chrome)));
                assert!(size.x <= 1440. * scale && size.y <= 900. * scale);
                let (actual, clamped) = fitted(work, chrome, size, origin - vec2(200., 400.));
                assert_eq!(actual, size);
                assert_eq!(clamped, origin);
            }
        }
    }

    struct Fake(Bounds);
    impl NativeWindow for Fake {
        fn bounds(&self, _: bool) -> Option<Bounds> {
            Some(self.0)
        }
    }
    #[test]
    fn display_native_placement_converts_dpi_once_and_tracks_actual_os_size() {
        for scale in [1., 1.25, 1.5, 2.] {
            let ctx = Context::default();
            let work = Rect::from_min_size(pos2(-1920., -200.), vec2(1920., 1040.));
            let mut fake = Fake(Bounds {
                work,
                outer: Rect::from_min_size(pos2(-900., 700.), vec2(1100., 800.)),
                inner: vec2(1084., 761.),
                chrome: vec2(16., 39.) * scale,
                dpi: 96. * scale,
                minimized: false,
                maximized: false,
            });
            let mut guard = Placement::default();
            let mut raw = egui::RawInput::default();
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(scale);
            let out = ctx.run(raw.clone(), |ctx| guard.update(ctx, &fake, true));
            let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
            let size = commands
                .iter()
                .find_map(|c| {
                    if let Cmd::InnerSize(s) = c {
                        Some(*s)
                    } else {
                        None
                    }
                })
                .unwrap();
            let position = commands
                .iter()
                .find_map(|c| {
                    if let Cmd::OuterPosition(s) = c {
                        Some(*s)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(work.expand(0.1).contains_rect(Rect::from_min_size(
                position * scale,
                size * scale + fake.0.chrome
            )));
            // Simulate an OS-adjusted rectangle, not the requested rectangle.
            fake.0.outer = Rect::from_min_size(work.max - vec2(100., 100.), vec2(800., 600.));
            fake.0.inner = vec2(784., 561.);
            let out = ctx.run(raw.clone(), |ctx| guard.update(ctx, &fake, true));
            let position = out.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .find_map(|c| {
                    if let Cmd::OuterPosition(p) = c {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(work
                .expand(0.1)
                .contains_rect(Rect::from_min_size(position * scale, fake.0.outer.size())));
            // Stable frames must never force the default workspace size back.
            fake.0.outer = Rect::from_min_size(work.min, vec2(800., 600.));
            for _ in 0..5 {
                let _ = ctx.run(raw.clone(), |ctx| guard.update(ctx, &fake, true));
            }
            let out = ctx.run(raw.clone(), |ctx| guard.update(ctx, &fake, true));
            assert!(out.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .is_empty());
            // Move the same window to a smaller monitor with another DPI.
            fake.0.work = Rect::from_min_size(pos2(0., 40.), vec2(1366., 688.));
            fake.0.dpi = 144.;
            fake.0.chrome = vec2(24., 59.);
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(1.5);
            let out = ctx.run(raw.clone(), |ctx| guard.update(ctx, &fake, true));
            let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
            let size = commands
                .iter()
                .find_map(|c| {
                    if let Cmd::InnerSize(s) = c {
                        Some(*s)
                    } else {
                        None
                    }
                })
                .unwrap();
            let position = commands
                .iter()
                .find_map(|c| {
                    if let Cmd::OuterPosition(p) = c {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(fake.0.work.expand(0.1).contains_rect(Rect::from_min_size(
                position * 1.5,
                size * 1.5 + fake.0.chrome
            )));
            fake.0.maximized = true;
            for _ in 0..5 {
                let _ = ctx.run(raw.clone(), |ctx| guard.update(ctx, &fake, true));
            }
            assert_eq!(
                guard.settle, 0,
                "maximizing must not keep a permanent placement repaint loop"
            );
        }
    }
}

#[cfg(test)]
mod proportional_splash_tests {
    use super::*;
    #[test]
    fn scales_with_resolution_and_stays_centered_at_every_dpi() {
        for resolution in [
            vec2(1920., 1080.),
            vec2(2560., 1440.),
            vec2(3840., 2160.),
            vec2(7680., 4320.),
        ] {
            let size = splash_physical_size(resolution);
            assert!((size.y / resolution.y - 1024. / 2160.).abs() < 1e-6);
            for ppp in [1., 1.5, 2.] {
                let work = Rect::from_min_size(pos2(-1920., 40.), resolution);
                let (inner, position) = fitted(work, Vec2::ZERO, (size / ppp) * ppp, work.center());
                assert!((position + inner * 0.5 - work.center()).length() < 0.01);
                assert_eq!(inner, size);
            }
        }
    }
}
