//! 模式状态机（与 Super3D `main.js` 的 mode 对应）。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    EquationCurve,
    OffsetShell,
    /// 空闲：选择/操作 3D 对象。
    Idle,
    /// 选择草图基准面。
    PickPlane,
    /// 在平面上画草图。
    Sketch,
    /// 拉伸（输入深度/拖拽）。
    Extrude,
    /// 选择旋转轴。
    RevolveAxis,
    /// 旋转（输入角度）。
    Revolve,
    /// 布尔运算（union/subtract/intersect）。
    Boolean,
    /// 平面切分（选 object + 切割平面）。
    Split,
    /// 剖面视图（所有 visible objects 沿平面剖切显示）。
    SectionView,
    /// 镜像（选镜像面）。
    Mirror,
    /// 插入参数化基础 3D 物体。
    Insert3D,
}

/// Align 对齐方式（参考 Tinkercad）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    /// 负方向边缘（min 边界）。
    Min,
    /// 中心。
    Center,
    /// 正方向边缘（max 边界）。
    Max,
}

impl Mode {
    pub fn name(&self) -> &'static str {
        match self {
            Mode::EquationCurve => "equation-curve",
            Mode::OffsetShell => "offset-shell",
            Mode::Idle => "idle",
            Mode::PickPlane => "pick-plane",
            Mode::Sketch => "sketch",
            Mode::Extrude => "extrude",
            Mode::RevolveAxis => "revolve-axis",
            Mode::Revolve => "revolve",
            Mode::Boolean => "boolean",
            Mode::Split => "split",
            Mode::SectionView => "section-view",
            Mode::Mirror => "mirror",
            Mode::Insert3D => "draw-3d",
        }
    }
}
