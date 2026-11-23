//! Shared interaction for the two independently enabled fastener plug-ins.
use crate::{
    app::{UiLanguage, UiUnit},
    plugins::{PluginCommand, S33DPlugin},
};
use eframe::egui;
use s33d_model::{BodyId, FastenerKind as K, FastenerParams};

pub struct FastenerPlugin {
    nut: bool,
    enabled: bool,
    open: bool,
    params: FastenerParams,
    editing: Option<BodyId>,
    synced: Option<(BodyId, FastenerParams)>,
    preset: String,
}
impl FastenerPlugin {
    pub fn new(nut: bool, enabled: bool) -> Self {
        Self {
            nut,
            enabled,
            open: false,
            params: FastenerParams::defaults(if nut { K::HexNut } else { K::HexScrew }),
            editing: None,
            synced: None,
            preset: "M6".into(),
        }
    }
}
impl S33DPlugin for FastenerPlugin {
    fn id(&self) -> &'static str {
        if self.nut {
            "nut-generator"
        } else {
            "screw-generator"
        }
    }
    fn name(&self) -> &'static str {
        if self.nut {
            "Nut Generator"
        } else {
            "Screw Generator"
        }
    }
    fn description(&self) -> &'static str {
        "Editable printable fasteners with modeled helical threads."
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
    fn sync_selected_fastener(&mut self, selection: Option<(BodyId, FastenerParams)>) {
        let selection = selection.filter(|(_, p)| p.kind.is_nut() == self.nut);
        if selection == self.synced {
            return;
        }
        self.synced = selection;
        if let Some((id, p)) = selection {
            self.params = p;
            self.editing = Some(id);
            self.preset = "Custom".into();
        } else {
            self.editing = None;
        }
    }
    fn draw(&mut self, ctx: &egui::Context, l: UiLanguage, unit: UiUnit) -> Option<PluginCommand> {
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut command = None;
        let title = if self.nut {
            l.text("Nut Generator", "螺帽生成器")
        } else {
            l.text("Screw Generator", "螺钉生成器")
        };
        egui::Window::new(title).id(egui::Id::new(self.id())).default_width(380.).resizable(true).open(&mut open)
            .show(ctx,|ui| {
            egui::ScrollArea::vertical().max_height((ctx.screen_rect().height()-160.).max(200.)).show(ui,|ui| {
                ui.horizontal(|ui| {
                    ui.label(l.text("Style","类型"));
                    let previous=self.params.kind;
                    egui::ComboBox::from_id_salt((self.id(),"kind"))
                        .selected_text(l.text(self.params.kind.name(),self.params.kind.name_zh())).show_ui(ui,|ui| {
                            for kind in if self.nut {&K::NUTS[..]}else{&K::SCREWS[..]} {
                                ui.selectable_value(&mut self.params.kind,*kind,l.text(kind.name(),kind.name_zh()));
                            }
                        });
                    if previous!=self.params.kind {
                        let position=self.params.position; let basis=self.params.basis;
                        self.params=FastenerParams::defaults(self.params.kind);
                        self.params.position=position; self.params.basis=basis;self.preset="M6".into();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(l.text("Size preset","尺寸预设"));
                    let display=match self.preset.as_str() {"Custom"=>l.text("Custom","自定义"),"Print 12 × 2"=>l.text("Print coarse 12 × 2","打印粗牙 12 × 2"),_=>&self.preset};
                    egui::ComboBox::from_id_salt((self.id(),"preset")).selected_text(display).show_ui(ui,|ui| {
                        for size in [3,4,5,6,8,10,12] {
                            let label=format!("M{size}");
                            if ui.selectable_label(self.preset==label,&label).clicked() {
                                let position=self.params.position; let basis=self.params.basis;
                                self.params=FastenerParams::metric(self.params.kind,size).unwrap();
                                self.params.position=position; self.params.basis=basis;self.preset=label;
                            }
                        }
                        if ui.selectable_label(self.preset=="Print 12 × 2",l.text("Print coarse 12 × 2","打印粗牙 12 × 2")).clicked() {
                            let position=self.params.position; let basis=self.params.basis;
                            self.params=FastenerParams::print_coarse(self.params.kind);
                            self.params.position=position; self.params.basis=basis;self.preset="Print 12 × 2".into();
                        }
                        ui.selectable_value(&mut self.preset,"Custom".into(),l.text("Custom","自定义"));
                    });
                });
                ui.weak(l.text("Presets are nominal starting dimensions; all fields remain editable.","预设提供公称起始尺寸，以下参数均可自定义。"));
                let before=self.params;
                dimension(ui,l.text("Nominal diameter","公称直径"),&mut self.params.diameter,unit);
                dimension(ui,l.text("Pitch","螺距"),&mut self.params.pitch,unit);
                let length_label=if self.nut {l.text("Nut height","螺帽高度")}else if self.params.kind==K::CountersunkScrew {l.text("Overall length (head included)","总长（含沉头）")}else if self.params.kind==K::Stud {l.text("Overall length","总长")}else{l.text("Under-head length","头下长度")};
                dimension(ui,length_label,&mut self.params.length,unit);
                if self.params.kind!=K::Stud {
                    dimension(ui,if matches!(self.params.kind,K::HexScrew|K::HexNut|K::SquareNut|K::FlangeNut) {l.text("Width across flats","对边宽")}else{l.text("Head / body diameter","头部 / 帽体直径")},&mut self.params.head_size,unit);
                    if !self.nut {dimension(ui,l.text("Head height","头部高度"),&mut self.params.head_height,unit);}
                    if matches!(self.params.kind,K::SocketScrew|K::CountersunkScrew) {
                        dimension(ui,l.text("Hex socket across flats","内六角对边宽"),&mut self.params.socket_size,unit);
                    }
                }
                if self.nut { self.params.thread_length=self.params.length;self.params.head_height=self.params.length; }
                else {
                    let mut full=(self.params.thread_length-before.shaft_length()).abs()<1e-6;
                    if ui.checkbox(&mut full,l.text("Full thread","全螺纹")).changed() && !full {self.params.thread_length=self.params.shaft_length()*0.7;}
                    if full {self.params.thread_length=self.params.shaft_length();}
                    else {dimension(ui,l.text("Thread length from tip","从端部起的螺纹长度"),&mut self.params.thread_length,unit);}
                }
                ui.separator();
                ui.checkbox(&mut self.params.modeled_thread,l.text("Modeled thread (printable)","实体螺纹（可打印）"));
                ui.checkbox(&mut self.params.left_hand,l.text("Left-hand thread","左旋螺纹"));
                dimension(ui,l.text("Radial print clearance","径向打印间隙"),&mut self.params.clearance,unit);
                dimension(ui,l.text("Entry chamfer","入口倒角"),&mut self.params.lead_in,unit);
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button(l.text("Nominal fit","公称配合")).clicked() {self.params.clearance=0.;}
                    if ui.small_button(l.text("Print fit","打印配合")).clicked() {self.params.clearance=(self.params.pitch*0.075).min(0.15);}
                });
                ui.weak(l.text("Print a fit sample first. Larger, coarser threads are easier to print; clearance is radial, per part.","请先打印配合试件。较大直径和粗牙更易打印；间隙为单件径向值。"));
                ui.collapsing(l.text("Position (base, Y-up)","位置（底部，Y轴朝上）"),|ui| {
                    for (i,name) in ["X","Y","Z"].iter().enumerate() {dimension(ui,name,&mut self.params.position[i],unit);}
                });
                if before!=self.params {self.preset="Custom".into();}
                let error=crate::fasteners::validate(&self.params).err();
                if let Some(error)=&error {
                    ui.colored_label(ui.visuals().error_fg_color,crate::fasteners::error_text(error,l));
                }
                ui.separator();
                if let Some(id)=self.editing {
                    ui.label(format!("{} #{id}",l.text("Editing object","编辑对象")));
                }
                ui.horizontal_wrapped(|ui| {
                    let label=if self.editing.is_some(){l.text("Update selected","更新所选对象")}else{l.text("Generate","生成")};
                    if ui.add_enabled(error.is_none(),egui::Button::new(label)).clicked() {
                        command=Some(PluginCommand::GenerateFastener{params:self.params,replace_body:self.editing});
                    }
                    if self.editing.is_some() && ui.add_enabled(error.is_none(),egui::Button::new(l.text("Create copy","新建副本"))).clicked() {
                        command=Some(PluginCommand::GenerateFastener{params:self.params,replace_body:None});
                    }
                    if ui.button(l.text("Collapse","收起")).clicked() {self.open=false;}
                });
            });
        });
        self.open &= open;
        command
    }
}
fn dimension(ui: &mut egui::Ui, label: &str, value: &mut f64, unit: UiUnit) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(crate::ui::length_drag_value(value, unit).speed(0.05));
    });
}
