//! Portable, editable screw and nut recipes. All lengths are millimeters.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FastenerKind {
    HexScrew,
    SocketScrew,
    CountersunkScrew,
    ThumbScrew,
    Stud,
    HexNut,
    SquareNut,
    RoundNut,
    FlangeNut,
}
impl FastenerKind {
    pub const SCREWS: [Self; 5] = [
        Self::HexScrew,
        Self::SocketScrew,
        Self::CountersunkScrew,
        Self::ThumbScrew,
        Self::Stud,
    ];
    pub const NUTS: [Self; 4] = [
        Self::HexNut,
        Self::SquareNut,
        Self::RoundNut,
        Self::FlangeNut,
    ];
    pub fn is_nut(self) -> bool {
        Self::NUTS.contains(&self)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::HexScrew => "Hex screw",
            Self::SocketScrew => "Socket cap screw",
            Self::CountersunkScrew => "Countersunk screw",
            Self::ThumbScrew => "Thumb screw",
            Self::Stud => "Stud",
            Self::HexNut => "Hex nut",
            Self::SquareNut => "Square nut",
            Self::RoundNut => "Round nut",
            Self::FlangeNut => "Flange nut",
        }
    }
    pub fn name_zh(self) -> &'static str {
        match self {
            Self::HexScrew => "六角螺钉",
            Self::SocketScrew => "内六角圆柱头螺钉",
            Self::CountersunkScrew => "沉头螺钉",
            Self::ThumbScrew => "手拧螺钉",
            Self::Stud => "螺柱",
            Self::HexNut => "六角螺帽",
            Self::SquareNut => "方螺帽",
            Self::RoundNut => "圆螺帽",
            Self::FlangeNut => "法兰螺帽",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FastenerParams {
    pub kind: FastenerKind,
    pub diameter: f64,
    pub pitch: f64,
    /// Under-head length; countersunk length includes the head.
    pub length: f64,
    /// Hex/square across flats; round/cap/countersunk head diameter.
    pub head_size: f64,
    pub head_height: f64,
    pub socket_size: f64,
    pub thread_length: f64,
    pub clearance: f64,
    pub lead_in: f64,
    pub left_hand: bool,
    pub modeled_thread: bool,
    pub position: [f64; 3],
    /// Linear placement, preserved when transforming and editing a fastener.
    #[serde(default = "identity_basis")]
    pub basis: [f64; 9],
}
fn identity_basis() -> [f64; 9] {
    [1., 0., 0., 0., 1., 0., 0., 0., 1.]
}
impl FastenerParams {
    pub fn defaults(kind: FastenerKind) -> Self {
        Self::metric(kind, 6).unwrap()
    }
    /// Nominal metric references: ISO 4017 hex heads, ISO 4762 cap
    /// heads, ISO 4032:2023 regular nuts (M3/M4: legacy DIN 934).
    /// Thumb, countersunk and other nut styles are custom starting shapes.
    pub fn metric(kind: FastenerKind, size: u32) -> Option<Self> {
        let (pitch, flats, hex_height, cap_d, socket, nut_height) = match size {
            3 => (0.5, 5.5, 2., 5.5, 2.5, 2.4),
            4 => (0.7, 7., 2.8, 7., 3., 3.2),
            5 => (0.8, 8., 3.5, 8.5, 4., 4.7),
            6 => (1., 10., 4., 10., 5., 5.2),
            8 => (1.25, 13., 5.3, 13., 6., 6.8),
            10 => (1.5, 16., 6.4, 16., 8., 8.4),
            12 => (1.75, 18., 7.5, 18., 10., 10.8),
            _ => return None,
        };
        let d = size as f64;
        let nut = kind.is_nut();
        let head_size = match kind {
            FastenerKind::SocketScrew => cap_d,
            FastenerKind::CountersunkScrew => 2. * d,
            FastenerKind::ThumbScrew | FastenerKind::RoundNut => 2.5 * d,
            _ => flats,
        };
        let head_height = if nut {
            nut_height
        } else {
            match kind {
                FastenerKind::SocketScrew => d,
                FastenerKind::CountersunkScrew => d * 0.5,
                FastenerKind::ThumbScrew => d * 0.8,
                _ => hex_height,
            }
        };
        let length = if nut { nut_height } else { 3. * d };
        Some(Self {
            kind,
            diameter: d,
            pitch,
            length,
            head_size,
            head_height,
            socket_size: socket,
            thread_length: if kind == FastenerKind::CountersunkScrew {
                length - head_height
            } else {
                length
            },
            clearance: 0.,
            lead_in: pitch * 0.3,
            left_hand: false,
            modeled_thread: true,
            position: [0.; 3],
            basis: identity_basis(),
        })
    }
    pub fn print_coarse(kind: FastenerKind) -> Self {
        let mut p = Self::metric(kind, 12).unwrap();
        p.pitch = 2.;
        p.clearance = 0.15;
        p.lead_in = 0.6;
        p.head_size = if kind.is_nut() { 22. } else { 24. };
        if kind.is_nut() {
            p.length = 12.;
            p.head_height = 12.;
        }
        p.thread_length = p.shaft_length();
        p
    }
    pub fn shaft_length(&self) -> f64 {
        if self.kind == FastenerKind::CountersunkScrew {
            self.length - self.head_height
        } else {
            self.length
        }
    }
}
