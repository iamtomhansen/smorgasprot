//! Named color palette shared by panels, legends, and viewport scenes.
//! Values are Tailwind CSS tones, matching the frozen HTML viewer.

use egui::Color32;
use smorgasprot_core::InterfaceSide;

// --- Interface sides ---
pub const SIDE_A: Color32 = Color32::from_rgb(14, 165, 233); // sky-500
pub const SIDE_A_LIGHT: Color32 = Color32::from_rgb(56, 189, 248); // sky-400
pub const SIDE_A_DARK: Color32 = Color32::from_rgb(2, 132, 199); // sky-600
pub const SIDE_B: Color32 = Color32::from_rgb(249, 115, 22); // orange-500
pub const SIDE_B_LIGHT: Color32 = Color32::from_rgb(251, 146, 60); // orange-400
pub const SIDE_B_DARK: Color32 = Color32::from_rgb(234, 88, 12); // orange-600
pub const UNASSIGNED: Color32 = Color32::from_rgb(148, 163, 184); // slate-400

// --- Contacts ---
pub const CLASH: Color32 = Color32::from_rgb(242, 20, 20);
pub const SALT_BRIDGE: Color32 = Color32::from_rgb(157, 20, 255);
pub const HBOND: Color32 = Color32::from_rgb(37, 99, 235); // blue-600
pub const CONTACT: Color32 = Color32::from_rgb(148, 163, 184); // slate-400

// --- Chemistry lens ---
pub const ALIPHATIC: Color32 = Color32::from_rgb(148, 163, 184); // slate-400
pub const AROMATIC: Color32 = Color32::from_rgb(245, 158, 11); // amber-500
pub const POSITIVE: Color32 = Color32::from_rgb(59, 130, 246); // blue-500
pub const NEGATIVE: Color32 = Color32::from_rgb(239, 68, 68); // red-500
pub const GLY_PRO: Color32 = Color32::from_rgb(168, 85, 247); // purple-500
pub const POLAR: Color32 = Color32::from_rgb(6, 182, 212); // cyan-500

// --- Modern CPK lens ---
pub const CPK_C: Color32 = Color32::from_rgb(71, 85, 105); // slate-600
pub const CPK_N: Color32 = Color32::from_rgb(37, 99, 235); // blue-600
pub const CPK_O: Color32 = Color32::from_rgb(220, 38, 38); // red-600
pub const CPK_S: Color32 = Color32::from_rgb(217, 119, 6); // amber-600
pub const CPK_P: Color32 = Color32::from_rgb(234, 88, 12); // orange-600
pub const CPK_OTHER: Color32 = Color32::from_rgb(100, 116, 139); // slate-500

/// B-factor / pLDDT bins from most ordered (index 0) to most mobile (index 3); AlphaFold colors.
pub const MOBILITY_BINS: [Color32; 4] = [
    Color32::from_rgb(0, 83, 214),
    Color32::from_rgb(101, 203, 243),
    Color32::from_rgb(255, 219, 19),
    Color32::from_rgb(255, 125, 69),
];

// --- Derived interface geometry ---
pub const SHEET_FILL: Color32 = Color32::from_rgb(250, 204, 21); // yellow-400
pub const SHEET_EDGE: Color32 = Color32::from_rgb(217, 119, 6); // amber-600
pub const ENVELOPE_A: Color32 = Color32::from_rgb(250, 204, 21); // yellow-400
pub const ENVELOPE_A_EDGE: Color32 = Color32::from_rgb(180, 83, 9); // amber-700
pub const ENVELOPE_B: Color32 = Color32::from_rgb(168, 85, 247); // purple-500
pub const ENVELOPE_B_EDGE: Color32 = Color32::from_rgb(107, 33, 168); // purple-800
pub const VOID: Color32 = Color32::from_rgb(6, 182, 212); // cyan-500
pub const VOID_EDGE: Color32 = Color32::from_rgb(14, 116, 144); // cyan-700

// --- Viewport chrome ---
pub const VIEWPORT_BG: Color32 = Color32::from_rgb(248, 250, 252); // slate-50
pub const GRID: Color32 = Color32::from_rgb(226, 232, 240); // slate-200
pub const INK: Color32 = Color32::from_rgb(15, 23, 42); // slate-900, outlines and labels
pub const HINGE: Color32 = Color32::from_rgb(100, 116, 139); // slate-500
pub const HOVER_RING: Color32 = Color32::from_rgb(14, 165, 233); // sky-500

// --- Gizmo axes (positive, negative) ---
pub const AXIS_U: (Color32, Color32) = (Color32::from_rgb(239, 68, 68), Color32::from_rgb(153, 27, 27));
pub const AXIS_V: (Color32, Color32) = (Color32::from_rgb(34, 197, 94), Color32::from_rgb(22, 101, 52));
pub const AXIS_N: (Color32, Color32) = (Color32::from_rgb(59, 130, 246), Color32::from_rgb(30, 64, 175));

pub const ACCENT: Color32 = Color32::from_rgb(14, 165, 233); // sky-500, section headings
pub const MUTED: Color32 = Color32::from_rgb(100, 116, 139); // slate-500, muted captions
pub const WARNING: Color32 = Color32::from_rgb(245, 158, 11); // amber-500
pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94); // green-500
pub const ERROR: Color32 = Color32::from_rgb(239, 68, 68); // red-500

/// Base color for an interface side.
pub fn side(side: InterfaceSide) -> Color32 {
    match side {
        InterfaceSide::SideA => SIDE_A,
        InterfaceSide::SideB => SIDE_B,
        InterfaceSide::None => UNASSIGNED,
    }
}

/// Lighter tint for an interface side (labels, spheres).
pub fn side_light(side: InterfaceSide) -> Color32 {
    match side {
        InterfaceSide::SideA => SIDE_A_LIGHT,
        InterfaceSide::SideB => SIDE_B_LIGHT,
        InterfaceSide::None => UNASSIGNED,
    }
}

/// Darker shade for an interface side (outlines, ribbons).
pub fn side_dark(side: InterfaceSide) -> Color32 {
    match side {
        InterfaceSide::SideA => SIDE_A_DARK,
        InterfaceSide::SideB => SIDE_B_DARK,
        InterfaceSide::None => UNASSIGNED,
    }
}

/// Applies an opacity in `[0, 1]` to an opaque color.
pub fn with_alpha(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Darkens an opaque color by a Lambert `shade` factor and applies opacity.
pub fn shaded(c: Color32, shade: f32, alpha: f32) -> Color32 {
    let s = |ch: u8| (ch as f32 * shade) as u8;
    with_alpha(Color32::from_rgb(s(c.r()), s(c.g()), s(c.b())), alpha)
}
