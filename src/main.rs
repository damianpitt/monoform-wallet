#![forbid(unsafe_code)]

use eframe::egui::{self, Color32, Stroke};

#[cfg(all(feature = "core-rpc", any(target_os = "macos", target_os = "linux")))]
mod watch_only;
#[cfg(all(feature = "core-rpc", any(target_os = "macos", target_os = "linux")))]
use watch_only::MonoformApp;

#[cfg(not(all(feature = "core-rpc", any(target_os = "macos", target_os = "linux"))))]
#[derive(Default)]
struct MonoformApp;

#[cfg(not(all(feature = "core-rpc", any(target_os = "macos", target_os = "linux"))))]
impl eframe::App for MonoformApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("MONOFORM");
            ui.label("Sovereign Money, One form, Three Chains");
            ui.separator();
            ui.label("Offline interface preview");
            ui.label("Wallet connection is available in the testnet watch-only edition.");
        });
    }
}

const INK: Color32 = Color32::from_rgb(238, 238, 235);
const MUTED: Color32 = Color32::from_rgb(152, 152, 146);
const PANEL: Color32 = Color32::from_rgb(27, 27, 26);
const BORDER: Color32 = Color32::from_rgb(55, 55, 52);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 720.0])
            .with_min_inner_size([360.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        // Paths, consent, balances and window state stay out of UI persistence.
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "Monoform Wallet",
        options,
        Box::new(|creation| {
            set_style(creation);
            Ok(Box::<MonoformApp>::default())
        }),
    )
}

fn set_style(creation: &eframe::CreationContext<'_>) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(13, 13, 12);
    visuals.window_fill = PANEL;
    visuals.faint_bg_color = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = PANEL;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(38, 38, 36);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
    visuals.override_text_color = Some(INK);
    creation.egui_ctx.set_visuals(visuals);
}
