use findanything_core::model::{EntityKind, SearchResponse, SearchResult};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fixture {
    Results,
    Empty,
    Error,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn from_wide(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|c| *c == 0).unwrap_or(value.len())])
}

#[derive(Default)]
struct Work {
    search: Option<(u64, String)>,
    activations: VecDeque<(u64, String, String)>,
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
    fn spawn() -> Self {
        let work = Arc::new((Mutex::new(Work::default()), Condvar::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let (tx, replies) = mpsc::channel();
        let (thread_work, thread_generation) = (work.clone(), generation.clone());
        std::thread::Builder::new()
            .name("search-worker".into())
            .spawn(move || {
                let engine = match findanything_core::SearchEngine::new() {
                    Ok(engine) => {
                        let _ = tx.send(Reply::Ready);
                        Some(engine)
                    }
                    Err(error) => {
                        let _ = tx.send(Reply::Error(0, error));
                        None
                    }
                };
                loop {
                    let mut state = thread_work.0.lock().unwrap();
                    while state.search.is_none() && state.activations.is_empty() && !state.stop {
                        state = thread_work.1.wait(state).unwrap();
                    }
                    if state.stop {
                        break;
                    }
                    if let Some((g, id, query)) = state.activations.pop_front() {
                        drop(state);
                        if g != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        let answer = engine
                            .as_ref()
                            .ok_or_else(|| "Search engine is unavailable".into())
                            .and_then(|e| e.activate(&id, &query));
                        let _ = tx.send(Reply::Activated(g, answer));
                    } else if let Some((g, query)) = state.search.take() {
                        drop(state);
                        if g != thread_generation.load(Ordering::Acquire) {
                            continue;
                        }
                        match &engine {
                            Some(e) => {
                                let _ = tx.send(Reply::Search(g, e.search(&query)));
                            }
                            None => {
                                let _ =
                                    tx.send(Reply::Error(g, "Search engine is unavailable".into()));
                            }
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
    fn search(&self, g: u64, query: String) {
        self.generation.store(g, Ordering::Release);
        self.work.0.lock().unwrap().search = Some((g, query));
        self.work.1.notify_one();
    }
    fn activate(&self, g: u64, id: String, query: String) {
        if g != self.generation.load(Ordering::Acquire) {
            return;
        }
        self.work
            .0
            .lock()
            .unwrap()
            .activations
            .push_back((g, id, query));
        self.work.1.notify_one();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.work.0.lock().unwrap().stop = true;
        self.work.1.notify_one();
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
                subtitle: "Documents".into(),
                score: 80.0,
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

#[cfg(windows)]
mod win {
    use super::*;
    use std::cell::{RefCell, RefMut};
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };

    const EDIT: i32 = 101;
    const LIST: i32 = 102;
    const STATUS: i32 = 103;
    const RETRY: i32 = 104;
    const MENU_CHECK: usize = 201;
    const MENU_RESTART: usize = 202;
    const MENU_QUIT: usize = 203;
    const HOTKEY: i32 = 1;
    const TIMER: usize = 1;

    struct App {
        hwnd: HWND,
        edit: HWND,
        list: HWND,
        status: HWND,
        retry: HWND,
        response: SearchResponse,
        query: String,
        generation: u64,
        response_generation: u64,
        pending: bool,
        activating: bool,
        fixture: Option<Fixture>,
        worker: Option<Worker>,
        instance: Option<findanything_core::instance::Instance>,
        hotkey: bool,
        quitting: bool,
        font: HFONT,
        last_warming_poll: Instant,
    }
    impl App {
        fn request_search(&mut self) {
            self.generation += 1;
            self.pending = true;
            if let Some(worker) = &self.worker {
                worker.search(self.generation, self.query.clone());
            } else if let Some(f) = self.fixture {
                let mut r = fixture_response(f);
                let q = self.query.to_lowercase();
                r.results.retain(|x| {
                    format!("{} {}", x.title, x.subtitle)
                        .to_lowercase()
                        .contains(&q)
                });
                self.accept(self.generation, r);
            }
        }
        fn accept(&mut self, g: u64, response: SearchResponse) {
            if g != self.generation {
                return;
            }
            self.response = response;
            self.response_generation = g;
            self.pending = false;
            unsafe {
                ShowWindow(self.retry, SW_HIDE);
                SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
                for result in &self.response.results {
                    let label = wide(&format!(
                        "{}\t{} — {}",
                        result.title, result.subtitle, result.reason
                    ));
                    SendMessageW(self.list, LB_ADDSTRING, 0, label.as_ptr() as isize);
                }
                if !self.response.results.is_empty() {
                    SendMessageW(self.list, LB_SETCURSEL, 0, 0);
                }
                self.set_status(if self.fixture == Some(Fixture::Error) {
                    "Search hit a snag. Deterministic fixture error"
                } else if self.response.results.is_empty() {
                    "No local matches yet. Try an app, setting, or filename."
                } else {
                    "Up/Down Navigate   Enter Open   Esc Hide"
                });
            }
        }
        unsafe fn set_status(&self, text: &str) {
            unsafe {
                SetWindowTextW(self.status, wide(text).as_ptr());
            }
        }
        unsafe fn activate(&mut self) {
            unsafe {
                if self.pending || self.activating || self.response_generation != self.generation {
                    return;
                }
                let index = SendMessageW(self.list, LB_GETCURSEL, 0, 0) as usize;
                let Some(result) = self.response.results.get(index) else {
                    return;
                };
                if let Some(worker) = &self.worker {
                    self.activating = true;
                    EnableWindow(self.edit, 0);
                    worker.activate(self.generation, result.id.clone(), self.query.clone());
                }
            }
        }
        unsafe fn show(&self) {
            unsafe {
                ShowWindow(self.hwnd, SW_RESTORE);
                ShowWindow(self.hwnd, SW_SHOW);
                SetForegroundWindow(self.hwnd);
                SetFocus(self.edit);
                SendMessageW(self.edit, EM_SETSEL, 0, -1);
            }
        }
        unsafe fn dismiss(&self) {
            unsafe {
                if self.hotkey {
                    ShowWindow(self.hwnd, SW_HIDE);
                } else {
                    ShowWindow(self.hwnd, SW_MINIMIZE);
                }
            }
        }
    }

    // The Box<RefCell<App>> outlives the window/message loop. Native control
    // calls can synchronously reenter window_proc, so never alias &mut App.
    unsafe fn app<'a>(hwnd: HWND) -> Option<RefMut<'a, App>> {
        unsafe {
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>)
                .as_ref()?
                .try_borrow_mut()
                .ok()
        }
    }
    unsafe fn handle_key(hwnd: HWND, msg: u32, wp: WPARAM) -> bool {
        unsafe {
            if let Some(mut a) = app(hwnd) {
                match msg {
                    WM_KEYDOWN if wp as u16 == VK_ESCAPE => {
                        a.dismiss();
                        return true;
                    }
                    WM_KEYDOWN if wp as u16 == VK_RETURN => {
                        a.activate();
                        return true;
                    }
                    WM_KEYDOWN if wp as u16 == VK_DOWN || wp as u16 == VK_UP => {
                        let count = a.response.results.len();
                        if count > 0 {
                            let old = SendMessageW(a.list, LB_GETCURSEL, 0, 0).max(0) as usize;
                            let next = if wp as u16 == VK_DOWN {
                                (old + 1).min(count - 1)
                            } else {
                                old.saturating_sub(1)
                            };
                            SendMessageW(a.list, LB_SETCURSEL, next, 0);
                        }
                        return true;
                    }
                    _ => {}
                }
            }
            false
        }
    }

    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_NCCREATE => {
                    let cs = &*(lp as *const CREATESTRUCTW);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                    (&*(cs.lpCreateParams as *const RefCell<App>))
                        .borrow_mut()
                        .hwnd = hwnd;
                }
                WM_SIZE => {
                    if let Some(a) = app(hwnd) {
                        let w = (lp as u32 & 0xffff) as i32;
                        let h = ((lp as u32 >> 16) & 0xffff) as i32;
                        let dpi = GetDpiForWindow(hwnd) as i32;
                        let s = |v| v * dpi / 96;
                        MoveWindow(a.edit, s(12), s(12), w - s(24), s(34), 1);
                        MoveWindow(a.list, s(12), s(54), w - s(24), (h - s(96)).max(s(40)), 1);
                        MoveWindow(a.status, s(12), h - s(34), w - s(110), s(22), 1);
                        MoveWindow(a.retry, w - s(90), h - s(38), s(78), s(26), 1);
                    }
                }
                WM_GETMINMAXINFO => {
                    let info = &mut *(lp as *mut MINMAXINFO);
                    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                    info.ptMinTrackSize.x = 640 * dpi / 96;
                    info.ptMinTrackSize.y = 420 * dpi / 96;
                }
                WM_DPICHANGED => {
                    let r = &*(lp as *const RECT);
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                }
                WM_COMMAND => {
                    if let Some(mut a) = app(hwnd) {
                        let id = (wp & 0xffff) as i32;
                        let notify = ((wp >> 16) & 0xffff) as u16;
                        if id == EDIT && notify == EN_CHANGE as u16 {
                            let n = GetWindowTextLengthW(a.edit) as usize;
                            let mut text = vec![0u16; n + 1];
                            GetWindowTextW(a.edit, text.as_mut_ptr(), text.len() as i32);
                            a.query = from_wide(&text);
                            a.request_search();
                        } else if id == LIST && notify == LBN_SELCHANGE as u16 {
                            a.activate();
                        } else if id == RETRY {
                            a.worker = Some(Worker::spawn());
                            a.request_search();
                        } else if id as usize == MENU_CHECK {
                            findanything_core::updates::check();
                        } else if id as usize == MENU_RESTART {
                            if let Err(e) = findanything_core::updates::apply() {
                                a.set_status(&e);
                            }
                        } else if id as usize == MENU_QUIT {
                            a.quitting = true;
                            DestroyWindow(hwnd);
                        }
                    }
                }
                WM_HOTKEY => {
                    if let Some(a) = app(hwnd) {
                        if IsWindowVisible(hwnd) != 0 && GetForegroundWindow() == hwnd {
                            a.dismiss();
                        } else {
                            a.show();
                        }
                    }
                }
                WM_TIMER => {
                    if let Some(mut a) = app(hwnd) {
                        if a.instance
                            .as_ref()
                            .is_some_and(|i| i.take_activation_request())
                        {
                            a.show();
                        }
                        let replies: Vec<_> = a
                            .worker
                            .as_ref()
                            .map(|w| w.replies.try_iter().collect())
                            .unwrap_or_default();
                        for reply in replies {
                            match reply {
                                Reply::Ready => a.request_search(),
                                Reply::Search(g, r) => a.accept(g, r),
                                Reply::Error(g, e) if g == a.generation => {
                                    a.pending = false;
                                    a.set_status(&e);
                                    ShowWindow(a.retry, SW_SHOW);
                                }
                                Reply::Activated(g, result) if g == a.generation => {
                                    a.activating = false;
                                    EnableWindow(a.edit, 1);
                                    match result {
                                        Ok(()) => a.dismiss(),
                                        Err(e) => a.set_status(&e),
                                    }
                                }
                                _ => {}
                            }
                        }
                        if a.response.semantic_status == "warming"
                            && !a.pending
                            && !a.activating
                            && a.last_warming_poll.elapsed() >= Duration::from_millis(500)
                        {
                            a.last_warming_poll = Instant::now();
                            a.request_search();
                        }
                        let update = findanything_core::updates::status();
                        let menu = GetMenu(hwnd);
                        EnableMenuItem(
                            menu,
                            MENU_CHECK as u32,
                            MF_BYCOMMAND
                                | if a.fixture.is_none() {
                                    MF_ENABLED
                                } else {
                                    MF_GRAYED
                                },
                        );
                        EnableMenuItem(
                            menu,
                            MENU_RESTART as u32,
                            MF_BYCOMMAND
                                | if update.state == "ready" && a.fixture.is_none() {
                                    MF_ENABLED
                                } else {
                                    MF_GRAYED
                                },
                        );
                    }
                }
                WM_CLOSE => {
                    if let Some(a) = app(hwnd)
                        && a.hotkey
                        && !a.quitting
                    {
                        a.dismiss();
                        return 0;
                    }
                }
                WM_DESTROY => {
                    KillTimer(hwnd, TIMER);
                    UnregisterHotKey(hwnd, HOTKEY);
                    PostQuitMessage(0);
                }
                _ => {}
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }

    pub fn run(
        fixture: Option<Fixture>,
        theme: Option<&str>,
        instance: Option<findanything_core::instance::Instance>,
    ) -> Result<(), String> {
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let module = GetModuleHandleW(null());
            let class = wide("FindAnythingNativeWindow");
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: (COLOR_WINDOW as isize + 1) as HBRUSH,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&wc) == 0 {
                return Err("Cannot register native window".into());
            }
            let mut state = Box::new(RefCell::new(App {
                hwnd: null_mut(),
                edit: null_mut(),
                list: null_mut(),
                status: null_mut(),
                retry: null_mut(),
                response: fixture_response(Fixture::Empty),
                query: String::new(),
                generation: 0,
                response_generation: 0,
                pending: false,
                activating: false,
                fixture,
                worker: fixture.is_none().then(Worker::spawn),
                instance,
                hotkey: false,
                quitting: false,
                font: null_mut(),
                last_warming_poll: Instant::now(),
            }));
            let dpi = GetDpiForSystem() as i32;
            let width = 760 * dpi / 96;
            let height = 570 * dpi / 96;
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                wide("Find Anything").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width,
                height,
                null_mut(),
                null_mut(),
                module,
                &mut *state as *mut _ as _,
            );
            if hwnd.is_null() {
                return Err("Cannot create native window".into());
            }
            let mut state_ref = state.borrow_mut();
            let state = &mut *state_ref;
            let font = CreateFontW(
                -16 * dpi / 96,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Segoe UI").as_ptr(),
            );
            state.font = font;
            state.edit = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(),
                null(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                0,
                0,
                0,
                0,
                hwnd,
                EDIT as _,
                module,
                null(),
            );
            state.list = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("LISTBOX").as_ptr(),
                null(),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | LBS_NOTIFY as u32
                    | LBS_NOINTEGRALHEIGHT as u32,
                0,
                0,
                0,
                0,
                hwnd,
                LIST as _,
                module,
                null(),
            );
            state.status = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Starting…").as_ptr(),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                0,
                0,
                hwnd,
                STATUS as _,
                module,
                null(),
            );
            state.retry = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide("Retry").as_ptr(),
                WS_CHILD | BS_PUSHBUTTON as u32,
                0,
                0,
                0,
                0,
                hwnd,
                RETRY as _,
                module,
                null(),
            );
            for control in [state.edit, state.list, state.status, state.retry] {
                SendMessageW(control, WM_SETFONT, font as usize, 1);
            }
            let menu = CreateMenu();
            let app_menu = CreatePopupMenu();
            AppendMenuW(
                app_menu,
                MF_STRING,
                MENU_CHECK,
                wide("Check for updates").as_ptr(),
            );
            AppendMenuW(
                app_menu,
                MF_STRING,
                MENU_RESTART,
                wide("Restart to update").as_ptr(),
            );
            AppendMenuW(app_menu, MF_SEPARATOR, 0, null());
            AppendMenuW(app_menu, MF_STRING, MENU_QUIT, wide("Quit").as_ptr());
            AppendMenuW(
                menu,
                MF_POPUP,
                app_menu as usize,
                wide("Find Anything").as_ptr(),
            );
            SetMenu(hwnd, menu);
            if theme == Some("dark") {
                SetWindowTheme(hwnd, wide("DarkMode_Explorer").as_ptr(), null());
            } else if theme == Some("light") {
                SetWindowTheme(hwnd, wide("Explorer").as_ptr(), null());
            }
            state.hotkey = fixture.is_none()
                && RegisterHotKey(
                    hwnd,
                    HOTKEY,
                    MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                    VK_SPACE as u32,
                ) != 0;
            if fixture.is_none() && !state.hotkey {
                state.set_status(
                    "Ctrl+Shift+Space unavailable; this window remains on the taskbar.",
                );
            }
            SetTimer(hwnd, TIMER, 100, None);
            state.request_search();
            SetFocus(state.edit);
            drop(state_ref);
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            SendMessageW(
                hwnd,
                WM_SIZE,
                0,
                ((rect.bottom as u32) << 16 | rect.right as u32) as isize,
            );
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                if handle_key(hwnd, msg.message, msg.wParam) {
                    continue;
                }
                if IsDialogMessageW(hwnd, &msg) != 0 {
                    continue;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            DeleteObject(font);
            Ok(())
        }
    }
}

fn parse_args() -> (Option<Fixture>, Option<String>) {
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
            "--theme" => theme = args.next().filter(|v| v == "light" || v == "dark"),
            "--theme=light" => theme = Some("light".into()),
            "--theme=dark" => theme = Some("dark".into()),
            _ => {}
        }
    }
    (fixture, theme)
}

pub fn main() {
    let (fixture, theme) = parse_args();
    let instance = if fixture.is_none() {
        findanything_core::updates::initialize();
        match findanything_core::instance::Instance::acquire() {
            Ok(Some(i)) => Some(i),
            Ok(None) => return,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        }
    } else {
        None
    };
    if fixture.is_none() {
        findanything_core::updates::start();
    }
    #[cfg(windows)]
    if let Err(e) = win::run(fixture, theme.as_deref(), instance) {
        eprintln!("{e}");
    }
    #[cfg(not(windows))]
    {
        let _ = (fixture, theme, instance);
        eprintln!("findanything-windows requires Windows");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_round_trip_handles_non_bmp_and_nul() {
        let value = "C:\\資料\\🚀.txt";
        assert_eq!(from_wide(&wide(value)), value);
        assert_eq!(from_wide(&[65, 0, 66]), "A");
    }
    #[test]
    fn fixtures_are_deterministic() {
        assert_eq!(fixture_response(Fixture::Results).results.len(), 3);
        assert!(fixture_response(Fixture::Empty).results.is_empty());
        assert!(fixture_response(Fixture::Error).results.is_empty());
    }
}
