// Parked UI from crates/nvf-launcher/src/app.rs.
// Not compiled. Nexus and ModPub download windows, the Downloads tab,
// and the link poll were removed from the running launcher.
// Line numbers are the ones these functions had when they were removed.

// ----- app.rs lines 5186-5355 -----

    fn poll_links(&mut self, ctx: &egui::Context) {
        let incoming = self.link_rx.as_ref().map(|rx| rx.try_recv());
        match incoming {
            Some(Ok(crate::links::DownloadDone::Saved(name))) => {
                self.link_rx = None;
                if self.download_name.trim().is_empty() {
                    self.download_name = Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or(&name).to_string();
                }
                self.download_pick = Some(name.clone());
                self.show_downloads();
                self.message = Some((format!("Downloaded {name}. It's in Downloads, not installed yet."), false));
            }
            Some(Ok(crate::links::DownloadDone::NeedKey(raw))) => {
                self.link_rx = None;
                self.ask_nexus_key_rejected(raw);
            }
            Some(Ok(crate::links::DownloadDone::Failed(e))) => {
                self.link_rx = None;
                self.message = Some((e, true));
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(200));
                return;
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.link_rx = None;
                self.message = Some(("The download stopped.".into(), true));
            }
            None => {}
        }
        if self.link_rx.is_some() || self.nexus_pending.is_some() || self.link_ask {
            return;
        }
        let Some(home) = self.home.clone() else { return };
        let Some(raw) = crate::links::pop(&home.root) else { return };
        self.start_link(home, raw);
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn start_link(&mut self, home: fnv_install::EngineHome, raw: String) {
        if !crate::links::handler_decided(&home.root) {
            self.link_held = Some(raw);
            self.link_ask = true;
            self.show_downloads();
            return;
        }
        if crate::links::is_nxm(&raw) && !crate::links::has_api_key(&home.root) {
            self.ask_nexus_key(raw);
            return;
        }
        if self.page != Page::Mods {
            self.show_downloads();
        }
        self.message = Some(("Downloading into Downloads. It is not installed until you install it.".into(), false));
        self.link_rx = Some(crate::links::spawn(home.root, raw));
    }

    fn link_handler_window(&mut self, ctx: &egui::Context) {
        if !self.link_ask {
            return;
        }
        let mut open = true;
        let mut yes = false;
        let mut no = false;
        egui::Window::new("Nexus and ModPub links")
            .collapsible(false)
            .resizable(false)
            .constrain(true)
            .max_size(fit_to_screen(ctx))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(RichText::new("Use this launcher when you click a Nexus or ModPub download? It is asked once. The file still downloads either way, and nothing is copied into Data.").small().color(WEAK));
                ui.horizontal(|ui| {
                    if ui.button("Use this launcher").clicked() {
                        yes = true;
                    }
                    if ui.button("Not now").clicked() {
                        no = true;
                    }
                });
            });
        if yes {
            self.answer_links(true);
        } else if no || !open {
            self.answer_links(false);
        }
    }

    fn answer_links(&mut self, yes: bool) {
        self.link_ask = false;
        let raw = self.link_held.take();
        let Some(home) = self.home.clone() else { return };
        if yes {
            match crate::links::register() {
                Ok(msg) => {
                    let _ = crate::links::remember_handler(&home.root, true);
                    self.message = Some((msg, false));
                }
                Err(e) => self.message = Some((e, true)),
            }
        } else if let Err(e) = crate::links::remember_handler(&home.root, false) {
            self.message = Some((e, true));
        }
        if let Some(raw) = raw {
            if crate::links::is_nxm(&raw) && !crate::links::has_api_key(&home.root) {
                self.ask_nexus_key(raw);
                return;
            }
            if self.page != Page::Mods {
                self.show_downloads();
            }
            self.link_rx = Some(crate::links::spawn(home.root, raw));
        }
    }

    /// The Downloads tab. Does not reload the mod list when it is already open.
    fn show_downloads(&mut self) {
        if self.page != Page::Mods {
            self.page = Page::Mods;
            self.enter_mods();
        }
        self.right_tab = 1;
    }

    fn downloads_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Downloads").small().color(GOLD)).on_hover_text("Archives from a Nexus or ModPub link. Install adds the mod. Data is not used.");
        if self.link_rx.is_some() {
            ui.label(RichText::new("Downloading… It shows up here when it finishes, and it is not installed yet.").small().color(WEAK));
        }
        if self.nexus_pending.is_some() && !self.nexus_key_open {
            ui.label(RichText::new("This Nexus download needs an API key.").small().color(BAD));
            if ui.button("Enter API key").clicked() {
                self.nexus_key_open = true;
                self.nexus_key_focus = true;
            }
        }
        ui.horizontal_wrapped(|ui| {
            let name_w = (ui.available_width() - 180.0).clamp(80.0, 220.0);
            ui.add(egui::TextEdit::singleline(&mut self.download_name).hint_text("Mod name").desired_width(name_w));
            if ui.add_enabled(self.download_pick.is_some(), egui::Button::new("Install")).on_hover_text("Installs this archive into the mods folder. Data is not used.").clicked() {
                self.install_picked_download();
            }
            if ui.button("Open folder").on_hover_text("The downloads folder next to the mods folder.").clicked() {
                if let Some(home) = &self.home {
                    let dir = home.root.join("downloads");
                    let _ = std::fs::create_dir_all(&dir);
                    launch::open_folder(&dir);
                }
            }
        });
        let files = self.download_files();
        if files.is_empty() && self.link_rx.is_none() {
            if self.nexus_pending.is_none() {
                ui.label(RichText::new("Nothing downloaded yet. A Nexus or ModPub link lands here.").small().color(WEAK));
            }
            return;
        }
        egui::ScrollArea::both().id_salt("downloads").auto_shrink([false, false]).show(ui, |ui| {
            for (name, bytes) in files {
                let on = self.download_pick.as_deref() == Some(name.as_str());
                let label = format!("{name}  {}", shown_size(bytes));
                if ui.selectable_label(on, label).clicked() {
                    self.download_name = Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or(&name).to_string();
                    self.download_pick = Some(name);
                }
            }
        });
    }

// ----- app.rs lines 5378-5413 -----

    fn download_files(&self) -> Vec<(String, u64)> {
        let Some(home) = &self.home else { return Vec::new() };
        let mut files = Vec::new();
        let Ok(rd) = std::fs::read_dir(home.root.join("downloads")) else { return files };
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
            if name.starts_with('.') {
                continue;
            }
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            files.push((name, bytes));
        }
        files.sort_by_key(|a| a.0.to_ascii_lowercase());
        files
    }

    fn install_picked_download(&mut self) {
        let Some(name) = self.download_pick.clone() else { return };
        let Some(home) = self.home.clone() else { return };
        let path = home.root.join("downloads").join(&name);
        if !path.is_file() {
            self.message = Some(("That download is gone.".into(), true));
            return;
        }
        let typed = self.download_name.trim().to_string();
        self.install_name = if typed.is_empty() {
            Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or(&name).to_string()
        } else {
            typed
        };
        self.begin_install(path);
    }

// ----- app.rs lines 3080-3175 -----

    /// Asks for the personal Nexus API key and saves it in the launcher home.
    fn nexus_key_window(&mut self, ctx: &egui::Context) {
        if !self.nexus_key_open {
            return;
        }
        let mut open = true;
        let bad = self.nexus_key_bad.clone();
        egui::Window::new("Nexus API key")
            .collapsible(false)
            .resizable(false)
            .constrain(true)
            .max_size(fit_to_screen(ctx))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(360.0);
                ui.label(RichText::new("Paste the personal API key from your Nexus account, under API access. It is sealed in the launcher home for this user. It is not written in the game folder, and it is not shown again.").small().color(WEAK));
                ui.label(RichText::new("A free account still needs Mod Manager Download on the mod page.").small().color(WEAK));
                let edit = ui.add(egui::TextEdit::singleline(&mut self.nexus_key_edit).password(true).hint_text("API key").desired_width(340.0));
                if self.nexus_key_focus {
                    edit.request_focus();
                    self.nexus_key_focus = false;
                }
                if let Some(bad) = &bad {
                    ui.label(RichText::new(bad).small().color(BAD));
                }
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Save").clicked() || (enter && (edit.lost_focus() || edit.has_focus())) {
                        self.commit_nexus_key();
                    }
                    if ui.button("Cancel").clicked() {
                        self.nexus_key_open = false;
                    }
                });
            });
        if !open {
            self.nexus_key_open = false;
        }
    }

    fn commit_nexus_key(&mut self) {
        let Some(home) = self.home.clone() else {
            self.nexus_key_bad = Some("No launcher home, so the API key was not saved.".into());
            return;
        };
        if let Err(e) = crate::links::save_api_key(&home.root, &self.nexus_key_edit) {
            self.nexus_key_bad = Some(e);
            return;
        }
        self.nexus_key_edit.clear();
        self.nexus_key_bad = None;
        self.nexus_key_open = false;
        let pending = self.nexus_pending.take();
        if let Some(raw) = pending {
            self.message = Some(("Nexus API key saved. Downloading.".into(), false));
            self.link_rx = Some(crate::links::spawn(home.root, raw));
        } else {
            self.message = Some(("Nexus API key saved.".into(), false));
        }
    }

    /// Holds an nxm link until a key is saved. A second link waits in the queue.
    fn ask_nexus_key(&mut self, raw: String) {
        let why = self.home.as_ref().and_then(|h| crate::links::saved_key_error(&h.root));
        self.hold_nexus_key(raw, why);
    }

    fn ask_nexus_key_rejected(&mut self, raw: String) {
        self.hold_nexus_key(raw, Some("Nexus refused that API key. Paste the personal API key again.".into()));
    }

    fn hold_nexus_key(&mut self, raw: String, refused: Option<String>) {
        if let Some(pending) = &self.nexus_pending {
            if pending != &raw {
                if let Some(home) = &self.home {
                    if let Err(e) = crate::links::enqueue(&home.root, std::slice::from_ref(&raw)) {
                        self.message = Some((e, true));
                    }
                }
                return;
            }
        } else {
            self.nexus_pending = Some(raw);
        }
        self.nexus_key_open = true;
        self.nexus_key_focus = true;
        if let Some(why) = refused {
            self.nexus_key_bad = Some(why.clone());
            self.message = Some((why, true));
        } else {
            self.nexus_key_bad = None;
            self.message = Some(("Nexus needs an API key for this download.".into(), false));
        }
        self.show_downloads();
    }


// ----- helper used only by the Downloads tab (app.rs shown_size) -----

fn shown_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let n = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if n < MB {
        format!("{:.1} KB", n / KB)
    } else if n < GB {
        format!("{:.1} MB", n / MB)
    } else {
        format!("{:.2} GB", n / GB)
    }
}
