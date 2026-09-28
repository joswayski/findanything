use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};

use findanything_core::model::{EntityKind, SearchResponse, SearchResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fixture {
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
    Error(u64, String),
    Activated(u64, Result<(), String>),
}

struct Worker {
    work: Arc<(Mutex<Work>, Condvar)>,
    generation: Arc<AtomicU64>,
    replies: mpsc::Receiver<Reply>,
}

impl Worker {
    fn spawn(context: eframe::egui::Context) -> Self {
        let work = Arc::new((Mutex::new(Work::default()), Condvar::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let (sender, replies) = mpsc::channel();
        let thread_work = Arc::clone(&work);
        let thread_generation = Arc::clone(&generation);

        std::thread::Builder::new()
            .name("search-worker".into())
            .spawn(move || {
                let send = |reply| {
                    if sender.send(reply).is_ok() {
                        context.request_repaint();
                    }
                };
                let mut engine = None;
                loop {
                    if engine.is_none() {
                        match findanything_core::SearchEngine::new() {
                            Ok(value) => {
                                engine = Some(value);
                                send(Reply::Ready);
                            }
                            Err(error) => send(Reply::Error(
                                thread_generation.load(Ordering::Acquire),
                                error,
                            )),
                        }
                    }

                    let mut state = thread_work.0.lock().unwrap();
                    while state.search.is_none()
                        && state.activations.is_empty()
                        && !state.retry
                        && !state.stop
                    {
                        state = thread_work.1.wait(state).unwrap();
                    }
                    if state.stop {
                        break;
                    }
                    if state.retry {
                        state.retry = false;
                        engine = None;
                        continue;
                    }
                    if let Some((generation, id, query)) = state.activations.pop_front() {
                        drop(state);
                        if generation != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        let result = engine
                            .as_ref()
                            .ok_or_else(|| "Search engine is unavailable".into())
                            .and_then(|engine| engine.activate(&id, &query));
                        send(Reply::Activated(generation, result));
                        continue;
                    }
                    let request = state.search.take();
                    drop(state);
                    if let Some((generation, query)) = request {
                        if generation != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        match engine.as_ref() {
                            Some(engine) => send(Reply::Search(generation, engine.search(&query))),
                            None => send(Reply::Error(
                                generation,
                                "Search engine is unavailable".into(),
                            )),
                        }
                    }
                }
            })
            .expect("failed to start search worker");

        Self {
            work,
            generation,
            replies,
        }
    }

    fn search(&self, generation: u64, query: String) {
        self.generation.store(generation, Ordering::Release);
        self.work.0.lock().unwrap().search = Some((generation, query));
        self.work.1.notify_one();
    }

    fn activate(&self, generation: u64, id: String, query: String) {
        if generation != self.generation.load(Ordering::Acquire) {
            return;
        }
        self.work
            .0
            .lock()
            .unwrap()
            .activations
            .push_back((generation, id, query));
        self.work.1.notify_one();
    }

    fn retry(&self) {
        self.work.0.lock().unwrap().retry = true;
        self.work.1.notify_one();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.work.0.lock().unwrap().stop = true;
        self.work.1.notify_one();
    }
}

pub struct Model {
    pub query: String,
    pub response: SearchResponse,
    pub selected: usize,
    pub pending: bool,
    pub activating: bool,
    pub error: Option<String>,
    fixture: Option<Fixture>,
    worker: Option<Worker>,
    generation: u64,
    response_generation: u64,
}

impl Model {
    pub fn new(fixture: Option<Fixture>, context: eframe::egui::Context) -> Self {
        let worker = fixture.is_none().then(|| Worker::spawn(context));
        let mut model = Self {
            query: String::new(),
            response: empty_response(),
            selected: 0,
            pending: false,
            activating: false,
            error: None,
            fixture,
            worker,
            generation: 0,
            response_generation: 0,
        };
        model.search();
        model
    }

    pub fn search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        self.activating = false;
        self.error = None;

        if let Some(worker) = &self.worker {
            worker.search(self.generation, self.query.clone());
        } else if let Some(fixture) = self.fixture {
            if fixture == Fixture::Error {
                self.accept_error(self.generation, "Deterministic fixture error".into());
                return;
            }
            let mut response = fixture_response(fixture);
            let needle = self.query.to_lowercase();
            response.results.retain(|result| {
                format!("{} {}", result.title, result.subtitle)
                    .to_lowercase()
                    .contains(&needle)
            });
            self.accept_search(self.generation, response);
        }
    }

    /// Applies all available worker replies. Returns true only when a current,
    /// live activation completed successfully and the host should dismiss.
    pub fn poll(&mut self) -> bool {
        let replies: Vec<_> = self
            .worker
            .as_ref()
            .map(|worker| worker.replies.try_iter().collect())
            .unwrap_or_default();
        let mut dismiss = false;
        for reply in replies {
            dismiss |= self.apply_reply(reply);
        }
        dismiss
    }

    pub fn activate(&mut self, index: usize) {
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
            worker.activate(self.generation, result.id.clone(), self.query.clone());
        }
    }

    pub fn retry(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        self.activating = false;
        self.error = None;
        worker.search(self.generation, self.query.clone());
        worker.retry();
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.response.results.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta)
            .clamp(0, self.response.results.len() as isize - 1) as usize;
    }

    fn accept_search(&mut self, generation: u64, response: SearchResponse) {
        if generation != self.generation {
            return;
        }
        self.response = response;
        self.response_generation = generation;
        self.error = None;
        self.pending = false;
        self.selected = self
            .selected
            .min(self.response.results.len().saturating_sub(1));
    }

    fn accept_error(&mut self, generation: u64, error: String) {
        if generation != self.generation {
            return;
        }
        self.pending = false;
        self.activating = false;
        self.error = Some(error);
    }

    fn apply_reply(&mut self, reply: Reply) -> bool {
        match reply {
            Reply::Ready => false,
            Reply::Search(generation, response) => {
                self.accept_search(generation, response);
                false
            }
            Reply::Error(generation, error) => {
                self.accept_error(generation, error);
                false
            }
            Reply::Activated(generation, result) if generation == self.generation => {
                self.activating = false;
                match result {
                    Ok(()) => true,
                    Err(error) => {
                        self.error = Some(error);
                        false
                    }
                }
            }
            Reply::Activated(_, _) => false,
        }
    }
}

fn empty_response() -> SearchResponse {
    SearchResponse {
        results: Vec::new(),
        semantic_status: "warming".into(),
        semantic_message: None,
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
                score: 100.0,
                reason: "Title match".into(),
            },
            SearchResult {
                id: "fixture:display".into(),
                kind: EntityKind::SystemAction,
                title: "Displays".into(),
                subtitle: "System Settings".into(),
                score: 90.0,
                reason: "Suggested".into(),
            },
            SearchResult {
                id: "fixture:file".into(),
                kind: EntityKind::File,
                title: "Project notes.md".into(),
                subtitle: "~/Documents".into(),
                score: 80.0,
                reason: "Filename match".into(),
            },
        ]
    } else {
        Vec::new()
    };
    SearchResponse {
        results,
        semantic_status: "unavailable".into(),
        semantic_message: Some("Fixture uses keyword mode".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_model(fixture: Fixture) -> Model {
        Model::new(Some(fixture), eframe::egui::Context::default())
    }

    #[test]
    fn fixtures_filter_title_and_subtitle_case_insensitively() {
        let mut model = fixture_model(Fixture::Results);
        model.query = "system settings".into();
        model.search();
        assert_eq!(model.response.results.len(), 1);
        assert_eq!(model.response.results[0].title, "Displays");
        assert!(!model.pending);

        model.query = "PROJECT".into();
        model.search();
        assert_eq!(model.response.results[0].id, "fixture:file");
    }

    #[test]
    fn stale_search_and_error_replies_are_rejected_asymmetrically() {
        let mut model = fixture_model(Fixture::Results);
        let old = model.generation;
        model.query = "browser".into();
        model.search();
        let current = model.generation;

        model.accept_error(old, "stale error".into());
        assert!(model.error.is_none());
        model.accept_search(old, empty_response());
        assert_eq!(model.response.results[0].title, "Browser");

        model.accept_error(current, "current error".into());
        assert_eq!(model.error.as_deref(), Some("current error"));
    }

    #[test]
    fn stale_activation_does_not_finish_current_activation() {
        let mut model = fixture_model(Fixture::Results);
        let stale = model.generation;
        model.search();
        let current = model.generation;
        model.activating = true;

        assert!(!model.apply_reply(Reply::Activated(stale, Ok(()))));
        assert!(model.activating);
        assert!(model.apply_reply(Reply::Activated(current, Ok(()))));
        assert!(!model.activating);
    }

    #[test]
    fn selection_is_clamped_and_error_fixture_cannot_activate() {
        let mut model = fixture_model(Fixture::Results);
        model.move_selection(99);
        assert_eq!(model.selected, 2);
        model.move_selection(-99);
        assert_eq!(model.selected, 0);

        let mut error = fixture_model(Fixture::Error);
        error.activate(0);
        assert!(!error.activating);
        assert_eq!(error.error.as_deref(), Some("Deterministic fixture error"));
    }

    #[test]
    fn activation_requires_current_ready_response_and_is_deduplicated() {
        let mut model = fixture_model(Fixture::Results);
        let work = Arc::new((Mutex::new(Work::default()), Condvar::new()));
        let (_sender, replies) = mpsc::channel();
        model.worker = Some(Worker {
            work: work.clone(),
            generation: Arc::new(AtomicU64::new(model.generation)),
            replies,
        });
        model.pending = true;
        model.activate(1);
        model.pending = false;
        model.error = Some("failed".into());
        model.activate(1);
        model.error = None;
        model.response_generation = model.generation - 1;
        model.activate(1);
        assert!(work.0.lock().unwrap().activations.is_empty());
        model.response_generation = model.generation;
        model.activate(1);
        model.activate(2);
        let queued = work.0.lock().unwrap();
        assert_eq!(queued.activations.len(), 1);
        assert_eq!(queued.activations[0].1, "fixture:display");
    }
}
