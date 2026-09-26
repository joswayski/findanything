#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, ScrollArea, Stroke, Vec2};
use findanything_core::model::{EntityKind, SearchResponse, SearchResult};

const SHORTCUT: &str = "Ctrl+Shift+Space";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
                                let generation = thread_generation.load(Ordering::Acquire);
                                let _ = tx.send(Reply::SearchError(generation, error));
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
                    if let Some((generation, id, query)) = work.activations.pop_front() {
                        drop(work);
                        if generation != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        let result = engine
                            .as_ref()
                            .ok_or_else(|| "Search engine is unavailable".into())
                            .and_then(|engine| engine.activate(&id, &query));
                        let _ = tx.send(Reply::Activation(generation, result));
                        continue;
                    }
                    let request = work.search.take();
                    drop(work);
                    if let Some((generation, query)) = request {
                        if generation != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        match engine.as_ref() {
                            Some(engine) => {
                                let _ = tx.send(Reply::Search(generation, engine.search(&query)));
                            }
                            None => {
                                let _ = tx.send(Reply::SearchError(
                                    generation,
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

    fn search(&self, generation: u64, query: String) {
        self.generation.store(generation, Ordering::Release);
        self.state.0.lock().unwrap().search = Some((generation, query));
        self.state.1.notify_one();
    }
    fn activate(&self, generation: u64, id: String, query: String) {
        if generation != self.generation.load(Ordering::Acquire) {
            return;
        }
        self.state
            .0
            .lock()
            .unwrap()
            .activations
            .push_back((generation, id, query));
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

struct Launcher {
    query: String,
    response: SearchResponse,
    selected: usize,
    generation: u64,
    response_generation: u64,
    pending: bool,
    activating: bool,
    scroll_selection: bool,
    visible: bool,
    error: Option<String>,
    worker: Option<Worker>,
    fixture: Option<Fixture>,
    focus_query: bool,
    was_focused: bool,
    hotkey: Option<Hotkey>,
    shortcut_error: Option<String>,
    instance: Option<findanything_core::instance::Instance>,
    last_warming_poll: Instant,
    update_status: String,
    quitting: bool,
}

impl Launcher {
    fn new(fixture: Option<Fixture>) -> Self {
        let response = fixture_response(fixture);
        let worker = fixture.is_none().then(Worker::spawn);
        let (hotkey, shortcut_error) = if fixture.is_some() {
            (None, None)
        } else {
            match Hotkey::new() {
                Ok(h) => (Some(h), None),
                Err(e) => (None, Some(e)),
            }
        };
        let mut app = Self {
            query: String::new(),
            response,
            selected: 0,
            generation: 0,
            response_generation: 0,
            pending: false,
            activating: false,
            scroll_selection: true,
            visible: true,
            error: (fixture == Some(Fixture::Error)).then(|| "Deterministic fixture error".into()),
            worker,
            fixture,
            focus_query: true,
            was_focused: false,
            hotkey,
            shortcut_error,
            instance: None,
            last_warming_poll: Instant::now(),
            update_status: String::new(),
            quitting: false,
        };
        app.request_search();
        app
    }
    fn request_search(&mut self) {
        self.generation += 1;
        if let Some(worker) = &self.worker {
            self.pending = true;
            worker.search(self.generation, self.query.clone());
        } else if let Some(fixture) = self.fixture {
            let mut response = fixture_response(Some(fixture));
            response.results.retain(|r| {
                format!("{} {}", r.title, r.subtitle)
                    .to_lowercase()
                    .contains(&self.query.to_lowercase())
            });
            self.accept_response(self.generation, response);
        }
    }
    fn accept_response(&mut self, generation: u64, response: SearchResponse) -> bool {
        if generation != self.generation {
            return false;
        }
        self.response = response;
        self.response_generation = generation;
        self.pending = false;
        self.selected = self
            .selected
            .min(self.response.results.len().saturating_sub(1));
        self.scroll_selection = true;
        true
    }
    fn can_activate(&self) -> bool {
        !self.pending
            && !self.activating
            && self.error.is_none()
            && self.response_generation == self.generation
            && self.selected < self.response.results.len()
    }
    fn move_selection(&mut self, delta: isize) {
        if self.response.results.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta)
            .clamp(0, self.response.results.len() as isize - 1) as usize;
        self.scroll_selection = true;
    }
    fn activate_selected(&mut self) {
        if !self.can_activate() {
            return;
        }
        let Some(result) = self.response.results.get(self.selected) else {
            return;
        };
        if let Some(worker) = &self.worker {
            self.activating = true;
            worker.activate(self.generation, result.id.clone(), self.query.clone());
        }
    }
    fn show(&mut self, ctx: &egui::Context) {
        self.visible = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.focus_query = true;
    }
    fn dismiss(&mut self, ctx: &egui::Context) {
        self.visible = false;
        if self.hotkey.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        if focused && !self.was_focused {
            self.focus_query = true;
            self.visible = true;
        }
        self.was_focused = focused;
        while self.hotkey.as_ref().is_some_and(Hotkey::pressed) {
            if self.visible && ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
                self.dismiss(ctx);
            } else {
                self.show(ctx);
            }
        }
        if self
            .instance
            .as_ref()
            .is_some_and(|i| i.take_activation_request())
        {
            self.show(ctx);
        }
        if self.fixture.is_none() {
            let status = findanything_core::updates::status();
            self.update_status = if status.message.is_empty() {
                status.state
            } else {
                status.message
            };
        }
        let replies: Vec<_> = self
            .worker
            .as_ref()
            .map(|w| w.replies.try_iter().collect())
            .unwrap_or_default();
        for reply in replies {
            match reply {
                Reply::Ready => {
                    self.error = None;
                    self.request_search();
                }
                Reply::Search(g, response) if g == self.generation => {
                    self.accept_response(g, response);
                    self.error = None;
                }
                Reply::SearchError(g, e) if g == self.generation => {
                    self.pending = false;
                    self.error = Some(e);
                }
                Reply::Activation(g, result) if g == self.generation => {
                    self.activating = false;
                    match result {
                        Ok(()) => self.dismiss(ctx),
                        Err(e) => self.error = Some(e),
                    }
                }
                _ => {}
            }
        }
        if self.response.semantic_status == "warming"
            && !self.pending
            && !self.activating
            && self.last_warming_poll.elapsed() >= Duration::from_millis(500)
        {
            self.last_warming_poll = Instant::now();
            self.request_search();
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.hotkey.is_some() && !self.quitting
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dismiss(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dismiss(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            self.move_selection(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.move_selection(-1);
        }
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));

        let dark = ctx.style().visuals.dark_mode;
        let panel = if dark {
            Color32::from_rgb(25, 25, 24)
        } else {
            Color32::from_rgb(246, 246, 244)
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(panel)
                    .inner_margin(12)
                    .corner_radius(20)
                    .stroke(Stroke::new(
                        1_f32,
                        if dark {
                            Color32::from_white_alpha(30)
                        } else {
                            Color32::from_black_alpha(28)
                        },
                    )),
            )
            .show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("Find Anything", |ui| {
                        if ui
                            .add_enabled(
                                self.fixture.is_none(),
                                egui::Button::new("Check for updates"),
                            )
                            .clicked()
                        {
                            findanything_core::updates::check();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.fixture.is_none()
                                    && findanything_core::updates::status().state == "ready",
                                egui::Button::new("Restart to update"),
                            )
                            .clicked()
                        {
                            if let Err(e) = findanything_core::updates::apply() {
                                self.error = Some(e);
                            }
                            ui.close();
                        }
                        if ui.button("Quit").clicked() {
                            self.quitting = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    let (_, rect) = ui.allocate_space(Vec2::splat(28.));
                    let stroke = Stroke::new(2_f32, ui.visuals().weak_text_color());
                    ui.painter()
                        .circle_stroke(rect.center() - Vec2::splat(3.), 8., stroke);
                    ui.painter().line_segment(
                        [
                            rect.center() + Vec2::splat(3.),
                            rect.right_bottom() - Vec2::splat(2.),
                        ],
                        stroke,
                    );
                    let mut output = ui
                        .add_enabled_ui(!self.activating, |ui| {
                            egui::TextEdit::singleline(&mut self.query)
                                .desired_width((ui.available_width() - 35.).max(100.))
                                .hint_text("Find anything…")
                                .font(egui::FontId::proportional(28.))
                                .show(ui)
                        })
                        .inner;
                    let edit = &output.response;
                    if self.focus_query {
                        edit.request_focus();
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(self.query.chars().count()),
                            )));
                        output.state.store(ctx, edit.id);
                        self.focus_query = false;
                    }
                    if edit.changed() {
                        self.selected = 0;
                        self.error = None;
                        self.request_search();
                    }
                    if !self.query.is_empty()
                        && ui
                            .add_enabled(!self.activating, egui::Button::new("×"))
                            .clicked()
                    {
                        self.query.clear();
                        self.selected = 0;
                        self.request_search();
                        edit.request_focus();
                    }
                });
                if enter {
                    self.activate_selected();
                }
                ui.separator();
                ui.label(
                    RichText::new(if self.query.is_empty() {
                        "APPS & ACTIONS"
                    } else {
                        "BEST MATCHES"
                    })
                    .size(11.)
                    .color(ui.visuals().weak_text_color()),
                );
                let footer_height = 42.;
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - footer_height).max(100.))
                    .show(ui, |ui| {
                        if let Some(error) = &self.error {
                            ui.vertical_centered(|ui| {
                                ui.add_space(65.);
                                ui.heading("Search hit a snag.");
                                ui.label(error);
                                if self.fixture.is_none()
                                    && ui.button("Retry").clicked()
                                    && let Some(w) = &self.worker
                                {
                                    w.retry()
                                }
                            });
                        } else if self.response.results.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(65.);
                                ui.heading("No local matches yet.");
                                ui.label("Try an app, a setting, or a filename.");
                            });
                        } else {
                            for index in 0..self.response.results.len() {
                                let result = &self.response.results[index];
                                let selected = index == self.selected;
                                let response = egui::Frame::new()
                                    .fill(if selected {
                                        ui.visuals().faint_bg_color
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .corner_radius(12)
                                    .inner_margin(8)
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.set_min_height(49.);
                                        ui.horizontal(|ui| {
                                            result_icon(ui, result);
                                            ui.vertical(|ui| {
                                                ui.set_width(
                                                    (ui.available_width() - 85.).max(150.),
                                                );
                                                ui.horizontal(|ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&result.title)
                                                                .size(16.)
                                                                .strong(),
                                                        )
                                                        .truncate(),
                                                    );
                                                });
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(&result.subtitle)
                                                            .size(12.)
                                                            .weak(),
                                                    )
                                                    .truncate(),
                                                );
                                                ui.label(
                                                    RichText::new(&result.reason).size(11.).weak(),
                                                );
                                            });
                                            if selected {
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(RichText::new("Open").weak());
                                                    },
                                                );
                                            }
                                        });
                                    })
                                    .response
                                    .interact(egui::Sense::click());
                                if response.hovered()
                                    && ctx.input(|i| i.pointer.delta() != Vec2::ZERO)
                                {
                                    self.selected = index;
                                }
                                if response.clicked() {
                                    self.selected = index;
                                    self.activate_selected();
                                }
                                if selected && self.scroll_selection {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                            }
                        }
                    });
                self.scroll_selection = false;
                ui.separator();
                ui.horizontal(|ui| {
                    let status = match self.response.semantic_status.as_str() {
                        "ready" => "Semantic ready",
                        "unavailable" => "Keyword mode",
                        _ => "Preparing index",
                    };
                    ui.label(RichText::new(status).size(10.).weak());
                    if let Some(e) = &self.shortcut_error {
                        ui.label(
                            RichText::new("Shortcut unavailable")
                                .size(10.)
                                .color(ui.visuals().warn_fg_color),
                        )
                        .on_hover_text(e);
                    }
                    if self.fixture.is_none() {
                        ui.label(RichText::new("Updates").size(10.).weak())
                            .on_hover_text(&self.update_status);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("Up/Down Navigate   Enter Open   Esc Close")
                                .size(10.)
                                .weak(),
                        );
                    });
                });
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

fn result_icon(ui: &mut egui::Ui, result: &SearchResult) {
    let (_, rect) = ui.allocate_space(Vec2::splat(38.));
    let color = match result.kind {
        EntityKind::Application => Color32::from_rgb(190, 113, 25),
        EntityKind::SystemAction => Color32::from_rgb(74, 82, 98),
        EntityKind::File => Color32::from_rgb(62, 92, 140),
    };
    ui.painter().rect_filled(rect, 9, color);
    let stroke = Stroke::new(1.5_f32, Color32::WHITE);
    match result.kind {
        EntityKind::Application => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                result
                    .title
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string(),
                egui::FontId::proportional(19.),
                Color32::WHITE,
            );
        }
        EntityKind::SystemAction => {
            for y in [-7., 0., 7.] {
                ui.painter().line_segment(
                    [
                        rect.center() + Vec2::new(-10., y),
                        rect.center() + Vec2::new(10., y),
                    ],
                    stroke,
                );
                ui.painter().circle_filled(
                    rect.center() + Vec2::new(if y == 0. { 4. } else { -4. }, y),
                    3.,
                    Color32::WHITE,
                );
            }
        }
        EntityKind::File => {
            ui.painter().rect_stroke(
                rect.shrink2(Vec2::new(10., 7.)),
                1,
                stroke,
                egui::StrokeKind::Inside,
            );
            for y in [-3., 3., 9.] {
                ui.painter().line_segment(
                    [
                        rect.center() + Vec2::new(-5., y),
                        rect.center() + Vec2::new(5., y),
                    ],
                    stroke,
                );
            }
        }
    }
}

fn fixture_response(fixture: Option<Fixture>) -> SearchResponse {
    let results = if fixture == Some(Fixture::Results) {
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

#[cfg(any(target_os = "windows", target_os = "linux"))]
struct Hotkey {
    _manager: global_hotkey::GlobalHotKeyManager,
    id: u32,
}
#[cfg(any(target_os = "windows", target_os = "linux"))]
impl Hotkey {
    fn new() -> Result<Self, String> {
        use global_hotkey::hotkey::{Code, HotKey, Modifiers};
        if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Err(
                "Use your desktop's shortcut settings to launch Find Anything on Wayland.".into(),
            );
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
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
struct Hotkey;
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
impl Hotkey {
    fn new() -> Result<Self, String> {
        Err(format!(
            "{SHORTCUT} registration is not available on this host"
        ))
    }
    fn pressed(&self) -> bool {
        false
    }
}

fn main() -> eframe::Result {
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
                    Some("light") => Some(egui::Theme::Light),
                    Some("dark") => Some(egui::Theme::Dark),
                    _ => None,
                }
            }
            "--theme=light" => theme = Some(egui::Theme::Light),
            "--theme=dark" => theme = Some(egui::Theme::Dark),
            _ => {}
        }
    }
    let instance = if fixture.is_none() {
        findanything_core::updates::initialize();
        match findanything_core::instance::Instance::acquire() {
            Ok(Some(i)) => Some(i),
            Ok(None) => return Ok(()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    if fixture.is_none() {
        findanything_core::updates::start();
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Find Anything")
            .with_inner_size(Vec2::new(760., 570.))
            .with_min_inner_size(Vec2::new(640., 420.))
            .with_transparent(false),
        ..Default::default()
    };
    eframe::run_native(
        "Find Anything",
        options,
        Box::new(move |cc| {
            if let Some(theme) = theme {
                cc.egui_ctx.set_theme(theme);
            }
            let mut app = Launcher::new(fixture);
            app.instance = instance;
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_is_clamped() {
        let mut app = Launcher::new(Some(Fixture::Results));
        app.selected = 2;
        app.move_selection(1);
        assert_eq!(app.selected, 2);
        app.move_selection(-9);
        assert_eq!(app.selected, 0);
    }
    #[test]
    fn empty_selection_is_safe() {
        let mut app = Launcher::new(Some(Fixture::Empty));
        app.move_selection(1);
        app.activate_selected();
        assert_eq!(app.selected, 0);
    }
    #[test]
    fn stale_results_cannot_replace_or_activate_a_new_query() {
        let mut app = Launcher::new(Some(Fixture::Results));
        assert!(app.can_activate());
        let old = app.generation;
        app.generation += 1;
        assert!(!app.can_activate());
        assert!(!app.accept_response(old, fixture_response(Some(Fixture::Empty))));
        assert_eq!(app.response.results[0].title, "Browser");
        assert!(app.accept_response(old + 1, fixture_response(Some(Fixture::Empty))));
        assert!(!app.can_activate());
        assert!(app.response.results.is_empty());
    }
    #[test]
    fn fixture_search_filters_and_clamps_selection() {
        let mut app = Launcher::new(Some(Fixture::Results));
        app.selected = 2;
        app.query = "display".into();
        app.request_search();
        assert_eq!(app.response.results.len(), 1);
        assert_eq!(app.response.results[0].title, "Displays");
        assert_eq!(app.selected, 0);
    }
}
