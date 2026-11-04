// Warnings about untyped float literals are accepted for this UI code.
#![allow(float_literal_f32_fallback)]

mod bridge;
mod device;
mod ffb;
mod mechanics;
mod panel;
mod paths;
mod protondb;
mod setup;
mod steam;
mod theme;

use eframe::egui::{self, Vec2};
use panel::{Panel, PanelAction};
use setup::Setup;
use std::path::PathBuf;

#[derive(PartialEq, Eq)]
enum Mode {
    Panel,
    Setup,
}

struct App {
    mode: Mode,
    panel: Panel,
    setup: Setup,
    shot_path: Option<PathBuf>,
    frame: u64,
    shot_requested: bool,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        shot_path: Option<PathBuf>,
        tab: usize,
        effect: Option<usize>,
        start_setup: bool,
        setup_page: usize,
        show_update: bool,
        diagnose: bool,
        notes: bool,
    ) -> Self {
        theme::install_style(&cc.egui_ctx);
        cc.egui_ctx.set_zoom_factor(theme::load_scale());
        let mut panel = Panel::new();
        panel.tab = match tab {
            1 => panel::Tab::Forces,
            2 => panel::Tab::Settings,
            3 => panel::Tab::Profiles,
            _ => panel::Tab::Input,
        };
        if let Some(i) = effect {
            panel.effect = Some(panel::Play {
                idx: i.min(11),
                t: 0.55,
            });
            panel.pending_effect = Some((i.min(11), 0.55));
        }
        if show_update {
            panel.show_update = true;
        }
        if diagnose {
            panel.open_diagnostics();
        }
        if notes {
            panel.show_notes = true;
        }
        let mut setup = Setup::new();
        setup.page = match setup_page {
            1 => setup::Page::License,
            _ => setup::Page::Language,
        };
        Self {
            mode: if start_setup {
                Mode::Setup
            } else {
                Mode::Panel
            },
            panel,
            setup,
            shot_path,
            frame: 0,
            shot_requested: false,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.05);
        self.panel.update(dt, ctx);
        self.setup.update(dt);

        // --screenshot: draw about 120 frames, save the window, exit
        self.frame += 1;
        if self.shot_path.is_some() {
            if self.frame == 120 && !self.shot_requested {
                self.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(img) = shot {
                if let Some(path) = &self.shot_path {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let mut buf = Vec::with_capacity(img.pixels.len() * 4);
                    for p in &img.pixels {
                        buf.push(p.r());
                        buf.push(p.g());
                        buf.push(p.b());
                        buf.push(p.a());
                    }
                    let _ = image::save_buffer(
                        path,
                        &buf,
                        img.width() as u32,
                        img.height() as u32,
                        image::ColorType::Rgba8,
                    );
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        match self.mode {
            Mode::Panel => {
                egui::TopBottomPanel::bottom("status")
                    .exact_height(44.0)
                    .frame(
                        egui::Frame::none()
                            .fill(theme::pal().card)
                            .stroke(egui::Stroke::new(1.0, theme::pal().line))
                            .inner_margin(egui::Margin {
                                left: 20.0,
                                right: 20.0,
                                top: 6.0,
                                bottom: 6.0,
                            }),
                    )
                    .show(ctx, |ui| {
                        self.panel.status_bar(ui);
                    });
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(theme::pal().bg)
                            .inner_margin(egui::Margin {
                                left: 20.0,
                                right: 20.0,
                                top: 0.0,
                                bottom: 0.0,
                            }),
                    )
                    .show(ctx, |ui| {
                        if let Some(action) = self.panel.ui(ui) {
                            match action {
                                PanelAction::Setup => {
                                    self.setup.restart();
                                    self.mode = Mode::Setup;
                                }
                            }
                        }
                    });
            }
            Mode::Setup => {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(theme::pal().bg)
                            .inner_margin(egui::Margin::same(16.0)),
                    )
                    .show(ctx, |ui| {
                        if self.setup.ui(ui) {
                            self.mode = Mode::Panel;
                            self.panel.status =
                                String::from("Setup finished. Control panel is ready.");
                        }
                    });
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    // `tmx-t150-panel --tmx-switch` sends the Xbox-to-PC mode command and exits.
    // The panel uses the same code path through polkit, and a udev rule can
    // call it directly.
    if std::env::args().any(|a| a == "--tmx-switch") {
        return match device::switch_gip_to_pc() {
            Ok(message) => {
                println!("{message}");
                Ok(())
            }
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(1);
            }
        };
    }

    // `tmx-t150-panel --bridge-check` starts the virtual pad for a moment and
    // reports whether /dev/uinput accepted it.  Useful for support: it needs
    // no window and no game.
    if std::env::args().any(|a| a == "--bridge-check") {
        let Some(info) = device::discover() else {
            eprintln!("no Thrustmaster wheel found");
            std::process::exit(1);
        };
        let mut b = bridge::Bridge::new();
        b.start(&info);
        std::thread::sleep(std::time::Duration::from_millis(900));
        b.poll();
        println!("{}", b.live.message);
        println!("running: {}", b.live.running);
        b.stop();
        return Ok(());
    }

    // `tmx-t150-panel --protondb <appid>` prints the ProtonDB verdict and exits.
    if let Some(pos) = std::env::args().position(|a| a == "--protondb") {
        let appid = std::env::args()
            .nth(pos + 1)
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        match protondb::spawn(appid).recv().ok().and_then(|(_, status)| status) {
            Some(status) => println!(
                "{appid}: {} ({} reports, trending {})",
                status.tier.name(),
                status.total,
                status.trending.name()
            ),
            None => println!("{appid}: no ProtonDB answer"),
        }
        return Ok(());
    }

    // `tmx-t150-panel --scan` prints what the Steam library scanner finds.
    if std::env::args().any(|a| a == "--scan") {
        let (games, problems) = steam::scan();
        let mut racing = 0;
        for game in games.iter().filter(|g| steam::is_racing(&g.name)) {
            racing += 1;
            let s = steam::suggest(&game.name);
            println!(
                "{:>8}  {:<42} {:<6} rot {:.0} gain {:.2} curve {:.2} dz {:.0}%  [{}]",
                game.appid,
                game.name,
                if s.bridge { "bridge" } else { "native" },
                s.rotation,
                s.gain,
                s.curve,
                s.deadzone * 100.0,
                s.source
            );
        }
        println!("{} racing game(s) in {} installed game(s)", racing, games.len());
        for problem in &problems {
            println!("unreadable: {problem}");
        }
        return Ok(());
    }

    let mut shot_path = None;
    let mut tab = 0usize;
    let mut effect = None;
    let mut start_setup = false;
    let mut setup_page = 0usize;
    let mut show_update = false;
    let mut diagnose = false;
    let mut notes = false;
    let mut theme_choice: Option<theme::ThemeMode> = None;
    let mut size = Vec2::new(1240.0, 880.0);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => shot_path = args.next().map(PathBuf::from),
            "--size" => {
                if let Some(v) = args.next() {
                    if let Some((w, h)) = v.split_once('x') {
                        if let (Ok(w), Ok(h)) = (w.parse::<f32>(), h.parse::<f32>()) {
                            size = Vec2::new(w.max(720.0), h.max(520.0));
                        }
                    }
                }
            }
            "--tab" => {
                tab = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "--effect" => {
                effect = args.next().and_then(|v| v.parse::<usize>().ok());
            }
            "--setup" => start_setup = true,
            "--update" => show_update = true,
            "--diagnose" => diagnose = true,
            "--notes" => notes = true,
            "--theme" => {
                theme_choice = args.next().map(|v| match v.to_lowercase().as_str() {
                    "dark" => theme::ThemeMode::Dark,
                    "light" => theme::ThemeMode::Light,
                    _ => theme::ThemeMode::System,
                });
            }
            "--setup-page" => {
                setup_page = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                start_setup = true;
            }
            _ => {}
        }
    }

    theme::set_mode(theme_choice.unwrap_or_else(theme::load_mode));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("tmx-t150-panel")
            .with_inner_size(size)
            .with_min_inner_size(Vec2::new(900.0, 640.0))
            .with_title("TMX/T150 Control Panel"),
        ..Default::default()
    };

    eframe::run_native(
        "TMX/T150 Control Panel",
        options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                shot_path.clone(),
                tab,
                effect,
                start_setup,
                setup_page,
                show_update,
                diagnose,
                notes,
            )))
        }),
    )
}
