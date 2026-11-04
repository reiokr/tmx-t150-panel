//! The first-run wizard: language and licence, nothing else.
//!
//! The original package ships an InstallShield wizard that installs drivers,
//! services and shortcuts.  This build is a reimplementation that is started
//! from its own folder, so there is nothing to install: the earlier pages
//! (destination folder, setup type, component list, progress bar) only
//! pretended to do work and have been removed.
//!
//! Installation for this project happens outside the app: see the repository
//! README (`desktop/install.sh` adds the menu entry).

use crate::theme::{self, button_widget, pal};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Rounding, Stroke, Ui, Vec2,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Language,
    License,
}

pub struct Setup {
    pub page: Page,
    pub lang: usize,
    pub accepted: bool,
    pub t: f32,
}

impl Setup {
    pub fn new() -> Self {
        Self {
            page: Page::Language,
            lang: 0,
            accepted: false,
            t: 0.0,
        }
    }

    pub fn restart(&mut self) {
        self.page = Page::Language;
        self.accepted = false;
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
    }

    fn tr(&self, en: &str, et: &str) -> String {
        if self.lang == 1 {
            et.to_string()
        } else {
            en.to_string()
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> bool {
        let mut done = false;
        let area = ui.max_rect();
        let card = Vec2::new(
            760.0_f32.min(area.width() - 32.0),
            680.0_f32.min(area.height() - 32.0),
        );
        let card_rect = Rect::from_center_size(area.center(), card);
        let p = ui.painter().clone();
        p.add(
            egui::epaint::Shadow {
                offset: Vec2::new(0.0, 10.0),
                blur: 28.0,
                spread: 0.0,
                color: Color32::from_black_alpha(45),
            }
            .as_shape(card_rect, Rounding::same(8.0)),
        );
        p.rect(
            card_rect,
            Rounding::same(8.0),
            pal().card,
            Stroke::new(1.0, pal().line),
        );
        let banner_rect = Rect::from_min_size(card_rect.min, Vec2::new(186.0, card.y));
        self.paint_banner(&p, banner_rect);

        let content = Rect::from_min_max(
            Pos2::new(card_rect.left() + 186.0 + 26.0, card_rect.top() + 20.0),
            Pos2::new(card_rect.right() - 26.0, card_rect.bottom() - 18.0),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content), |ui| {
            ui.set_clip_rect(card_rect);
            ui.set_width(content.width());
            ui.set_height(content.height());
            done = self.page_body(ui);
        });
        done
    }

    fn paint_banner(&self, p: &egui::Painter, rect: Rect) {
        let p = p.with_clip_rect(rect);
        let steps = 60;
        let top = Color32::from_rgb(20, 54, 98);
        let bottom = Color32::from_rgb(52, 118, 178);
        for i in 0..steps {
            let f = i as f32 / steps as f32;
            let y0 = rect.top() + rect.height() * f;
            let y1 = y0 + rect.height() / steps as f32 + 1.0;
            let c = Color32::from_rgb(
                (top.r() as f32 + (bottom.r() as f32 - top.r() as f32) * f) as u8,
                (top.g() as f32 + (bottom.g() as f32 - top.g() as f32) * f) as u8,
                (top.b() as f32 + (bottom.b() as f32 - top.b() as f32) * f) as u8,
            );
            let rounding = if i == 0 {
                Rounding {
                    nw: 8.0,
                    ne: 0.0,
                    sw: 0.0,
                    se: 0.0,
                }
            } else if i == steps - 1 {
                Rounding {
                    nw: 0.0,
                    ne: 0.0,
                    sw: 8.0,
                    se: 0.0,
                }
            } else {
                Rounding::ZERO
            };
            p.rect_filled(
                Rect::from_min_max(Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y1)),
                rounding,
                c,
            );
        }

        // A drawn steering wheel stands in for the vendor artwork.
        let c = Pos2::new(rect.left() + 30.0, rect.top() + 214.0);
        let r = 116.0;
        let ink = Color32::from_white_alpha(16);
        p.circle_stroke(c, r, Stroke::new(20.0, ink));
        p.circle_stroke(c, r - 28.0, Stroke::new(2.0, Color32::from_white_alpha(22)));
        for k in 0..3 {
            let a = std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 3.0;
            let d = Vec2::new(a.cos(), a.sin());
            p.line_segment([c + d * 24.0, c + d * (r - 10.0)], Stroke::new(13.0, ink));
        }
        p.circle_filled(c, 28.0, Color32::from_white_alpha(20));
        p.circle_stroke(c, 28.0, Stroke::new(2.0, Color32::from_white_alpha(36)));

        p.circle_filled(
            Pos2::new(rect.left() + 34.0, rect.bottom() - 80.0),
            13.0,
            Color32::WHITE,
        );
        p.circle_filled(
            Pos2::new(rect.left() + 34.0, rect.bottom() - 80.0),
            7.0,
            Color32::from_rgb(30, 76, 132),
        );
        p.text(
            Pos2::new(rect.left() + 20.0, rect.bottom() - 54.0),
            Align2::LEFT_BOTTOM,
            "TTRS Racing Wheel",
            FontId::proportional(14.0),
            Color32::WHITE,
        );
        p.text(
            Pos2::new(rect.left() + 20.0, rect.bottom() - 36.0),
            Align2::LEFT_BOTTOM,
            "Control panel 4.24.2.0 layout",
            FontId::proportional(10.0),
            Color32::from_white_alpha(170),
        );
    }

    fn page_body(&mut self, ui: &mut Ui) -> bool {
        let (title, subtitle) = match self.page {
            Page::Language => (
                self.tr("Choose language", "Vali keel"),
                self.tr(
                    "This choice applies to the wizard pages only; the control panel itself is in English.",
                    "Valik kehtib ainult viisardi lehtedele; juhtpaneel ise on inglise keeles.",
                ),
            ),
            Page::License => (
                self.tr("Licence and legal notice", "Litsents ja õigusteave"),
                self.tr(
                    "Read this before using the panel.",
                    "Loe seda enne paneeli kasutamist.",
                ),
            ),
        };

        let area = ui.max_rect();
        let header = Rect::from_min_max(area.min, Pos2::new(area.right(), area.top() + 54.0));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(header), |ui| {
                ui.label(
                    RichText::new(title)
                        .size(17.0)
                        .strong()
                        .color(pal().ink),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(subtitle)
                        .size(11.0)
                        .color(pal().muted),
                );
            },
        );

        let body_rect = Rect::from_min_max(
            Pos2::new(area.left(), area.top() + 62.0),
            Pos2::new(area.right(), area.bottom() - 52.0),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body_rect), |ui| match self.page {
            Page::Language => self.language_body(ui),
            Page::License => self.license_body(ui),
        });

        // Footer: Back / Next-Finish / Cancel, right aligned.
        let mut done = false;
        let by = area.bottom() - 36.0;
        let bw = 104.0;
        let bh = 30.0;
        let gap = 8.0;
        let mut right = area.right();
        let slot = |right: &mut f32| {
            let r = Rect::from_min_size(
                Pos2::new(*right - bw, by - bh * 0.5),
                Vec2::new(bw, bh),
            );
            *right -= bw + gap;
            r
        };

        let (label, enabled) = match self.page {
            Page::Language => (self.tr("Next", "Edasi"), true),
            Page::License => (self.tr("Finish", "Lõpeta"), self.accepted),
        };
        let primary = slot(&mut right);
        if ui
            .put(primary, button_widget(&label, true, enabled))
            .clicked()
            && enabled
        {
            match self.page {
                Page::Language => self.page = Page::License,
                Page::License => done = true,
            }
        }

        if self.page == Page::License {
            let back = slot(&mut right);
            if ui.put(back, button_widget(&self.tr("Back", "Tagasi"), false, true)).clicked() {
                self.page = Page::Language;
            }
        }

        let cancel = slot(&mut right);
        if ui
            .put(cancel, button_widget(&self.tr("Cancel", "Loobu"), false, true))
            .clicked()
        {
            done = true;
        }

        done
    }

    fn language_body(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        for (index, name) in ["English", "Eesti"].iter().enumerate() {
            if ui
                .radio(self.lang == index, *name)
                .clicked()
            {
                self.lang = index;
            }
            ui.add_space(4.0);
        }
        ui.add_space(8.0);
        theme::small(
            ui,
            self.tr(
                "The original package ships 14 languages for its installer and panel. \
                 This rebuild is English-only; the wizard keeps the two languages its own \
                 pages are written in.",
                "Originaalpaketis on paigaldajale ja paneelile 14 keelt. See versioon on \
                 inglise keeles; viisardil on alles kaks keelt, milles tema lehed kirjutatud on.",
            ),
        );
    }

    fn license_body(&mut self, ui: &mut Ui) {
        let text_rect = Rect::from_min_size(
            ui.max_rect().min,
            Vec2::new(ui.max_rect().width(), ui.max_rect().height() - 42.0),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(text_rect), |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(
                    egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                )
                .show(ui, |ui| {
                    for paragraph in [
                        "Independent reimplementation",
                        "This program re-creates the function of a racing wheel control panel. It is not \
                         affiliated with, endorsed by or supported by Thrustmaster or Guillemot. Product \
                         names such as TMX, T150 and Thrustmaster are trademarks of their owners and are \
                         used here only to say which hardware the program works with.",
                        "",
                        "No vendor files are included",
                        "The repository contains no original driver binaries, icons, logos, effect presets \
                         or installer payloads. Everything drawn or played here was written for this \
                         project. If you installed the Thrustmaster driver package on Windows, that is a \
                         separate product under its own licence.",
                        "",
                        "Kernel driver",
                        "Force feedback on Linux comes from the hid-t150 kernel module, which is licensed \
                         under the GNU GPL version 2 by its authors. It is distributed as a separate \
                         component and keeps its own licence.",
                        "",
                        "What this program writes",
                        "Profiles, the theme and the ProtonDB cache go to ~/.config/ttrs-wheel/. A menu \
                         entry is only created when you run desktop/install.sh yourself. Game files are \
                         edited only by the changes you apply explicitly (for example the DiRT 3 action \
                         map); backups are kept next to the originals.",
                        "",
                        "No warranty, and the code licence",
                        "The software comes as is, without warranty of any kind; you use it at your own \
                         risk, including any change you make to a game's configuration files. The source \
                         in this repository is released under the licence stated in its LICENSE file.",
                    ] {
                        if paragraph.is_empty() {
                            ui.add_space(6.0);
                        } else if paragraph.starts_with("Independent")
                            || paragraph.starts_with("No vendor")
                            || paragraph.starts_with("Kernel")
                            || paragraph.starts_with("What this")
                            || paragraph.starts_with("No warranty")
                        {
                            ui.label(
                                RichText::new(paragraph)
                                    .size(12.0)
                                    .strong()
                                    .color(pal().ink),
                            );
                        } else {
                            ui.label(RichText::new(paragraph).size(11.5).color(pal().ink));
                        }
                        ui.add_space(4.0);
                    }
                });
        });
        let accept_rect = Rect::from_min_max(
            Pos2::new(ui.max_rect().left(), ui.max_rect().bottom() - 26.0),
            ui.max_rect().max,
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(accept_rect), |ui| {
            if ui
                .radio(
                    self.accepted,
                    self.tr(
                        "I have read and accept these terms",
                        "Olen tingimused läbi lugenud ja nõustun",
                    ),
                )
                .clicked()
            {
                self.accepted = true;
            }
        });
    }
}

impl Default for Setup {
    fn default() -> Self {
        Self::new()
    }
}
