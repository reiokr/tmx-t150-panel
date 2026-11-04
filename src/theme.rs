use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Rounding, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn name(self) -> &'static str {
        match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }

    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    fn parse(text: &str) -> ThemeMode {
        match text.trim().to_lowercase().as_str() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

/// The colours the whole panel paints with.  Both palettes keep the same
/// structure so a switch only changes the values.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub card: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub accent_dark: Color32,
    pub accent_soft: Color32,
    pub ok_green: Color32,
    pub warn: Color32,
    pub track: Color32,
    pub metal: Color32,
    pub metal_light: Color32,
}

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(233, 236, 240),
    card: Color32::from_rgb(255, 255, 255),
    ink: Color32::from_rgb(28, 36, 48),
    muted: Color32::from_rgb(104, 116, 132),
    line: Color32::from_rgb(211, 217, 224),
    accent: Color32::from_rgb(31, 95, 168),
    accent_dark: Color32::from_rgb(21, 62, 110),
    accent_soft: Color32::from_rgb(223, 234, 247),
    ok_green: Color32::from_rgb(42, 122, 82),
    warn: Color32::from_rgb(190, 120, 30),
    track: Color32::from_rgb(214, 220, 227),
    metal: Color32::from_rgb(54, 60, 70),
    metal_light: Color32::from_rgb(96, 104, 116),
};

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(23, 26, 31),
    card: Color32::from_rgb(33, 37, 44),
    ink: Color32::from_rgb(231, 235, 241),
    muted: Color32::from_rgb(148, 158, 172),
    line: Color32::from_rgb(58, 64, 74),
    accent: Color32::from_rgb(86, 152, 232),
    accent_dark: Color32::from_rgb(46, 100, 168),
    accent_soft: Color32::from_rgb(44, 60, 82),
    ok_green: Color32::from_rgb(104, 196, 148),
    warn: Color32::from_rgb(232, 168, 86),
    track: Color32::from_rgb(52, 58, 68),
    metal: Color32::from_rgb(70, 78, 90),
    metal_light: Color32::from_rgb(126, 136, 150),
};

use std::sync::atomic::{AtomicU8, Ordering};

/// 0 = system, 1 = light, 2 = dark.
static MODE: AtomicU8 = AtomicU8::new(0);
static DARK_NOW: AtomicU8 = AtomicU8::new(0);

pub fn mode() -> ThemeMode {
    match MODE.load(Ordering::Relaxed) {
        1 => ThemeMode::Light,
        2 => ThemeMode::Dark,
        _ => ThemeMode::System,
    }
}

pub fn set_mode(mode: ThemeMode) {
    MODE.store(
        match mode {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        },
        Ordering::Relaxed,
    );
    DARK_NOW.store(if effective_dark(mode) { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK_NOW.load(Ordering::Relaxed) != 0
}

/// The colours for the theme in force.
pub fn pal() -> Palette {
    if is_dark() {
        DARK
    } else {
        LIGHT
    }
}

fn effective_dark(mode: ThemeMode) -> bool {
    match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => system_is_dark(),
    }
}

/// KDE keeps the active scheme in kdeglobals; a light window background means a
/// light desktop.  Anything unreadable falls back to light, the original look.
fn system_is_dark() -> bool {
    let Ok(home) = std::env::var("HOME") else {
        return false;
    };
    let path = std::path::Path::new(&home).join(".config/kdeglobals");
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let mut in_window = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_window = line.eq_ignore_ascii_case("[Colors:Window]");
            continue;
        }
        if !in_window {
            continue;
        }
        if let Some(value) = line.strip_prefix("BackgroundNormal=") {
            let parts: Vec<f32> = value
                .split(',')
                .filter_map(|p| p.trim().parse::<f32>().ok())
                .collect();
            if parts.len() >= 3 {
                let luma = (0.2126 * parts[0] + 0.7152 * parts[1] + 0.0722 * parts[2]) / 255.0;
                return luma < 0.5;
            }
        }
    }
    false
}

fn ui_config() -> std::path::PathBuf {
    crate::paths::config_dir().join("ui.conf")
}

/// UI zoom steps, so the text can be made larger on a 4K screen.
pub const SCALES: [f32; 5] = [1.0, 1.1, 1.2, 1.3, 1.5];
pub const SCALE_DEFAULT: f32 = 1.2;

pub fn load_scale() -> f32 {
    std::fs::read_to_string(ui_config())
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("scale=")
                    .and_then(|v| v.trim().parse::<f32>().ok())
            })
        })
        .map(|v| v.clamp(0.8, 2.0))
        .unwrap_or(SCALE_DEFAULT)
}

pub fn save_ui(mode: ThemeMode, scale: f32) {
    let path = ui_config();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(
        path,
        format!(
            "theme={}\nscale={:.2}\n",
            mode.name().to_lowercase(),
            scale
        ),
    );
}

pub fn load_mode() -> ThemeMode {
    std::fs::read_to_string(ui_config())
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("theme=").map(ThemeMode::parse))
        })
        .unwrap_or(ThemeMode::System)
}

pub fn install_style(ctx: &egui::Context) {
    let p = pal();
    let mut style = (*ctx.style()).clone();
    use egui::{FontFamily, TextStyle};
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(21.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(8.0, 7.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.visuals = if is_dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.visuals.override_text_color = Some(p.ink);
    style.visuals.panel_fill = p.bg;
    style.visuals.window_fill = p.card;
    style.visuals.extreme_bg_color = if is_dark() {
        Color32::from_rgb(18, 21, 25)
    } else {
        Color32::from_rgb(246, 247, 249)
    };
    style.visuals.selection.bg_fill = p.accent_soft;
    style.visuals.selection.stroke = Stroke::new(1.0, p.accent);
    style.visuals.widgets.inactive.bg_fill = if is_dark() {
        Color32::from_rgb(41, 46, 54)
    } else {
        Color32::from_rgb(247, 248, 250)
    };
    style.visuals.widgets.inactive.weak_bg_fill = style.visuals.widgets.inactive.bg_fill;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line);
    style.visuals.widgets.inactive.rounding = Rounding::same(4.0);
    style.visuals.widgets.hovered.bg_fill = p.accent_soft;
    style.visuals.widgets.hovered.weak_bg_fill = p.accent_soft;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    style.visuals.widgets.hovered.rounding = Rounding::same(4.0);
    style.visuals.widgets.active.bg_fill = p.accent;
    style.visuals.widgets.active.weak_bg_fill = p.accent;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, p.accent_dark);
    style.visuals.widgets.active.rounding = Rounding::same(4.0);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    style.visuals.window_rounding = Rounding::same(8.0);
    style.visuals.menu_rounding = Rounding::same(6.0);
    ctx.set_style(style);
}

pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(10.5)
            .strong()
            .color(pal().muted),
    );
}

pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::none()
        .fill(pal().card)
        .rounding(Rounding::same(7.0))
        .stroke(Stroke::new(1.0, pal().line))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Pick a colour that only exists as a light value in the original design.
fn surface(light: Color32, dark: Color32) -> Color32 {
    if is_dark() {
        dark
    } else {
        light
    }
}

pub fn button_widget(text: &str, primary: bool, enabled: bool) -> egui::Button<'static> {
    let (fill, fg, stroke) = if primary {
        (pal().accent, Color32::WHITE, Stroke::new(1.0, pal().accent_dark))
    } else {
        (
            surface(
                Color32::from_rgb(247, 248, 250),
                Color32::from_rgb(45, 50, 58),
            ),
            pal().ink,
            Stroke::new(1.0, pal().line),
        )
    };
    let mut b = egui::Button::new(egui::RichText::new(text.to_owned()).size(13.5).color(fg))
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(4.0))
        .min_size(Vec2::new(88.0, 30.0));
    if !enabled {
        b = b.fill(surface(
            Color32::from_rgb(235, 237, 240),
            Color32::from_rgb(36, 40, 47),
        ));
    }
    b
}

pub fn button(ui: &mut Ui, text: &str, primary: bool, enabled: bool) -> Response {
    ui.add_enabled(enabled, button_widget(text, primary, enabled))
}

pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    width: f32,
    color: Color32,
) -> Response {
    let height = 22.0;
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    // Guard every divisor: a zero-width slider or a NaN value would otherwise
    // poison the layout and trip egui's `!avail.any_nan()` assertion.
    let span = (max - min).abs().max(f32::EPSILON);
    if (response.dragged() || response.clicked()) && response.enabled() {
        if let Some(pos) = response.interact_pointer_pos() {
            let drag_span = (rect.width() - 16.0).max(1.0);
            let t = ((pos.x - rect.left() - 8.0) / drag_span).clamp(0.0, 1.0);
            *value = min + t * (max - min);
            // Without this every caller's `.changed()` stays false and values
            // derived from a local copy (the deadzone slider) snap back.
            response.mark_changed();
        }
    }
    let cy = rect.center().y;
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + 8.0, cy - 2.5),
        Pos2::new(rect.right() - 8.0, cy + 2.5),
    );
    let t = if value.is_finite() {
        ((*value - min) / span).clamp(0.0, 1.0)
    } else {
        *value = min;
        0.0
    };
    let knob_x = track.left() + t * track.width();
    let p = ui.painter();
    p.rect_filled(track, Rounding::same(3.0), pal().track);
    p.rect_filled(
        Rect::from_min_max(track.min, Pos2::new(knob_x, track.max.y)),
        Rounding::same(3.0),
        color,
    );
    let knob = Pos2::new(knob_x, cy);
    let ring = if response.hovered() || response.dragged() {
        pal().accent_dark
    } else {
        Color32::from_rgb(120, 132, 148)
    };
    p.circle_filled(knob, 7.5, Color32::WHITE);
    p.circle_stroke(knob, 7.5, Stroke::new(2.0, color));
    p.circle_stroke(knob, 9.0, Stroke::new(1.0, ring.gamma_multiply(0.35)));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn small(ui: &mut Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text).size(11.0).color(pal().muted));
}

/// A horizontal bar, like the axis read-outs in the original Test Input page.
pub fn axis_bar(ui: &mut Ui, value: f32, label: &str, width: f32, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(11.5).color(pal().muted));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, Rounding::same(3.0), pal().track);
        let t = value.clamp(0.0, 1.0);
        p.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width() * t, rect.height())),
            Rounding::same(3.0),
            color,
        );
        p.circle_filled(
            Pos2::new(rect.left() + rect.width() * t, rect.center().y),
            3.0,
            Color32::WHITE,
        );
    });
}

/// A read-only bar that fills from the centre, so -1..1 (a steering axis)
/// reads as "left of centre" or "right of centre" instead of "half full".
pub fn bipolar_bar(ui: &mut Ui, value: f32, label: &str, width: f32, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(11.5).color(pal().muted));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, Rounding::same(3.0), pal().track);
        let centre = rect.center().x;
        p.line_segment(
            [
                Pos2::new(centre, rect.top()),
                Pos2::new(centre, rect.bottom()),
            ],
            Stroke::new(1.0, pal().line),
        );
        let v = value.clamp(-1.0, 1.0);
        let x = centre + v * rect.width() * 0.5;
        let (a, b) = if x >= centre { (centre, x) } else { (x, centre) };
        p.rect_filled(
            Rect::from_min_max(Pos2::new(a, rect.top()), Pos2::new(b, rect.bottom())),
            Rounding::same(3.0),
            color,
        );
    });
}

/// Draw a steering wheel. `angle_deg` is the wheel rotation, 0 = straight ahead.
pub fn draw_wheel(ui: &mut Ui, size: Vec2, angle_deg: f32, leds_on: bool, glow: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let p = ui.painter();
    let c = rect.center();
    let r = (rect.width().min(rect.height()) * 0.5 - 6.0).max(24.0);

    // shadow
    p.circle_filled(
        c + Vec2::new(2.0, 5.0),
        r + 2.0,
        Color32::from_black_alpha(26),
    );
    // rim
    p.circle_filled(c, r, Color32::from_rgb(38, 43, 52));
    p.circle_filled(c, r - 16.0, Color32::from_rgb(52, 58, 68));
    p.circle_filled(c, r - 24.0, pal().bg);
    // subtle rim highlight
    p.circle_stroke(
        c,
        r - 6.0,
        Stroke::new(1.5, Color32::from_rgb(120, 128, 140)),
    );

    // spokes
    let a = angle_deg.to_radians();
    for k in 0..3 {
        let ang = a + std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 3.0;
        let dir = Vec2::new(ang.cos(), ang.sin());
        let from = c + dir * (r * 0.20);
        let to = c + dir * (r - 22.0);
        p.line_segment([from, to], Stroke::new(11.0, pal().metal));
        p.line_segment([from, to], Stroke::new(6.0, pal().metal_light));
    }
    // hub
    p.circle_filled(c, r * 0.25, Color32::from_rgb(30, 34, 40));
    p.circle_stroke(c, r * 0.25, Stroke::new(2.0, pal().accent));
    p.circle_filled(c, r * 0.10, pal().accent);

    // top marker
    let top = c + Vec2::new(0.0, -r + 2.0);
    p.circle_filled(top, 4.5, pal().accent);
    if glow > 0.0 {
        p.circle_stroke(
            c,
            r + 3.0,
            Stroke::new(2.0, pal().accent_soft.gamma_multiply(glow)),
        );
    }

    // LED strip (the HexaLed feature)
    let leds = 6;
    let led_y = c.y - r * 0.62;
    for i in 0..leds {
        let t = (i as f32 + 0.5) / leds as f32 - 0.5;
        let pos = Pos2::new(c.x + t * r * 0.95, led_y);
        let col = if leds_on {
            match i % 3 {
                0 => Color32::from_rgb(80, 200, 120),
                1 => Color32::from_rgb(255, 190, 70),
                _ => Color32::from_rgb(90, 160, 255),
            }
        } else {
            Color32::from_rgb(58, 64, 74)
        };
        p.circle_filled(pos, 3.0, col);
        if leds_on {
            p.circle_filled(pos, 6.0, col.gamma_multiply(0.18));
        }
    }
    response.on_hover_cursor(egui::CursorIcon::Grab)
}

pub fn tag(ui: &mut Ui, text: &str, color: Color32) {
    let painter = ui.painter().clone();
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), color);
    let pad = Vec2::new(8.0, 3.0);
    let size = galley.size() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    painter.rect_filled(
        rect,
        Rounding::same(size.y * 0.5),
        color.gamma_multiply(0.13),
    );
    painter.galley(rect.min + pad, galley, color);
}

pub fn dot(ui: &mut Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(radius * 2.0, radius * 2.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}

pub fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(0.0), pal().line);
}

pub fn pill_button(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let fill = if selected {
        pal().accent
    } else {
        surface(
            Color32::from_rgb(247, 248, 250),
            Color32::from_rgb(45, 50, 58),
        )
    };
    let fg = if selected { Color32::WHITE } else { pal().ink };
    let stroke = if selected {
        Stroke::new(1.0, pal().accent_dark)
    } else {
        Stroke::new(1.0, pal().line)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(text).size(12.5).color(fg))
            .fill(fill)
            .stroke(stroke)
            .rounding(Rounding::same(13.0))
            .min_size(Vec2::new(0.0, 24.0)),
    )
}

pub fn tab_button(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let h = 34.0;
    let w = 132.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let p = ui.painter();
    let fill = if selected {
        pal().card
    } else {
        surface(
            Color32::from_rgb(240, 242, 245),
            Color32::from_rgb(36, 40, 47),
        )
    };
    let mut r = rect;
    if selected {
        r.max.y += 1.0;
    }
    p.rect_filled(
        r,
        Rounding {
            nw: 6.0,
            ne: 6.0,
            sw: 0.0,
            se: 0.0,
        },
        fill,
    );
    if selected {
        p.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), 3.0)),
            Rounding {
                nw: 6.0,
                ne: 6.0,
                sw: 0.0,
                se: 0.0,
            },
            pal().accent,
        );
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(13.0),
        if selected { pal().accent_dark } else { pal().muted },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
