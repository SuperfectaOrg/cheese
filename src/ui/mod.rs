use gtk4::prelude::*;
use gtk4::{glib, Application, ApplicationWindow, Box, Orientation, Notebook};
use crate::state::AppState;
use std::sync::Arc;
use std::path::PathBuf;

pub struct CheeseWindow {
    window: ApplicationWindow,
    notebook: Notebook,
    app_state: Arc<AppState>,
}

impl CheeseWindow {
    pub fn new(app: &Application, app_state: Arc<AppState>) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Cheese")
            .default_width(1200)
            .default_height(800)
            .build();

        let main_box = Box::new(Orientation::Vertical, 0);
        window.set_child(Some(&main_box));

        let notebook = Notebook::builder()
            .scrollable(true)
            .show_border(false)
            .build();

        main_box.append(&notebook);

        let mut cheese_window = Self {
            window,
            notebook,
            app_state,
        };

        cheese_window.create_initial_tab();
        cheese_window.setup_keyboard_shortcuts();
        cheese_window.setup_signals();

        cheese_window
    }

    fn create_initial_tab(&mut self) {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/"));
        
        self.add_tab(home);
    }

    fn add_tab(&mut self, path: PathBuf) {
        let tab_label = gtk4::Label::new(Some(&self.get_tab_name(&path)));
        
        let tab_content = Box::new(Orientation::Vertical, 0);
        let path_label = gtk4::Label::new(Some(&format!("Path: {}", path.display())));
        tab_content.append(&path_label);

        let close_button = gtk4::Button::with_label("×");
        close_button.set_has_frame(false);
        
        let tab_box = Box::new(Orientation::Horizontal, 4);
        tab_box.append(&tab_label);
        tab_box.append(&close_button);

        let page_num = self.notebook.append_page(&tab_content, Some(&tab_box));
        self.notebook.set_current_page(Some(page_num));

        let notebook = self.notebook.clone();
        close_button.connect_clicked(move |_| {
            if let Some(page) = notebook.current_page() {
                notebook.remove_page(Some(page));
            }
        });

        self.app_state.add_tab(path);
    }

    fn get_tab_name(&self, path: &PathBuf) -> String {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Home")
            .to_string()
    }

    fn setup_keyboard_shortcuts(&self) {
        let controller = gtk4::EventControllerKey::new();
        let app_state = Arc::clone(&self.app_state);
        let window_ref = self.window.clone();

        controller.connect_key_pressed(move |_, key, _, modifiers| {
            use gtk4::gdk::ModifierType;
            use gtk4::gdk::Key;

            let ctrl = modifiers.contains(ModifierType::CONTROL_MASK);
            let shift = modifiers.contains(ModifierType::SHIFT_MASK);

            match (key, ctrl, shift) {
                (Key::t, true, false) => {
                    tracing::info!("New tab");
                    glib::Propagation::Stop
                }
                (Key::w, true, false) => {
                    tracing::info!("Close tab");
                    glib::Propagation::Stop
                }
                (Key::q, true, false) => {
                    window_ref.close();
                    glib::Propagation::Stop
                }
                (Key::h, true, false) => {
                    tracing::info!("Toggle hidden files");
                    glib::Propagation::Stop
                }
                (Key::f, true, false) => {
                    tracing::info!("Fuzzy search");
                    glib::Propagation::Stop
                }
                (Key::p, true, false) => {
                    tracing::info!("Command palette");
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });

        self.window.add_controller(controller);
    }

    fn setup_signals(&self) {
        let app_state = Arc::clone(&self.app_state);
        
        self.notebook.connect_switch_page(move |_, _, page_num| {
            app_state.set_active_tab(page_num as usize);
        });

        self.window.connect_close_request(move |_| {
            tracing::info!("Window closing");
            glib::Propagation::Proceed
        });
    }

    pub fn present(&self) {
        self.window.present();
    }
}
