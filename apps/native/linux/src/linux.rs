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
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        if let Some(error) = &self.error {
            let row = gtk::ListBoxRow::new();
            let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
            let heading = gtk::Label::new(Some("Search hit a snag."));
            heading.add_css_class("title-3");
            box_.append(&heading);
            let detail = gtk::Label::new(Some(error));
            detail.set_wrap(true);
            box_.append(&detail);
            if self.fixture.is_none() {
                let retry = gtk::Button::with_label("Retry");
                retry.set_action_name(Some("app.retry"));
                box_.append(&retry);
            }
            row.set_selectable(false);
            row.set_child(Some(&box_));
            self.list.append(&row);
        } else if self.response.results.is_empty() {
            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            let label = gtk::Label::new(Some(
                "No local matches yet.\nTry an app, a setting, or a filename.",
            ));
            label.set_justify(gtk::Justification::Center);
            row.set_child(Some(&label));
            self.list.append(&row);
        } else {
            for result in &self.response.results {
                let row = gtk::ListBoxRow::new();
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                let icon = gtk::Image::from_icon_name(match result.kind {
                    EntityKind::Application => "application-x-executable-symbolic",
                    EntityKind::SystemAction => "preferences-system-symbolic",
                    EntityKind::File => "text-x-generic-symbolic",
                });
                icon.set_pixel_size(32);
                content.append(&icon);
                let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
                labels.set_hexpand(true);
                let title = gtk::Label::new(Some(&result.title));
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title.set_xalign(0.0);
                title.add_css_class("heading");
                let subtitle =
                    gtk::Label::new(Some(&format!("{}  ·  {}", result.subtitle, result.reason)));
                subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
                subtitle.set_xalign(0.0);
                subtitle.add_css_class("dim-label");
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
    if let (Some(settings), Some(dark)) = (gtk::Settings::default(), theme) {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Find Anything")
        .default_width(760)
        .default_height(570)
        .build();
    window.set_size_request(640, 420);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let query = gtk::SearchEntry::builder()
        .placeholder_text("Find anything…")
        .hexpand(true)
        .build();
    header.append(&query);
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .build();
    let popover = gtk::Popover::new();
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 4);
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
    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    root.append(&scroll);
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let status = gtk::Label::new(None);
    status.add_css_class("dim-label");
    footer.append(&status);
    let help = gtk::Label::new(Some("↑/↓ Navigate   Enter Open   Esc Close"));
    help.set_hexpand(true);
    help.set_xalign(1.0);
    help.add_css_class("dim-label");
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
