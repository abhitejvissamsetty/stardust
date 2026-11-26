//! The installer's window: welcome, the licence agreement, where to install, the
//! install itself, and done. It takes its look from Stardust's welcome page: white,
//! with the workspace palette as soft light at the edges.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout,
    Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};

use crate::install::{self, Options, SharedProgress};
use crate::payload::Payload;

const PAPER: Color32 = Color32::WHITE;
const INK: Color32 = Color32::from_rgb(0x15, 0x16, 0x1a);
const INK_2: Color32 = Color32::from_rgb(0x55, 0x59, 0x66);
const INK_3: Color32 = Color32::from_rgb(0x8b, 0x8f, 0x9b);
const HAIR: Color32 = Color32::from_rgb(0xeb, 0xec, 0xf0);
const HAIR_2: Color32 = Color32::from_rgb(0xd9, 0xdb, 0xe1);
const BLUE: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xed);
const VIOLET: Color32 = Color32::from_rgb(0x8b, 0x5c, 0xf6);
const PINK: Color32 = Color32::from_rgb(0xd6, 0x47, 0x9a);
const TEAL: Color32 = Color32::from_rgb(0x14, 0xa3, 0xa3);
const AMBER: Color32 = Color32::from_rgb(0xd9, 0x8e, 0x1f);
const RED: Color32 = Color32::from_rgb(0xc5, 0x36, 0x2c);

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ICON: &[u8] = include_bytes!("../../../assets/icon/stardust-256.png");
/// The welcome page's orb, still, from `assets/icon/orb-light.svg`.
const ORB: &[u8] = include_bytes!("../../../assets/icon/orb-light-320.png");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    Agreement,
    Location,
    Installing,
    Done,
}

const STEPS: [Step; 5] = [
    Step::Welcome,
    Step::Agreement,
    Step::Location,
    Step::Installing,
    Step::Done,
];

pub struct App {
    step: Step,
    payload: Result<Payload, String>,
    icon: Option<egui::TextureHandle>,
    orb: Option<egui::TextureHandle>,
    accepted: bool,
    dir: String,
    menu: bool,
    command: bool,
    apparmor: bool,
    needs_apparmor: bool,
    progress: SharedProgress,
    result: Option<Result<Vec<String>, String>>,
    open_after: bool,
}

/// Opens the installer window.
pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Install Stardust")
            .with_app_id("stardust-installer")
            .with_inner_size([860.0, 600.0])
            .with_min_inner_size([720.0, 540.0])
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Install Stardust",
        options,
        Box::new(|creation| Ok(Box::new(App::new(&creation.egui_ctx)))),
    )
}

fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    pixels.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba).then_some((info.width, info.height, pixels))
}

fn window_icon() -> egui::IconData {
    decode_png(ICON).map_or_else(egui::IconData::default, |(width, height, rgba)| {
        egui::IconData {
            rgba,
            width,
            height,
        }
    })
}

/// The system's own interface fonts, found through fontconfig, so the installer looks
/// at home on each desktop. egui's built-in fonts stay as fallbacks.
fn install_fonts(ctx: &egui::Context) {
    let find = |pattern: &str| -> Option<Vec<u8>> {
        let output = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", pattern])
            .output()
            .ok()?;
        let path = String::from_utf8(output.stdout).ok()?;
        let path = path.trim();
        let ok = path.ends_with(".ttf") || path.ends_with(".otf");
        ok.then(|| std::fs::read(path).ok()).flatten()
    };
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, pattern: &str, family: FontFamily| {
        if let Some(data) = find(pattern) {
            fonts
                .font_data
                .insert(name.to_owned(), Arc::new(FontData::from_owned(data)));
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, name.to_owned());
        }
    };
    add("ui", "sans-serif:weight=regular", FontFamily::Proportional);
    add(
        "ui-medium",
        "sans-serif:weight=medium",
        FontFamily::Name("medium".into()),
    );
    add(
        "display",
        "sans-serif:weight=light",
        FontFamily::Name("display".into()),
    );
    add(
        "serif-italic",
        "serif:italic",
        FontFamily::Name("serif".into()),
    );
    // Families with no system font fall back to the built-in one.
    for family in ["medium", "display", "serif"] {
        let fallback = fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        fonts
            .families
            .entry(FontFamily::Name(family.into()))
            .or_default()
            .extend(fallback);
    }
    ctx.set_fonts(fonts);
}

fn style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(18.0, 10.0);
        style.spacing.interact_size.y = 30.0;
        let visuals = &mut style.visuals;
        visuals.panel_fill = PAPER;
        visuals.window_fill = PAPER;
        visuals.extreme_bg_color = PAPER;
        visuals.override_text_color = Some(INK);
        visuals.selection.bg_fill = BLUE.gamma_multiply(0.2);
        visuals.selection.stroke = Stroke::new(1.0, BLUE);
        for (widget, fill) in [
            (&mut visuals.widgets.inactive, PAPER),
            (
                &mut visuals.widgets.hovered,
                Color32::from_rgb(0xf6, 0xf7, 0xf9),
            ),
            (
                &mut visuals.widgets.active,
                Color32::from_rgb(0xee, 0xf0, 0xf4),
            ),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = Stroke::new(1.0, HAIR_2);
            widget.corner_radius = CornerRadius::same(10);
            widget.fg_stroke = Stroke::new(1.5, INK);
        }
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, HAIR);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK_2);
    });
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        install_fonts(ctx);
        style(ctx);
        let texture = |name: &str, bytes: &[u8]| {
            decode_png(bytes).map(|(width, height, rgba)| {
                let size = [width as usize, height as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
            })
        };
        let icon = texture("icon", ICON);
        let orb = texture("orb", ORB);
        let needs_apparmor = install::needs_apparmor();
        Self {
            step: Step::Welcome,
            payload: Payload::find().map_err(|error| error.to_string()),
            icon,
            orb,
            accepted: false,
            dir: install::default_dir().to_string_lossy().into_owned(),
            menu: true,
            command: true,
            apparmor: needs_apparmor,
            needs_apparmor,
            progress: SharedProgress::default(),
            result: None,
            open_after: true,
        }
    }

    fn begin_install(&mut self) {
        let Ok(payload) = self.payload.clone() else {
            return;
        };
        let dir = PathBuf::from(self.dir.trim());
        install::start(
            Options {
                dir,
                menu: self.menu,
                command: self.command,
                apparmor: self.apparmor,
            },
            payload,
            self.progress.clone(),
        );
        self.step = Step::Installing;
    }

    fn finish(&self, ctx: &egui::Context) {
        if self.open_after && matches!(self.result, Some(Ok(_))) {
            let program = PathBuf::from(self.dir.trim()).join("stardust");
            let _ = std::process::Command::new(program)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

// ---- Drawing helpers -------------------------------------------------------------

fn font(size: f32, family: &str) -> FontId {
    match family {
        "" => FontId::proportional(size),
        name => FontId::new(size, FontFamily::Name(name.into())),
    }
}

/// A soft glow, drawn as rings that fade towards the edge.
fn glow(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32, strength: f32) {
    const RINGS: usize = 28;
    for ring in 0..RINGS {
        let t = 1.0 - ring as f32 / RINGS as f32;
        let alpha = strength / RINGS as f32 * 1.6;
        painter.circle_filled(center, radius * t, color.gamma_multiply(alpha));
    }
}

fn background(ui: &egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, PAPER);
    let size = rect.size();
    glow(
        painter,
        rect.left_top() + Vec2::new(size.x * 0.06, size.y * 0.02),
        size.x * 0.42,
        BLUE,
        0.08,
    );
    glow(
        painter,
        rect.right_top() + Vec2::new(-size.x * 0.04, size.y * 0.08),
        size.x * 0.36,
        PINK,
        0.07,
    );
    glow(
        painter,
        rect.right_bottom() + Vec2::new(-size.x * 0.08, -size.y * 0.02),
        size.x * 0.38,
        TEAL,
        0.07,
    );
    glow(
        painter,
        rect.left_bottom() + Vec2::new(size.x * 0.03, -size.y * 0.04),
        size.x * 0.34,
        AMBER,
        0.06,
    );
}

/// The dark pill that moves the installer on.
fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    let button = egui::Button::new(
        RichText::new(text)
            .font(font(15.0, "medium"))
            .color(Color32::WHITE),
    )
    .fill(if enabled { INK } else { INK_3 })
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(23))
    .min_size(Vec2::new(148.0, 46.0));
    let response = ui.add_enabled(enabled, button);
    if enabled && response.hovered() {
        // A violet halo under the button, as on the welcome page.
        let halo = response.rect.translate(Vec2::new(0.0, 5.0)).expand(2.0);
        ui.painter()
            .rect_filled(halo, 26.0, VIOLET.gamma_multiply(0.10));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn secondary(ui: &mut egui::Ui, text: &str) -> bool {
    let button = egui::Button::new(RichText::new(text).font(font(15.0, "")).color(INK_2))
        .fill(PAPER)
        .stroke(Stroke::new(1.0, HAIR_2))
        .corner_radius(CornerRadius::same(23))
        .min_size(Vec2::new(96.0, 46.0));
    ui.add(button).clicked()
}

fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(FontId::monospace(11.0))
            .color(VIOLET)
            .extra_letter_spacing(2.5),
    );
    ui.add_space(6.0);
}

fn heading(ui: &mut egui::Ui, plain: &str, accent: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.label(RichText::new(plain).font(font(38.0, "")).color(INK));
        ui.label(
            RichText::new(accent)
                .font(font(40.0, "serif"))
                .color(VIOLET)
                .italics(),
        );
    });
}

fn body(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(font(15.0, "")).color(INK_2));
}

/// Three-to-five dots joined by lines, lit up to the current step.
fn constellation(ui: &mut egui::Ui, at: usize) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(4.0 * 40.0 + 10.0, 12.0), Sense::hover());
    let painter = ui.painter();
    let y = rect.center().y;
    for index in 0..STEPS.len() {
        let x = rect.left() + 5.0 + index as f32 * 40.0;
        if index > 0 {
            let color = if index <= at { BLUE } else { HAIR };
            painter.line_segment(
                [Pos2::new(x - 35.0, y), Pos2::new(x - 5.0, y)],
                Stroke::new(1.0, color),
            );
        }
        let lit = index <= at;
        if index == at {
            painter.circle_filled(Pos2::new(x, y), 7.0, BLUE.gamma_multiply(0.15));
        }
        painter.circle_filled(
            Pos2::new(x, y),
            if index == at { 3.5 } else { 2.2 },
            if lit { BLUE } else { HAIR_2 },
        );
    }
}

/// The progress bar, filled with the palette.
fn progress_bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0xee, 0xf0, 0xf4));
    let filled = Rect::from_min_size(
        rect.min,
        Vec2::new(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    if filled.width() < 1.0 {
        return;
    }
    // A left-to-right blend from blue through violet to pink.
    let mut mesh = egui::Mesh::default();
    let stops = [(0.0, BLUE), (0.55, VIOLET), (1.0, PINK)];
    for window in stops.windows(2) {
        let (t0, c0) = window[0];
        let (t1, c1) = window[1];
        let x0 = rect.left() + rect.width() * t0;
        let x1 = (rect.left() + rect.width() * t1).min(filled.right());
        if x1 <= x0 {
            break;
        }
        let base = mesh.vertices.len() as u32;
        let c1 = lerp_color(c0, c1, (x1 - x0) / (rect.width() * (t1 - t0)));
        mesh.colored_vertex(Pos2::new(x0, rect.top()), c0);
        mesh.colored_vertex(Pos2::new(x1, rect.top()), c1);
        mesh.colored_vertex(Pos2::new(x1, rect.bottom()), c1);
        mesh.colored_vertex(Pos2::new(x0, rect.bottom()), c0);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PAPER)
        .stroke(Stroke::new(1.0, HAIR_2))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(18))
        .show(ui, add)
        .inner
}

fn check(ui: &mut egui::Ui, value: &mut bool, title: &str, detail: &str) {
    ui.horizontal(|ui| {
        ui.checkbox(value, "");
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).font(font(14.5, "medium")).color(INK));
            if !detail.is_empty() {
                ui.label(RichText::new(detail).font(font(13.0, "")).color(INK_3));
            }
        });
    });
}

/// The agreement, with its numbered headings in the ink colour.
fn agreement_text(ui: &mut egui::Ui) {
    for line in install::AGREEMENT.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            ui.add_space(4.0);
            continue;
        }
        let is_heading = trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
            && !trimmed.contains('.')
            || trimmed == "STARDUST END USER LICENCE AGREEMENT";
        let text = if is_heading {
            RichText::new(trimmed).font(font(13.0, "medium")).color(INK)
        } else {
            RichText::new(trimmed).font(font(13.0, "")).color(INK_2)
        };
        ui.add(egui::Label::new(text).wrap());
    }
}

// ---- Steps -----------------------------------------------------------------------

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.step == Step::Installing {
            let done = self
                .progress
                .lock()
                .expect("Failed to lock progress")
                .done
                .take();
            if let Some(result) = done {
                self.result = Some(result);
                self.step = Step::Done;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
        background(ui);
        let at = STEPS
            .iter()
            .position(|step| *step == self.step)
            .unwrap_or(0);

        let area = ui.max_rect().shrink2(Vec2::new(40.0, 26.0));
        let header = Rect::from_min_size(area.min, Vec2::new(area.width(), 28.0));
        let footer = Rect::from_min_max(Pos2::new(area.left(), area.bottom() - 50.0), area.max);
        let content = Rect::from_min_max(
            Pos2::new(area.left(), header.bottom() + 26.0),
            Pos2::new(area.right(), footer.top() - 14.0),
        );
        let place =
            |rect: Rect, layout: Layout| egui::UiBuilder::new().max_rect(rect).layout(layout);
        {
            // Header: the mark, the steps, the version.
            ui.scope_builder(place(header, Layout::left_to_right(Align::Center)), |ui| {
                if let Some(icon) = &self.icon {
                    ui.add(
                        egui::Image::new(icon)
                            .fit_to_exact_size(Vec2::splat(20.0))
                            .corner_radius(CornerRadius::same(5)),
                    );
                }
                ui.label(
                    RichText::new("Stardust")
                        .font(font(14.0, "medium"))
                        .color(INK),
                );
                let space = ui.available_width() - 170.0 - 80.0;
                ui.add_space((space / 2.0).max(0.0));
                constellation(ui, at);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("Version {VERSION}"))
                            .font(font(12.0, ""))
                            .color(INK_3),
                    );
                });
            });

            ui.scope_builder(
                place(content, Layout::top_down(Align::Min)),
                |ui| match self.step {
                    Step::Welcome => self.welcome(ui),
                    Step::Agreement => self.agreement(ui),
                    Step::Location => self.location(ui),
                    Step::Installing => self.installing(ui),
                    Step::Done => self.done(ui),
                },
            );

            // Footer: Back on the left of the primary button, both on the right.
            ui.scope_builder(place(footer, Layout::right_to_left(Align::Center)), |ui| {
                match self.step {
                    Step::Welcome => {
                        if primary(ui, "Get started", self.payload.is_ok()) {
                            self.step = Step::Agreement;
                        }
                        if secondary(ui, "Cancel") {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    Step::Agreement => {
                        if primary(ui, "Continue", self.accepted) {
                            self.step = Step::Location;
                        }
                        if secondary(ui, "Back") {
                            self.step = Step::Welcome;
                        }
                    }
                    Step::Location => {
                        let ready = !self.dir.trim().is_empty() && self.accepted;
                        if primary(ui, "Install", ready) {
                            self.begin_install();
                        }
                        if secondary(ui, "Back") {
                            self.step = Step::Agreement;
                        }
                    }
                    Step::Installing => {}
                    Step::Done => match &self.result {
                        Some(Ok(_)) => {
                            if primary(ui, "Finish", true) {
                                self.finish(&ctx);
                            }
                        }
                        _ => {
                            if primary(ui, "Try again", true) {
                                *self.progress.lock().expect("Failed to lock progress") = Default::default();
                                self.result = None;
                                self.step = Step::Location;
                            }
                            if secondary(ui, "Close") {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    },
                }
                if self.step == Step::Agreement {
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.label(RichText::new("You can read it again any time in LICENSE-AGREEMENT.txt, installed with Stardust.").font(font(12.0, "")).color(INK_3));
                    });
                }
            });
        }
    }
}

impl App {
    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(156.0), Sense::hover());
            // A soft violet glow behind the orb.
            glow(ui.painter(), rect.center(), 150.0, VIOLET, 0.06);
            if let Some(orb) = &self.orb {
                egui::Image::new(orb).paint_at(ui, rect);
            }
            ui.add_space(14.0);
            ui.label(RichText::new("Welcome to").font(font(20.0, "serif")).color(INK_2).italics());
            ui.label(RichText::new("Stardust").font(font(62.0, "display")).color(INK));
            ui.add_space(6.0);
            ui.label(
                RichText::new("A fast, private browser for people who make things.\nThis puts it on your computer in under a minute.")
                    .font(font(15.5, ""))
                    .color(INK_2),
            );
            ui.add_space(12.0);
            match &self.payload {
                Ok(payload) => {
                    let megabytes = payload.length as f64 / 1_000_000.0;
                    ui.label(
                        RichText::new(format!("Version {VERSION}  ·  {megabytes:.0} MB download, about 400 MB installed  ·  No administrator needed"))
                            .font(font(12.5, ""))
                            .color(INK_3),
                    );
                }
                Err(error) => {
                    ui.label(RichText::new(error).font(font(13.5, "")).color(RED));
                }
            }
        });
    }

    fn agreement(&mut self, ui: &mut egui::Ui) {
        eyebrow(ui, "01  —  The agreement");
        heading(ui, "Before you", "install.");
        body(
            ui,
            "Please read the Stardust End User Licence Agreement. Stardust's source code stays open source under the Mozilla Public License 2.0, and its other components' licences are installed with it in CREDITS.html.",
        );
        ui.add_space(8.0);
        let height = (ui.available_height() - 70.0).max(120.0);
        card(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(height - 36.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    agreement_text(ui);
                });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.accepted, "");
            ui.label(
                RichText::new("I have read and accept the Stardust End User Licence Agreement")
                    .font(font(14.5, "medium"))
                    .color(INK),
            );
        });
    }

    fn location(&mut self, ui: &mut egui::Ui) {
        eyebrow(ui, "02  —  Where it goes");
        heading(ui, "Choose a", "home.");
        body(
            ui,
            "Stardust installs just for you, in a folder you own, so it can keep itself up to date. Your bookmarks and settings are kept separately and stay if you reinstall.",
        );
        ui.add_space(14.0);
        ui.label(
            RichText::new("Install folder")
                .font(font(13.0, "medium"))
                .color(INK_2),
        );
        let field = egui::TextEdit::singleline(&mut self.dir)
            .font(font(15.0, ""))
            .margin(Margin::symmetric(14, 11))
            .desired_width(f32::INFINITY);
        ui.add(field);
        let dir = PathBuf::from(self.dir.trim());
        if install::is_running(&dir) {
            ui.label(
                RichText::new("Stardust is open from this folder. Close it before installing.")
                    .font(font(13.0, ""))
                    .color(RED),
            );
        } else if dir.join("stardust").exists() {
            ui.label(
                RichText::new(
                    "Stardust is already here. Installing replaces it, and keeps your data.",
                )
                .font(font(13.0, ""))
                .color(INK_3),
            );
        }
        ui.add_space(16.0);
        check(
            ui,
            &mut self.menu,
            "Add Stardust to the app menu",
            "With its icon, alongside your other browsers.",
        );
        ui.add_space(4.0);
        check(
            ui,
            &mut self.command,
            "Add the stardust command",
            "So you can start it from a terminal, from ~/.local/bin.",
        );
        if self.needs_apparmor {
            ui.add_space(4.0);
            check(
                ui,
                &mut self.apparmor,
                "Let Stardust's security sandbox start",
                "This system restricts sandboxes. Adding Stardust's rule asks for your password once.",
            );
        }
    }

    fn installing(&mut self, ui: &mut egui::Ui) {
        let (fraction, status) = {
            let progress = self.progress.lock().expect("Failed to lock progress");
            (progress.fraction, progress.status.clone())
        };
        ui.add_space(40.0);
        eyebrow(ui, "03  —  Installing");
        heading(ui, "Bringing the", "stars in.");
        body(ui, "This takes a moment. Please keep this window open.");
        ui.add_space(30.0);
        progress_bar(ui, fraction);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(status).font(font(13.0, "")).color(INK_3));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0}%", fraction * 100.0))
                        .font(font(13.0, "medium"))
                        .color(INK_2),
                );
            });
        });
    }

    fn done(&mut self, ui: &mut egui::Ui) {
        match self.result.clone() {
            Some(Ok(warnings)) => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(76.0), Sense::hover());
                    glow(ui.painter(), rect.center(), 70.0, BLUE, 0.10);
                    ui.painter().circle_filled(rect.center(), 30.0, INK);
                    let c = rect.center();
                    ui.painter().line(
                        vec![
                            c + Vec2::new(-11.0, 1.0),
                            c + Vec2::new(-3.0, 9.0),
                            c + Vec2::new(12.0, -8.0),
                        ],
                        Stroke::new(3.5, Color32::WHITE),
                    );
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("You're all set.")
                            .font(font(40.0, "display"))
                            .color(INK),
                    );
                    ui.add_space(4.0);
                    body(
                        ui,
                        "Stardust is installed. Find it in your app menu, or open it now.",
                    );
                    ui.add_space(18.0);
                });
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        let width = 210.0;
                        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                        ui.checkbox(
                            &mut self.open_after,
                            RichText::new("Open Stardust when I finish")
                                .font(font(14.5, "medium"))
                                .color(INK),
                        );
                    });
                    for warning in &warnings {
                        ui.label(RichText::new(warning).font(font(13.0, "")).color(AMBER));
                    }
                    ui.add_space(10.0);
                    let uninstall = PathBuf::from(self.dir.trim()).join("uninstall");
                    // Shown with ~ for the home folder, as people would type it.
                    let uninstall = match std::env::var_os("HOME") {
                        Some(home) => uninstall.strip_prefix(&home).map_or_else(
                            |_| uninstall.clone(),
                            |rest| PathBuf::from("~").join(rest),
                        ),
                        None => uninstall,
                    };
                    ui.label(
                        RichText::new(format!("To remove it later, run {}", uninstall.display()))
                            .font(font(12.0, ""))
                            .color(INK_3),
                    );
                });
            }
            Some(Err(error)) => {
                ui.add_space(30.0);
                eyebrow(ui, "Something went wrong");
                heading(ui, "Stardust isn't", "installed.");
                ui.label(RichText::new(error).font(font(15.0, "")).color(RED));
                ui.add_space(6.0);
                body(
                    ui,
                    "Nothing was changed. Fix the problem above, then try again.",
                );
            }
            None => {}
        }
    }
}
