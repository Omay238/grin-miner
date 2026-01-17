// Copyright 2020 The Grin Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Basic TUI to better output the overall system status and status
//! of various subsystems

use std::sync::{Arc, RwLock, mpsc};
use std::{self, thread};
use time;

use cursive::Cursive;
use cursive::CursiveExt;
use cursive::direction::Orientation;
use cursive::theme::BaseColor::*;
use cursive::theme::Color::*;
use cursive::theme::PaletteColor::*;
use cursive::theme::{BaseColor, BorderStyle, Color, Theme};
use cursive::traits::*;
use cursive::utils::markup::StyledString;
use cursive::views::{BoxedView, LinearLayout, Panel, StackView, TextView};

use crate::tui::constants::*;
use crate::tui::types::*;
use crate::tui::{menu, mining, version};

use crate::stats;

use crate::built_info;

/// Main UI
pub struct UI {
	ui_tx: mpsc::Sender<UIMessage>,
	handle: Option<std::thread::JoinHandle<()>>,
}

fn modify_theme(theme: &mut Theme) {
	theme.shadow = false;
	theme.borders = BorderStyle::Simple;
	theme.palette[Background] = Dark(Black);
	theme.palette[Shadow] = Dark(Black);
	theme.palette[View] = Dark(Black);
	theme.palette[Primary] = Dark(White);
	theme.palette[Highlight] = Dark(Cyan);
	theme.palette[HighlightInactive] = Dark(Blue);
	// also secondary, tertiary, TitlePrimary, TitleSecondary
}

impl UI {
	/// Create a new UI
	pub fn new(controller_tx: mpsc::Sender<ControllerMessage>) -> UI {
		let (ui_tx, ui_rx) = mpsc::channel::<UIMessage>();

		let handle = std::thread::spawn(move || {
			let mut cursive = Cursive::default();

			let mining_view = mining::TUIMiningView::create();
			let version_view = version::TUIVersionView::create();
			let main_menu = menu::create();

			let root_stack = StackView::new()
				.layer(version_view)
				.layer(mining_view)
				.with_name(ROOT_STACK);

			let mut title_string = StyledString::new();
			title_string.append(StyledString::styled(
				format!("Grin Miner Version {}", built_info::PKG_VERSION),
				Color::Dark(BaseColor::Yellow),
			));

			let main_layer = LinearLayout::new(Orientation::Vertical)
				.child(Panel::new(TextView::new(title_string)))
				.child(
					LinearLayout::new(Orientation::Horizontal)
						.child(Panel::new(BoxedView::new(main_menu)))
						.child(Panel::new(root_stack)),
				);

			let mut theme = cursive.current_theme().clone();
			modify_theme(&mut theme);
			cursive.set_theme(theme);
			cursive.add_layer(main_layer);

			let controller_tx_clone = controller_tx.clone();
			cursive.add_global_callback('q', move |_| {
				controller_tx_clone
					.send(ControllerMessage::Shutdown)
					.unwrap();
			});
			cursive.set_fps(4);

			let cb_sink = cursive.cb_sink().clone();
			let _listener = std::thread::spawn(move || {
				while let Ok(message) = ui_rx.recv() {
					match message {
						UIMessage::UpdateStatus(update) => {
							let _ = cb_sink.send(Box::new(move |s: &mut Cursive| {
								mining::TUIMiningView::update(s, update.clone());
								version::TUIVersionView::update(s, update.clone());
							}));
						}
						UIMessage::Quit => {
							let _ = cb_sink.send(Box::new(|s: &mut Cursive| {
								s.quit();
							}));
							break;
						}
					}
				}
			});

			cursive.run();
		});

		UI {
			ui_tx,
			handle: Some(handle),
		}
	}

	pub fn stop(&mut self) {
		let _ = self.ui_tx.send(UIMessage::Quit);

		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
		}
	}
}

/// Controller message

pub struct Controller {
	rx: mpsc::Receiver<ControllerMessage>,
	ui: UI,
}

/// Controller Message
pub enum ControllerMessage {
	/// Shutdown
	Shutdown,
}

impl Controller {
	/// Create a new controller
	pub fn new() -> Result<Controller, String> {
		let (tx, rx) = mpsc::channel::<ControllerMessage>();
		Ok(Controller {
			rx,
			ui: UI::new(tx),
		})
	}
	/// Run the controller
	pub fn run(&mut self, stats: Arc<RwLock<stats::Stats>>) {
		let stat_update_interval = 1;
		let mut next_stat_update = time::get_time().sec + stat_update_interval;
		loop {
			if let Ok(message) = self.rx.try_recv() {
				match message {
					ControllerMessage::Shutdown => {
						let _ = self.ui.ui_tx.send(UIMessage::Quit);
						if let Some(handle) = self.ui.handle.take() {
							let _ = handle.join();
						}
						return;
					}
				}
			}

			if time::get_time().sec > next_stat_update {
				let _ = self.ui.ui_tx.send(UIMessage::UpdateStatus(stats.clone()));
				next_stat_update = time::get_time().sec + stat_update_interval;
			}

			thread::sleep(std::time::Duration::from_millis(100));
		}
	}
}
