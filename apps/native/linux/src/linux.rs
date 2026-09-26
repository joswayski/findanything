use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use findanything_core::model::{EntityKind, SearchResponse, SearchResult};
use gtk::gdk;
use gtk::glib::{self, ControlFlow, Propagation};
use gtk::prelude::*;

const SHORTCUT: &str = "Ctrl+Shift+Space";

#[allow(dead_code)]
mod graphite {
    include!("../../design/graphite.rs");
}

const GRAPHITE_CSS: &str = concat!(
    include_str!("../../design/colors.css"),
    r#"
window, .graphite-root, scrolledwindow, viewport, list {
  background: @graphite_bg;
  color: @graphite_text;
  font-family: system-ui, sans-serif;
  font-size: 13px;
}
.graphite-root { padding: 0; }
.graphite-search {
  min-height: 38px;
  padding: 0 12px;
  border: 1px solid @graphite_border_strong;
  border-radius: 7px;
  background: @graphite_control;
  color: @graphite_text_strong;
  box-shadow: none;
}
.graphite-search:hover { background: @graphite_control_hover; }
.graphite-search:focus-within {
  border-color: @graphite_accent;
  box-shadow: 0 0 0 1px @graphite_accent;
}
.graphite-search:disabled { color: @graphite_faint; opacity: .65; }
.graphite-menu > button {
  min-width: 38px; min-height: 38px;
  padding: 0; border: 1px solid @graphite_border_strong;
  border-radius: 7px; background: @graphite_control; color: @graphite_secondary;
}
.graphite-menu > button:hover { background: @graphite_control_hover; color: @graphite_text_strong; }
.graphite-menu > button:active, .graphite-menu > button:checked { background: @graphite_control_active; }
.graphite-section {
  min-height: 20px; color: @graphite_muted; font-size: 11px;
  font-weight: 600; padding: 0 2px;
}
.graphite-list { background: @graphite_bg; }
.graphite-list row {
  min-height: 56px; padding: 0 12px 0 10px;
  border-left: 2px solid transparent; border-radius: 7px;
  background: @graphite_bg; color: @graphite_text;
}
.graphite-list row:hover { background: @graphite_control_hover; }
.graphite-list row:selected {
  background: @graphite_selected; border-left-color: @graphite_accent;
}
.result-icon { color: @graphite_secondary; font-size: 20px; font-weight: 400; }
.result-title { color: @graphite_text_strong; font-size: 13px; font-weight: 600; }
.result-subtitle { color: @graphite_muted; font-size: 11px; font-weight: 400; }
.state-row { background: @graphite_bg; border-left-color: transparent; }
.state-heading { color: @graphite_text_strong; font-weight: 600; font-size: 13px; }
.error-heading { color: @graphite_danger; }
.state-detail { color: @graphite_muted; font-size: 11px; }
.graphite-footer {
  min-height: 28px; color: @graphite_muted; font-size: 11px;
  border-top: 1px solid @graphite_hairline;
}
popover > contents {
  padding: 8px; border: 1px solid @graphite_border_strong;
  border-radius: 7px; background: @graphite_popover; color: @graphite_text;
}
.popover-menu button {
  padding: 7px 10px; border: 0; border-radius: 7px;
  background: transparent; color: @graphite_text;
}
.popover-menu button:hover { background: @graphite_control_hover; }
.popover-menu button:active { background: @graphite_control_active; }
.popover-menu button:disabled { color: @graphite_faint; opacity: .6; }
button.retry {
  padding: 6px 12px; border-radius: 7px; border: 1px solid @graphite_border_strong;
  background: @graphite_control; color: @graphite_text;
}
button.retry:hover { background: @graphite_control_hover; }
scrollbar { background: @graphite_bg; }
scrollbar slider { min-width: 5px; min-height: 24px; border-radius: 3px; background: @graphite_border_strong; }
"#
);

fn graphite_css() -> String {
    // GTK 4.6 has no CSS custom properties. Substitute the shared dimensions
    // here as well as using them in widget layout.
    GRAPHITE_CSS
        .replace("13px", &format!("{}px", graphite::BODY_SIZE))
        .replace("11px", &format!("{}px", graphite::METADATA_SIZE))
        .replace(
            "border-radius: 7px",
            &format!("border-radius: {}px", graphite::RADIUS),
        )
        .replace(
            "min-height: 56px",
            &format!("min-height: {}px", graphite::ROW_HEIGHT),
        )
        .replace(
            "min-height: 28px",
            &format!("min-height: {}px", graphite::FOOTER_HEIGHT),
        )
        .replace(
            "min-height: 20px",
            &format!("min-height: {}px", graphite::SECTION_HEIGHT),
        )
        .replace("38px", &format!("{}px", graphite::SEARCH_HEIGHT - 2))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Fixture {
    Results,
    Empty,
    Error,
}

#[derive(Default)]
struct Work {
    search: Option<(u64, String)>,
    activations: VecDeque<(u64, String, String)>,
    retry: bool,
    stop: bool,
}

enum Reply {
    Ready,
    Search(u64, SearchResponse),
    SearchError(u64, String),
    Activation(u64, Result<(), String>),
}

struct Worker {
    state: Arc<(Mutex<Work>, Condvar)>,
    generation: Arc<AtomicU64>,
    replies: mpsc::Receiver<Reply>,
}

impl Worker {
    fn spawn() -> Self {
        let state = Arc::new((Mutex::new(Work::default()), Condvar::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let (tx, replies) = mpsc::channel();
        let thread_state = state.clone();
        let thread_generation = generation.clone();
        std::thread::Builder::new()
            .name("search-worker".into())
            .spawn(move || {
                let mut engine = None;
                loop {
                    if engine.is_none() {
                        match findanything_core::SearchEngine::new() {
                            Ok(value) => {
                                engine = Some(value);
                                let _ = tx.send(Reply::Ready);
                            }
                            Err(error) => {
                                let _ = tx.send(Reply::SearchError(
                                    thread_generation.load(Ordering::Acquire),
                                    error,
                                ));
                            }
                        }
                    }
                    let mut work = thread_state.0.lock().unwrap();
                    while work.search.is_none()
                        && work.activations.is_empty()
                        && !work.retry
                        && !work.stop
                    {
                        work = thread_state.1.wait(work).unwrap();
                    }
                    if work.stop {
                        break;
                    }
                    if work.retry {
                        work.retry = false;
                        engine = None;
                        continue;
                    }
                    if let Some((g, id, query)) = work.activations.pop_front() {
                        drop(work);
                        if g != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        let result = engine
                            .as_ref()
                            .ok_or_else(|| "Search engine is unavailable".into())
                            .and_then(|e| e.activate(&id, &query));
                        let _ = tx.send(Reply::Activation(g, result));
                        continue;
                    }
                    let request = work.search.take();
                    drop(work);
                    if let Some((g, query)) = request {
                        if g != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        match engine.as_ref() {
                            Some(e) => {
                                let _ = tx.send(Reply::Search(g, e.search(&query)));
                            }
                            None => {
                                let _ = tx.send(Reply::SearchError(
                                    g,
                                    "Search engine is unavailable".into(),
                                ));
                            }
                        }
                    }
                }
            })
            .expect("failed to create search worker");
        Self {
            state,
            generation,
            replies,
        }
    }
    fn search(&self, g: u64, query: String) {
        self.generation.store(g, Ordering::Release);
        self.state.0.lock().unwrap().search = Some((g, query));
        self.state.1.notify_one();
    }
    fn activate(&self, g: u64, id: String, query: String) {
        if g != self.generation.load(Ordering::Acquire) {
            return;
        }
        self.state
            .0
            .lock()
            .unwrap()
            .activations
            .push_back((g, id, query));
        self.state.1.notify_one();
    }
    fn retry(&self) {
        self.state.0.lock().unwrap().retry = true;
        self.state.1.notify_one();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.state.0.lock().unwrap().stop = true;
        self.state.1.notify_one();
    }
}

struct Hotkey {
    _manager: global_hotkey::GlobalHotKeyManager,
    id: u32,
}
impl Hotkey {
    fn new() -> Result<Self, String> {
        use global_hotkey::hotkey::{Code, HotKey, Modifiers};
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Err("Configure your compositor to launch Find Anything with Ctrl+Shift+Space on Wayland.".into());
        }
        let manager = global_hotkey::GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        let key = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
        let id = key.id();
        manager
            .register(key)
            .map_err(|e| format!("{SHORTCUT}: {e}"))?;
        Ok(Self {
            _manager: manager,
            id,
        })
    }
    fn pressed(&self) -> bool {
        global_hotkey::GlobalHotKeyEvent::receiver()
            .try_recv()
            .is_ok_and(|e| e.id == self.id && e.state == global_hotkey::HotKeyState::Pressed)
    }
}

struct Ui {
    window: gtk::ApplicationWindow,
    query: gtk::SearchEntry,
    list: gtk::ListBox,
    scroll: gtk::ScrolledWindow,
    results_stack: gtk::Stack,
    message: gtk::Box,
    section: gtk::Label,
    status: gtk::Label,
    worker: Option<Worker>,
    fixture: Option<Fixture>,
    response: SearchResponse,
    generation: u64,
    response_generation: u64,
    selected: usize,
    pending: bool,
    activating: bool,
    error: Option<String>,
    hotkey: Option<Hotkey>,
    instance: Option<findanything_core::instance::Instance>,
    last_warming_poll: Instant,
    quitting: bool,
}

impl Ui {
    fn request_search(&mut self) {
        self.generation += 1;
        if let Some(worker) = &self.worker {
            self.pending = true;
            worker.search(self.generation, self.query.text().to_string());
        } else if let Some(fixture) = self.fixture {
            let needle = self.query.text().to_lowercase();
            let mut response = fixture_response(fixture);
            response.results.retain(|r| {
                format!("{} {}", r.title, r.subtitle)
                    .to_lowercase()
                    .contains(&needle)
            });
            self.accept_response(self.generation, response);
        }
    }
    fn accept_response(&mut self, g: u64, response: SearchResponse) {
        if g != self.generation {
            return;
        }
        self.response = response;
        self.response_generation = g;
        self.pending = false;
        self.selected = self
            .selected
            .min(self.response.results.len().saturating_sub(1));
        self.render();
    }
    fn render(&self) {
        self.section.set_text(if self.query.text().is_empty() {
            "Apps & actions"
        } else {
            "Best matches"
        });
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        while let Some(child) = self.message.first_child() {
            self.message.remove(&child);
        }
        self.results_stack.set_visible_child_name(
            if self.error.is_some() || self.response.results.is_empty() {
                "message"
            } else {
                "results"
            },
        );
        if let Some(error) = &self.error {
            let heading = gtk::Label::new(Some("Search hit a snag."));
            heading.add_css_class("state-heading");
            heading.add_css_class("error-heading");
            self.message.append(&heading);
            let detail = gtk::Label::new(Some(error));
            detail.set_wrap(true);
            detail.add_css_class("state-detail");
            self.message.append(&detail);
            if self.fixture.is_none() {
                let retry = gtk::Button::with_label("Retry");
                retry.add_css_class("retry");
                retry.set_halign(gtk::Align::Center);
                retry.set_action_name(Some("app.retry"));
                self.message.append(&retry);
            }
        } else if self.response.results.is_empty() {
            let label = gtk::Label::new(Some(
                "No local matches yet.\nTry an app, a setting, or a filename.",
            ));
            label.set_justify(gtk::Justification::Center);
            label.add_css_class("state-detail");
            self.message.append(&label);
        } else {
            for result in &self.response.results {
                let row = gtk::ListBoxRow::new();
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                let icon = gtk::DrawingArea::new();
                icon.set_size_request(graphite::ICON_SIZE, graphite::ICON_SIZE);
                icon.set_valign(gtk::Align::Center);
                let kind = result.kind;
                icon.set_draw_func(move |_, cr, width, height| {
                    cr.scale(width as f64 / 24.0, height as f64 / 24.0);
                    let color = graphite::SECONDARY;
                    cr.set_source_rgb(
                        (color >> 16) as f64 / 255.0,
                        ((color >> 8) & 255) as f64 / 255.0,
                        (color & 255) as f64 / 255.0,
                    );
                    cr.set_line_width(1.6);
                    cr.set_line_join(gtk::cairo::LineJoin::Round);
                    match kind {
                        EntityKind::Application => {
                            cr.rectangle(4.0, 4.0, 16.0, 16.0);
                            cr.move_to(4.0, 9.0);
                            cr.line_to(20.0, 9.0);
                        }
                        EntityKind::File => {
                            cr.move_to(6.0, 3.0);
                            cr.line_to(14.0, 3.0);
                            cr.line_to(19.0, 8.0);
                            cr.line_to(19.0, 21.0);
                            cr.line_to(6.0, 21.0);
                            cr.close_path();
                            cr.move_to(14.0, 3.0);
                            cr.line_to(14.0, 8.0);
                            cr.line_to(19.0, 8.0);
                        }
                        EntityKind::SystemAction => {
                            for (y, x) in [(6.0, 9.0), (12.0, 16.0), (18.0, 8.0)] {
                                cr.move_to(3.0, y);
                                cr.line_to(21.0, y);
                                cr.move_to(x, y - 3.0);
                                cr.line_to(x, y + 3.0);
                            }
                        }
                    }
                    let _ = cr.stroke();
                });
                content.append(&icon);
                let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
                labels.set_hexpand(true);
                let title = gtk::Label::new(Some(&result.title));
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title.set_xalign(0.0);
                title.add_css_class("result-title");
                let subtitle =
                    gtk::Label::new(Some(&format!("{}  ·  {}", result.subtitle, result.reason)));
                subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
                subtitle.set_xalign(0.0);
                subtitle.add_css_class("result-subtitle");
                labels.append(&title);
                labels.append(&subtitle);
                content.append(&labels);
                row.set_child(Some(&content));
                self.list.append(&row);
            }
            if let Some(row) = self.list.row_at_index(self.selected as i32) {
                self.list.select_row(Some(&row));
            }
        }
        let semantic = match self.response.semantic_status.as_str() {
            "ready" => "Semantic ready",
            "unavailable" => "Keyword mode",
            _ => "Preparing index",
        };
        self.status.set_text(semantic);
    }
    fn move_selection(&mut self, delta: isize) {
        if self.response.results.is_empty() {
            return;
        }
        self.selected = (self.selected as isize + delta)
            .clamp(0, self.response.results.len() as isize - 1) as usize;
        if let Some(row) = self.list.row_at_index(self.selected as i32) {
            self.list.select_row(Some(&row));
            let bounds = row.allocation();
            let adjustment = self.scroll.vadjustment();
            adjustment.clamp_page(bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
        }
    }
    fn activate(&mut self, index: usize) {
        if self.pending
            || self.activating
            || self.error.is_some()
            || self.response_generation != self.generation
        {
            return;
        }
        let Some(result) = self.response.results.get(index) else {
            return;
        };
        self.selected = index;
        if let Some(worker) = &self.worker {
            self.activating = true;
            self.query.set_sensitive(false);
            worker.activate(
                self.generation,
                result.id.clone(),
                self.query.text().to_string(),
            );
        }
    }
    fn present(&self) {
        self.window.present();
        self.query.grab_focus();
    }
    fn dismiss(&self) {
        if self.hotkey.is_some() {
            self.window.set_visible(false);
        } else {
            self.window.minimize();
        }
    }
}

fn fixture_response(fixture: Fixture) -> SearchResponse {
    let results = if fixture == Fixture::Results {
        vec![
            SearchResult {
                id: "fixture:browser".into(),
                kind: EntityKind::Application,
                title: "Browser".into(),
                subtitle: "Application".into(),
                score: 100.,
                reason: "Title match".into(),
            },
            SearchResult {
                id: "fixture:display".into(),
                kind: EntityKind::SystemAction,
                title: "Displays".into(),
                subtitle: "System Settings".into(),
                score: 90.,
                reason: "Suggested".into(),
            },
            SearchResult {
                id: "fixture:file".into(),
                kind: EntityKind::File,
                title: "Project notes.md".into(),
                subtitle: "~/Documents".into(),
                score: 80.,
                reason: "Filename match".into(),
            },
        ]
    } else {
        vec![]
    };
    SearchResponse {
        results,
        semantic_status: "unavailable".into(),
        semantic_message: Some("Fixture uses keyword mode".into()),
    }
}

fn build_ui(
    app: &gtk::Application,
    fixture: Option<Fixture>,
    theme: Option<bool>,
    instance: Option<findanything_core::instance::Instance>,
) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&graphite_css());
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    // Keep this fixture control: Graphite's application-priority surfaces must
    // render identically over both host theme variants.
    if let (Some(settings), Some(dark)) = (gtk::Settings::default(), theme) {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Find Anything")
        .default_width(graphite::WINDOW_WIDTH)
        .default_height(graphite::WINDOW_HEIGHT)
        .build();
    window.set_size_request(graphite::MINIMUM_WIDTH, graphite::MINIMUM_HEIGHT);
    let root = gtk::Box::new(gtk::Orientation::Vertical, graphite::GAP);
    root.add_css_class("graphite-root");
    root.set_margin_top(graphite::INSET);
    root.set_margin_bottom(graphite::INSET);
    root.set_margin_start(graphite::INSET);
    root.set_margin_end(graphite::INSET);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, graphite::GAP);
    let query = gtk::SearchEntry::builder()
        .placeholder_text("Find anything…")
        .hexpand(true)
        .build();
    query.set_size_request(-1, graphite::SEARCH_HEIGHT);
    query.add_css_class("graphite-search");
    header.append(&query);
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .build();
    menu_button.set_size_request(graphite::SEARCH_HEIGHT, graphite::SEARCH_HEIGHT);
    menu_button.add_css_class("graphite-menu");
    let popover = gtk::Popover::new();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 4);
    menu.add_css_class("popover-menu");
    let check = gtk::Button::with_label("Check for updates");
    let restart = gtk::Button::with_label("Restart to update");
    let quit = gtk::Button::with_label("Quit");
    check.set_sensitive(fixture.is_none());
    restart.set_sensitive(false);
    menu.append(&check);
    menu.append(&restart);
    menu.append(&quit);
    popover.set_child(Some(&menu));
    menu_button.set_popover(Some(&popover));
    header.append(&menu_button);
    root.append(&header);
    let section = gtk::Label::new(Some("Apps & actions"));
    section.set_xalign(0.0);
    section.set_size_request(-1, graphite::SECTION_HEIGHT);
    section.add_css_class("graphite-section");
    root.append(&section);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("graphite-list");
    list.set_vexpand(true);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    let message = gtk::Box::new(gtk::Orientation::Vertical, graphite::GAP);
    message.set_valign(gtk::Align::Center);
    let results_stack = gtk::Stack::new();
    results_stack.set_vexpand(true);
    results_stack.add_named(&scroll, Some("results"));
    results_stack.add_named(&message, Some("message"));
    root.append(&results_stack);
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_size_request(-1, graphite::FOOTER_HEIGHT);
    footer.add_css_class("graphite-footer");
    let status = gtk::Label::new(None);
    footer.append(&status);
    let help = gtk::Label::new(Some("↑/↓ Navigate   Enter Open   Esc Close"));
    help.set_hexpand(true);
    help.set_xalign(1.0);
    footer.append(&help);
    root.append(&footer);
    window.set_child(Some(&root));

    let (hotkey, shortcut_error) = if fixture.is_none() {
        match Hotkey::new() {
            Ok(h) => (Some(h), None),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };
    if let Some(error) = shortcut_error {
        status.set_tooltip_text(Some(&error));
        status.set_text("Shortcut unavailable — use the app window");
    }
    let response = fixture.map(fixture_response).unwrap_or(SearchResponse {
        results: vec![],
        semantic_status: "warming".into(),
        semantic_message: None,
    });
    let ui = Rc::new(RefCell::new(Ui {
        window: window.clone(),
        query: query.clone(),
        list: list.clone(),
        scroll,
        results_stack,
        message,
        section,
        status,
        worker: fixture.is_none().then(Worker::spawn),
        fixture,
        response,
        generation: 0,
        response_generation: 0,
        selected: 0,
        pending: false,
        activating: false,
        error: (fixture == Some(Fixture::Error)).then(|| "Deterministic fixture error".into()),
        hotkey,
        instance,
        last_warming_poll: Instant::now(),
        quitting: false,
    }));
    ui.borrow().render();
    ui.borrow_mut().request_search();

    let retry_action = gtk::gio::SimpleAction::new("retry", None);
    retry_action.connect_activate({
        let ui = ui.clone();
        move |_, _| {
            let mut ui = ui.borrow_mut();
            if let Some(worker) = &ui.worker {
                worker.retry();
                ui.pending = true;
            }
        }
    });
    app.add_action(&retry_action);

    query.connect_changed({
        let ui = ui.clone();
        move |_| {
            let mut ui = ui.borrow_mut();
            ui.selected = 0;
            ui.error = None;
            ui.request_search();
        }
    });
    list.connect_row_activated({
        let ui = ui.clone();
        move |_, row| ui.borrow_mut().activate(row.index() as usize)
    });
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let ui = ui.clone();
        move |_, key, _, _| {
            let mut ui = ui.borrow_mut();
            match key {
                gdk::Key::Down => ui.move_selection(1),
                gdk::Key::Up => ui.move_selection(-1),
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    let i = ui.selected;
                    ui.activate(i)
                }
                gdk::Key::Escape => ui.dismiss(),
                _ => return Propagation::Proceed,
            };
            Propagation::Stop
        }
    });
    window.add_controller(keys);
    check.connect_clicked(|_| findanything_core::updates::check());
    restart.connect_clicked({
        let ui = ui.clone();
        move |_| {
            if let Err(e) = findanything_core::updates::apply() {
                ui.borrow_mut().error = Some(e);
                ui.borrow().render()
            }
        }
    });
    quit.connect_clicked({
        let ui = ui.clone();
        let app = app.clone();
        move |_| {
            ui.borrow_mut().quitting = true;
            app.quit()
        }
    });
    window.connect_close_request({
        let ui = ui.clone();
        move |_| {
            if !ui.borrow().quitting && ui.borrow().hotkey.is_some() {
                ui.borrow().dismiss();
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        }
    });
    glib::timeout_add_local(Duration::from_millis(100), {
        let ui = ui.clone();
        let restart = restart.clone();
        move || {
            let mut ui = ui.borrow_mut();
            while ui.hotkey.as_ref().is_some_and(Hotkey::pressed) {
                if ui.window.is_visible() && ui.window.is_active() {
                    ui.dismiss()
                } else {
                    ui.present()
                }
            }
            if ui
                .instance
                .as_ref()
                .is_some_and(|i| i.take_activation_request())
            {
                ui.present()
            }
            let replies: Vec<_> = ui
                .worker
                .as_ref()
                .map(|w| w.replies.try_iter().collect())
                .unwrap_or_default();
            for reply in replies {
                match reply {
                    Reply::Ready => {
                        ui.error = None;
                        ui.request_search()
                    }
                    Reply::Search(g, r) if g == ui.generation => {
                        ui.error = None;
                        ui.accept_response(g, r)
                    }
                    Reply::SearchError(g, e) if g == ui.generation => {
                        ui.pending = false;
                        ui.error = Some(e);
                        ui.render()
                    }
                    Reply::Activation(g, r) if g == ui.generation => {
                        ui.activating = false;
                        ui.query.set_sensitive(true);
                        match r {
                            Ok(()) => ui.dismiss(),
                            Err(e) => {
                                ui.error = Some(e);
                                ui.render()
                            }
                        }
                    }
                    _ => {}
                }
            }
            if ui.response.semantic_status == "warming"
                && !ui.pending
                && !ui.activating
                && ui.last_warming_poll.elapsed() >= Duration::from_millis(500)
            {
                ui.last_warming_poll = Instant::now();
                ui.request_search()
            }
            if ui.fixture.is_none() {
                let updates = findanything_core::updates::status();
                restart.set_sensitive(updates.state == "ready");
                restart.set_tooltip_text(Some(&updates.message));
            }
            ControlFlow::Continue
        }
    });
    window.present();
    query.grab_focus();
}

pub fn main() -> glib::ExitCode {
    let mut fixture = None;
    let mut theme = None;
    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                fixture = Some(
                    match args.next_if(|v| v == "empty" || v == "error").as_deref() {
                        Some("empty") => Fixture::Empty,
                        Some("error") => Fixture::Error,
                        _ => Fixture::Results,
                    },
                )
            }
            "--fixture=empty" => fixture = Some(Fixture::Empty),
            "--fixture=error" => fixture = Some(Fixture::Error),
            "--theme" => {
                theme = match args.next().as_deref() {
                    Some("dark") => Some(true),
                    Some("light") => Some(false),
                    _ => None,
                }
            }
            "--theme=dark" => theme = Some(true),
            "--theme=light" => theme = Some(false),
            _ => {}
        }
    }
    let instance = if fixture.is_none() {
        findanything_core::updates::initialize();
        match findanything_core::instance::Instance::acquire() {
            Ok(Some(i)) => Some(i),
            Ok(None) => return glib::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                return glib::ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    if fixture.is_none() {
        findanything_core::updates::start()
    }
    let app = gtk::Application::builder()
        .application_id("com.joswayski.FindAnything")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let instance = Rc::new(RefCell::new(instance));
    app.connect_activate(move |app| build_ui(app, fixture, theme, instance.borrow_mut().take()));
    app.run_with_args::<&str>(&[])
}
