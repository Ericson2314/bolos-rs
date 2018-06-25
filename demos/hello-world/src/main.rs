#![no_std]
#![no_main]

#[macro_use]
extern crate bolos;

use bolos::seproxyhal::MessageLoop;
use bolos::runtime::exit;
use bolos::ui;

struct AppState ();

impl AppState {
    fn new() -> AppState {
        AppState()
    }
}

impl ui::Delegate for AppState {
    type Action = ui::BasicAction;

    fn ui_version(&self) -> u16 {
        0
    }

    fn prepare_ui(&self, ctrl: &mut ui::Controller<Self::Action>) {
        ctrl.set_button_actions(ui::ButtonAction::Map{
            left: None,
            right: Some(ui::BasicAction::Previous),
            both: None,
        });

        ctrl.add_view(|| ui::RectangleView{
            frame: ui::Frame{ x: 0, y: 0, width: 128, height: 32 },
            fill: ui::FillMode::Fill,
            ..Default::default()
        }.into());
        ctrl.add_view(|| ui::IconView{
            position: ui::Position{ x: 3, y: 12 },
            icon: ui::SystemIcon::Cross.into(),
            ..Default::default()
        }.into());
        ctrl.add_view(|| ui::IconView{
            position: ui::Position{ x: 117, y: 13 },
            icon: ui::SystemIcon::Check.into(),
            ..Default::default()
        }.into());
        ctrl.add_view(|| ui::LabelLineView{
            frame: ui::Frame{ x: 0, y: 12, width: 128, height: 12 },
            font: ui::TextFont::OpenSansRegular11px,
            horizontal_alignment: ui::TextHorizontalAlignment::Center,
            text: "Hello!",
            ..Default::default()
        }.into());
        ctrl.add_view(|| ui::LabelLineView{
            frame: ui::Frame{ x: 23, y: 26, width: 82, height: 12 },
            font: ui::TextFont::OpenSansRegular11px,
            horizontal_alignment: ui::TextHorizontalAlignment::Center,
            scroll: ui::ScrollMode::Once{ delay_secs: 10, speed: 26 },
            text: "Rust",
            ..Default::default()
        }.into());
    }

    fn process_action(&mut self, action: Self::Action) {
        // Handle actions generated by user interactions and ui events
        match action {
            ui::BasicAction::Previous => {
                exit(0);
            },
            _ => {},
        }
    }
}

fn main() {
    let mut state = AppState::new();
    let mut ui = ui::Middleware::new();

    MessageLoop::new().for_each(|ch| {
        let _ch = Some(ch)
            .and_then(|ch| ui.process_event(ch, &mut state))
            .and_then(|ch| ui.redraw_if_needed(ch, &mut state));
    });
}

entry!(main);
