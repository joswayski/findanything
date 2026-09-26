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
                subtitle: "~/Documents".into(),
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

    #[allow(dead_code)]
    mod graphite {
        include!("../../design/graphite.rs");
    }

    const EDIT: i32 = 101;
    const LIST: i32 = 102;
    const STATUS: i32 = 103;
    const RETRY: i32 = 104;
    const SECTION: i32 = 105;
    const MESSAGE: i32 = 106;
    const SHORTCUTS: i32 = 107;
    const MENU_BUTTON: i32 = 108;
    const MENU_CHECK: usize = 201;
    const MENU_RESTART: usize = 202;
    const MENU_QUIT: usize = 203;
    const HOTKEY: i32 = 1;
    const TIMER: usize = 1;
    const STATIC_CENTER: u32 = 1;
    const STATIC_RIGHT: u32 = 2;
    const HIGH_CONTRAST_ON: u32 = 1;

    #[repr(C)]
    struct HighContrast {
        size: u32,
        flags: u32,
        default_scheme: *mut u16,
    }

    struct App {
        hwnd: HWND,
        edit: HWND,
        list: HWND,
        status: HWND,
        retry: HWND,
        section: HWND,
        message: HWND,
        shortcuts: HWND,
        menu_button: HWND,
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
        title_font: HFONT,
        metadata_font: HFONT,
        brushes: [HBRUSH; 4],
        popup: HMENU,
        high_contrast: bool,
        last_warming_poll: Instant,
    }
    impl Drop for App {
        fn drop(&mut self) {
            unsafe {
                for object in [self.font, self.title_font, self.metadata_font]
                    .into_iter()
                    .chain(self.brushes)
                {
                    if !object.is_null() {
                        DeleteObject(object);
                    }
                }
                if !self.popup.is_null() {
                    DestroyMenu(self.popup);
                }
            }
        }
    }
    impl App {
        fn request_search(&mut self) {
            self.generation += 1;
            self.pending = true;
            unsafe {
                SetWindowTextW(
                    self.section,
                    wide(if self.query.is_empty() {
                        "Apps & actions"
                    } else {
                        "Best matches"
                    })
                    .as_ptr(),
                );
            }
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
                        "{} — {} — {}",
                        result.title, result.subtitle, result.reason
                    ));
                    SendMessageW(self.list, LB_ADDSTRING, 0, label.as_ptr() as isize);
                }
                if !self.response.results.is_empty() {
                    SendMessageW(self.list, LB_SETCURSEL, 0, 0);
                }
                let message = if self.fixture == Some(Fixture::Error) {
                    "Search hit a snag.\r\nDeterministic fixture error"
                } else if self.response.results.is_empty() {
                    "No local matches yet.\r\nTry an app, a setting, or a filename."
                } else {
                    ""
                };
                SetWindowTextW(self.message, wide(message).as_ptr());
                ShowWindow(
                    self.message,
                    if message.is_empty() { SW_HIDE } else { SW_SHOW },
                );
                self.set_status(match self.response.semantic_status.as_str() {
                    "ready" => "Semantic ready",
                    "unavailable" => "Keyword mode",
                    _ => "Preparing index",
                });
                InvalidateRect(self.list, null(), 1);
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
                    WM_KEYDOWN
                        if wp as u16 == VK_RETURN
                            && (GetFocus() == a.edit || GetFocus() == a.list) =>
                    {
                        a.activate();
                        return true;
                    }
                    WM_KEYDOWN
                        if (wp as u16 == VK_DOWN || wp as u16 == VK_UP)
                            && (GetFocus() == a.edit || GetFocus() == a.list) =>
                    {
                        let count = a.response.results.len();
                        if count > 0 {
                            let old = SendMessageW(a.list, LB_GETCURSEL, 0, 0).max(0) as usize;
                            let next = if wp as u16 == VK_DOWN {
                                (old + 1).min(count - 1)
                            } else {
                                old.saturating_sub(1)
                            };
                            SendMessageW(a.list, LB_SETCURSEL, next, 0);
                            InvalidateRect(a.list, null(), 0);
                        }
                        return true;
                    }
                    _ => {}
                }
            }
            false
        }
    }

    const fn colorref(rgb: u32) -> COLORREF {
        ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
    }
    fn scale(value: i32, dpi: i32) -> i32 {
        value * dpi / 96
    }
    unsafe fn make_font(dpi: i32, points: i32, weight: i32) -> HFONT {
        unsafe {
            CreateFontW(
                -points * dpi / 96,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Segoe UI").as_ptr(),
            )
        }
    }
    unsafe fn apply_fonts(a: &mut App, dpi: i32) {
        unsafe {
            for font in [a.font, a.title_font, a.metadata_font] {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
            a.font = make_font(dpi, graphite::BODY_SIZE, FW_NORMAL as i32);
            a.title_font = make_font(dpi, graphite::BODY_SIZE, FW_SEMIBOLD as i32);
            a.metadata_font = make_font(dpi, graphite::METADATA_SIZE, FW_NORMAL as i32);
            for control in [a.edit, a.message, a.retry, a.menu_button] {
                if !control.is_null() {
                    SendMessageW(control, WM_SETFONT, a.font as usize, 1);
                }
            }
            for control in [a.section, a.status, a.shortcuts] {
                if !control.is_null() {
                    SendMessageW(control, WM_SETFONT, a.metadata_font as usize, 1);
                }
            }
            if !a.list.is_null() {
                SendMessageW(a.list, WM_SETFONT, a.font as usize, 1);
                SendMessageW(
                    a.list,
                    LB_SETITEMHEIGHT,
                    0,
                    scale(graphite::ROW_HEIGHT, dpi) as isize,
                );
            }
        }
    }
    unsafe fn draw_icon(dc: HDC, kind: EntityKind, x: i32, y: i32, size: i32, color: COLORREF) {
        unsafe {
            let pen = CreatePen(PS_SOLID, (size / 10).max(1), color);
            let old = SelectObject(dc, pen);
            let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
            let p = size / 5;
            match kind {
                EntityKind::Application => {
                    Rectangle(dc, x + p, y + p, x + size - p, y + size - p);
                    MoveToEx(dc, x + p, y + size * 2 / 5, null_mut());
                    LineTo(dc, x + size - p, y + size * 2 / 5);
                }
                EntityKind::File => {
                    MoveToEx(dc, x + p, y + p, null_mut());
                    LineTo(dc, x + size * 3 / 5, y + p);
                    LineTo(dc, x + size - p, y + size * 2 / 5);
                    LineTo(dc, x + size - p, y + size - p);
                    LineTo(dc, x + p, y + size - p);
                    LineTo(dc, x + p, y + p);
                }
                _ => {
                    for (row, knob) in [(1, 2), (2, 3), (3, 2)] {
                        let line_y = y + size * row / 4;
                        let knob_x = x + size * knob / 5;
                        MoveToEx(dc, x + p, line_y, null_mut());
                        LineTo(dc, x + size - p, line_y);
                        MoveToEx(dc, knob_x, line_y - p / 2, null_mut());
                        LineTo(dc, knob_x, line_y + p / 2);
                    }
                }
            }
            SelectObject(dc, old_brush);
            SelectObject(dc, old);
            DeleteObject(pen);
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
                        let s = |v| scale(v, dpi);
                        let inset = s(graphite::INSET);
                        let search_y = inset;
                        MoveWindow(
                            a.edit,
                            inset + s(32),
                            search_y + s(12),
                            w - inset * 2 - s(92),
                            s(20),
                            1,
                        );
                        MoveWindow(
                            a.menu_button,
                            w - inset - s(graphite::SEARCH_HEIGHT),
                            search_y,
                            s(graphite::SEARCH_HEIGHT),
                            s(graphite::SEARCH_HEIGHT),
                            1,
                        );
                        let section_y = search_y + s(graphite::SEARCH_HEIGHT + graphite::GAP);
                        MoveWindow(
                            a.section,
                            inset,
                            section_y,
                            w - inset * 2,
                            s(graphite::SECTION_HEIGHT),
                            1,
                        );
                        let list_y = section_y + s(graphite::SECTION_HEIGHT + graphite::GAP);
                        let footer_y = h - inset - s(graphite::FOOTER_HEIGHT);
                        MoveWindow(
                            a.list,
                            inset,
                            list_y,
                            w - inset * 2,
                            (footer_y - s(graphite::GAP) - list_y).max(s(graphite::ROW_HEIGHT)),
                            1,
                        );
                        MoveWindow(
                            a.message,
                            inset,
                            list_y + (footer_y - list_y) / 2 - s(22),
                            w - inset * 2,
                            s(44),
                            1,
                        );
                        MoveWindow(
                            a.status,
                            inset,
                            footer_y,
                            (w / 2 - inset).max(1),
                            s(graphite::FOOTER_HEIGHT),
                            1,
                        );
                        MoveWindow(
                            a.shortcuts,
                            w / 2,
                            footer_y,
                            (w / 2 - inset).max(1),
                            s(graphite::FOOTER_HEIGHT),
                            1,
                        );
                        MoveWindow(
                            a.retry,
                            w / 2 - s(40),
                            list_y + (footer_y - list_y) / 2 + s(32),
                            s(80),
                            s(28),
                            1,
                        );
                        InvalidateRect(a.list, null(), 0);
                        InvalidateRect(hwnd, null(), 1);
                    }
                }
                WM_GETMINMAXINFO => {
                    let info = &mut *(lp as *mut MINMAXINFO);
                    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                    info.ptMinTrackSize.x = scale(graphite::MINIMUM_WIDTH, dpi);
                    info.ptMinTrackSize.y = scale(graphite::MINIMUM_HEIGHT, dpi);
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
                    if let Some(mut a) = app(hwnd) {
                        apply_fonts(&mut a, GetDpiForWindow(hwnd) as i32);
                    }
                }
                WM_ERASEBKGND => return 1,
                WM_PAINT => {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let dc = BeginPaint(hwnd, &mut ps);
                    if let Some(a) = app(hwnd) {
                        let mut r: RECT = std::mem::zeroed();
                        GetClientRect(hwnd, &mut r);
                        FillRect(
                            dc,
                            &r,
                            if a.high_contrast {
                                GetSysColorBrush(COLOR_WINDOW)
                            } else {
                                a.brushes[0]
                            },
                        );
                        if !a.high_contrast {
                            let dpi = GetDpiForWindow(hwnd) as i32;
                            let s = |v| scale(v, dpi);
                            let focused = GetFocus() == a.edit;
                            let pen = CreatePen(
                                PS_SOLID,
                                s(if focused { 2 } else { 1 }),
                                colorref(if focused {
                                    graphite::ACCENT
                                } else {
                                    graphite::BORDER_STRONG
                                }),
                            );
                            let old_pen = SelectObject(dc, pen);
                            let old_brush = SelectObject(dc, a.brushes[2]);
                            RoundRect(
                                dc,
                                s(graphite::INSET),
                                s(graphite::INSET),
                                r.right
                                    - s(graphite::INSET + graphite::SEARCH_HEIGHT + graphite::GAP),
                                s(graphite::INSET + graphite::SEARCH_HEIGHT),
                                s(graphite::RADIUS * 2),
                                s(graphite::RADIUS * 2),
                            );
                            SelectObject(dc, old_brush);
                            SelectObject(dc, old_pen);
                            DeleteObject(pen);
                            let search_pen =
                                CreatePen(PS_SOLID, s(2), colorref(graphite::SECONDARY));
                            let old_pen = SelectObject(dc, search_pen);
                            let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
                            let x = s(graphite::INSET + 12);
                            let y = s(graphite::INSET + 13);
                            Ellipse(dc, x, y, x + s(10), y + s(10));
                            MoveToEx(dc, x + s(8), y + s(8), null_mut());
                            LineTo(dc, x + s(14), y + s(14));
                            SelectObject(dc, old_brush);
                            SelectObject(dc, old_pen);
                            DeleteObject(search_pen);
                        }
                    }
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
                    if let Some(a) = app(hwnd) {
                        let dc = wp as HDC;
                        if a.high_contrast {
                            return DefWindowProcW(hwnd, msg, wp, lp);
                        }
                        SetTextColor(
                            dc,
                            colorref(
                                if lp as HWND == a.message
                                    && (a.fixture == Some(Fixture::Error)
                                        || IsWindowVisible(a.retry) != 0)
                                {
                                    graphite::DANGER
                                } else if lp as HWND == a.status
                                    || lp as HWND == a.shortcuts
                                    || lp as HWND == a.section
                                    || lp as HWND == a.message
                                {
                                    graphite::MUTED
                                } else {
                                    graphite::TEXT
                                },
                            ),
                        );
                        SetBkColor(
                            dc,
                            colorref(if lp as HWND == a.edit {
                                graphite::CONTROL
                            } else {
                                graphite::BG
                            }),
                        );
                        SetBkMode(dc, TRANSPARENT as i32);
                        return if lp as HWND == a.edit || lp as HWND == a.menu_button {
                            a.brushes[2]
                        } else {
                            a.brushes[0]
                        } as isize;
                    }
                }
                WM_DRAWITEM => {
                    let d = &*(lp as *const DRAWITEMSTRUCT);
                    if (d.CtlID == MENU_BUTTON as u32 || d.CtlID == RETRY as u32)
                        && let Some(a) = app(hwnd)
                    {
                        let saved_dc = SaveDC(d.hDC);
                        FillRect(
                            d.hDC,
                            &d.rcItem,
                            if a.high_contrast {
                                GetSysColorBrush(COLOR_BTNFACE)
                            } else {
                                a.brushes[0]
                            },
                        );
                        if !a.high_contrast {
                            let dpi = GetDpiForWindow(hwnd) as i32;
                            SelectObject(d.hDC, GetStockObject(NULL_PEN));
                            SelectObject(d.hDC, a.brushes[2]);
                            RoundRect(
                                d.hDC,
                                d.rcItem.left,
                                d.rcItem.top,
                                d.rcItem.right,
                                d.rcItem.bottom,
                                scale(graphite::RADIUS * 2, dpi),
                                scale(graphite::RADIUS * 2, dpi),
                            );
                        }
                        SetBkMode(d.hDC, TRANSPARENT as i32);
                        SetTextColor(
                            d.hDC,
                            if a.high_contrast {
                                GetSysColor(COLOR_BTNTEXT)
                            } else {
                                colorref(graphite::SECONDARY)
                            },
                        );
                        SelectObject(d.hDC, a.font);
                        let mut r = d.rcItem;
                        if d.CtlID == RETRY as u32 {
                            DrawTextW(
                                d.hDC,
                                wide("Retry").as_ptr(),
                                -1,
                                &mut r,
                                DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                            );
                        } else {
                            let dpi = GetDpiForWindow(hwnd) as i32;
                            let s = |v| scale(v, dpi);
                            let pen = CreatePen(
                                PS_SOLID,
                                s(2),
                                if a.high_contrast {
                                    GetSysColor(COLOR_BTNTEXT)
                                } else {
                                    colorref(graphite::SECONDARY)
                                },
                            );
                            let old = SelectObject(d.hDC, pen);
                            let cx = (r.left + r.right) / 2;
                            let cy = (r.top + r.bottom) / 2;
                            for dy in [-5, 0, 5] {
                                MoveToEx(d.hDC, cx - s(7), cy + s(dy), null_mut());
                                LineTo(d.hDC, cx + s(7), cy + s(dy));
                            }
                            SelectObject(d.hDC, old);
                            DeleteObject(pen);
                        }
                        if d.itemState & ODS_FOCUS != 0 {
                            DrawFocusRect(d.hDC, &r);
                        }
                        RestoreDC(d.hDC, saved_dc);
                        return 1;
                    }
                    if d.CtlID == LIST as u32
                        && d.itemID != u32::MAX
                        && let Some(a) = app(hwnd)
                    {
                        let saved_dc = SaveDC(d.hDC);
                        let selected = d.itemState & ODS_SELECTED != 0;
                        let bg = if a.high_contrast {
                            GetSysColorBrush(if selected {
                                COLOR_HIGHLIGHT
                            } else {
                                COLOR_WINDOW
                            })
                        } else {
                            a.brushes[0]
                        };
                        FillRect(d.hDC, &d.rcItem, bg);
                        if selected && !a.high_contrast {
                            let dpi = GetDpiForWindow(hwnd) as i32;
                            SelectObject(d.hDC, GetStockObject(NULL_PEN));
                            SelectObject(d.hDC, a.brushes[3]);
                            RoundRect(
                                d.hDC,
                                d.rcItem.left,
                                d.rcItem.top,
                                d.rcItem.right,
                                d.rcItem.bottom,
                                scale(graphite::RADIUS * 2, dpi),
                                scale(graphite::RADIUS * 2, dpi),
                            );
                        }
                        if let Some(item) = a.response.results.get(d.itemID as usize) {
                            let dpi = GetDpiForWindow(hwnd) as i32;
                            let s = |v| scale(v, dpi);
                            if selected && !a.high_contrast {
                                let edge = RECT {
                                    left: d.rcItem.left,
                                    top: d.rcItem.top,
                                    right: d.rcItem.left + s(2),
                                    bottom: d.rcItem.bottom,
                                };
                                let brush = CreateSolidBrush(colorref(graphite::ACCENT));
                                FillRect(d.hDC, &edge, brush);
                                DeleteObject(brush);
                            }
                            SetBkMode(d.hDC, TRANSPARENT as i32);
                            let icon_x = d.rcItem.left + s(12);
                            let icon_y = d.rcItem.top
                                + (d.rcItem.bottom - d.rcItem.top - s(graphite::ICON_SIZE)) / 2;
                            let primary = if a.high_contrast {
                                GetSysColor(if selected {
                                    COLOR_HIGHLIGHTTEXT
                                } else {
                                    COLOR_WINDOWTEXT
                                })
                            } else {
                                colorref(graphite::TEXT_STRONG)
                            };
                            let secondary = if a.high_contrast {
                                primary
                            } else {
                                colorref(graphite::MUTED)
                            };
                            draw_icon(
                                d.hDC,
                                item.kind,
                                icon_x,
                                icon_y,
                                s(graphite::ICON_SIZE),
                                secondary,
                            );
                            let left = icon_x + s(graphite::ICON_SIZE + 12);
                            let mut title = RECT {
                                left,
                                top: d.rcItem.top + s(12),
                                right: d.rcItem.right - s(8),
                                bottom: d.rcItem.bottom,
                            };
                            SelectObject(d.hDC, a.title_font);
                            SetTextColor(d.hDC, primary);
                            DrawTextW(
                                d.hDC,
                                wide(&item.title).as_ptr(),
                                -1,
                                &mut title,
                                DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                            );
                            let mut sub = RECT {
                                left,
                                top: d.rcItem.top + s(29),
                                right: d.rcItem.right - s(8),
                                bottom: d.rcItem.bottom,
                            };
                            SelectObject(d.hDC, a.metadata_font);
                            SetTextColor(d.hDC, secondary);
                            DrawTextW(
                                d.hDC,
                                wide(&format!("{} · {}", item.subtitle, item.reason)).as_ptr(),
                                -1,
                                &mut sub,
                                DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                            );
                        }
                        RestoreDC(d.hDC, saved_dc);
                        return 1;
                    }
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
                        } else if id == EDIT
                            && (notify == EN_SETFOCUS as u16 || notify == EN_KILLFOCUS as u16)
                        {
                            InvalidateRect(hwnd, null(), 0);
                        } else if id == LIST && notify == LBN_SELCHANGE as u16 {
                            a.activate();
                        } else if id == RETRY {
                            a.worker = Some(Worker::spawn());
                            a.request_search();
                        } else if id == MENU_BUTTON {
                            let mut r: RECT = std::mem::zeroed();
                            GetWindowRect(a.menu_button, &mut r);
                            let popup = a.popup;
                            drop(a); // Popup loops dispatch paint and command messages reentrantly.
                            let command = TrackPopupMenu(
                                popup,
                                TPM_RIGHTALIGN | TPM_TOPALIGN | TPM_RETURNCMD,
                                r.right,
                                r.bottom,
                                0,
                                hwnd,
                                null(),
                            );
                            if command != 0 {
                                PostMessageW(hwnd, WM_COMMAND, command as usize, 0);
                            }
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
                                    SetWindowTextW(
                                        a.message,
                                        wide(&format!("Search hit a snag\r\n{e}")).as_ptr(),
                                    );
                                    ShowWindow(a.message, SW_SHOW);
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
                        EnableMenuItem(
                            a.popup,
                            MENU_CHECK as u32,
                            MF_BYCOMMAND
                                | if a.fixture.is_none() {
                                    MF_ENABLED
                                } else {
                                    MF_GRAYED
                                },
                        );
                        EnableMenuItem(
                            a.popup,
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
                hbrBackground: null_mut(),
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&wc) == 0 {
                return Err("Cannot register native window".into());
            }
            let mut hc: HighContrast = std::mem::zeroed();
            hc.size = std::mem::size_of::<HighContrast>() as u32;
            let high_contrast =
                SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.size, &mut hc as *mut _ as _, 0) != 0
                    && hc.flags & HIGH_CONTRAST_ON != 0;
            let popup = CreatePopupMenu();
            let mut state = Box::new(RefCell::new(App {
                hwnd: null_mut(),
                edit: null_mut(),
                list: null_mut(),
                status: null_mut(),
                retry: null_mut(),
                section: null_mut(),
                message: null_mut(),
                shortcuts: null_mut(),
                menu_button: null_mut(),
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
                title_font: null_mut(),
                metadata_font: null_mut(),
                brushes: [
                    CreateSolidBrush(colorref(graphite::BG)),
                    CreateSolidBrush(colorref(graphite::CHROME)),
                    CreateSolidBrush(colorref(graphite::CONTROL)),
                    CreateSolidBrush(colorref(graphite::SELECTED)),
                ],
                popup,
                high_contrast,
                last_warming_poll: Instant::now(),
            }));
            let dpi = GetDpiForSystem() as i32;
            let mut window_rect = RECT {
                left: 0,
                top: 0,
                right: scale(graphite::WINDOW_WIDTH, dpi),
                bottom: scale(graphite::WINDOW_HEIGHT, dpi),
            };
            AdjustWindowRectExForDpi(&mut window_rect, WS_OVERLAPPEDWINDOW, 0, 0, dpi as u32);
            let width = window_rect.right - window_rect.left;
            let height = window_rect.bottom - window_rect.top;
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                wide("Find Anything").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
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
            state.edit = CreateWindowExW(
                0,
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
            SendMessageW(
                state.edit,
                EM_SETCUEBANNER,
                1,
                wide("Find anything…").as_ptr() as isize,
            );
            state.list = CreateWindowExW(
                0,
                wide("LISTBOX").as_ptr(),
                null(),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | LBS_NOTIFY as u32
                    | LBS_NOINTEGRALHEIGHT as u32
                    | LBS_OWNERDRAWFIXED as u32
                    | LBS_HASSTRINGS as u32,
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
            state.section = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Apps & actions").as_ptr(),
                WS_CHILD | WS_VISIBLE | 0x80, // SS_NOPREFIX: display the literal ampersand.
                0,
                0,
                0,
                0,
                hwnd,
                SECTION as _,
                module,
                null(),
            );
            state.message = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                null(),
                WS_CHILD | STATIC_CENTER,
                0,
                0,
                0,
                0,
                hwnd,
                MESSAGE as _,
                module,
                null(),
            );
            state.shortcuts = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("↑/↓ Navigate   Enter Open   Esc Close").as_ptr(),
                WS_CHILD | WS_VISIBLE | STATIC_RIGHT,
                0,
                0,
                0,
                0,
                hwnd,
                SHORTCUTS as _,
                module,
                null(),
            );
            state.menu_button = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide("Menu").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
                0,
                0,
                0,
                0,
                hwnd,
                MENU_BUTTON as _,
                module,
                null(),
            );
            state.retry = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide("Retry").as_ptr(),
                WS_CHILD | WS_TABSTOP | BS_OWNERDRAW as u32,
                0,
                0,
                0,
                0,
                hwnd,
                RETRY as _,
                module,
                null(),
            );
            AppendMenuW(
                popup,
                MF_STRING,
                MENU_CHECK,
                wide("Check for updates").as_ptr(),
            );
            AppendMenuW(
                popup,
                MF_STRING,
                MENU_RESTART,
                wide("Restart to update").as_ptr(),
            );
            AppendMenuW(popup, MF_SEPARATOR, 0, null());
            AppendMenuW(popup, MF_STRING, MENU_QUIT, wide("Quit").as_ptr());
            apply_fonts(state, dpi);
            if !high_contrast {
                SetWindowTheme(hwnd, wide("DarkMode_Explorer").as_ptr(), null());
                SetWindowTheme(state.edit, wide("DarkMode_CFD").as_ptr(), null());
            }
            let _ = theme; // Graphite is deliberately dark regardless of the system/theme argument.
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
