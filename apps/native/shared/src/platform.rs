//! OS integration only. Layout, input policy, search state and painting are shared.
use eframe::egui;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

pub enum Event {
    Toggle,
    Show,
    Update,
}

pub struct Desktop {
    _hotkey: Option<GlobalHotKeyManager>,
    pub can_hide: bool,
    pub message: Option<String>,
    pub events: mpsc::Receiver<Event>,
    stop: Arc<AtomicBool>,
}

impl Desktop {
    pub fn new(ctx: egui::Context, instance: findanything_core::instance::Instance) -> Self {
        use global_hotkey::hotkey::{Code, HotKey, Modifiers};
        let wayland = cfg!(target_os = "linux")
            && std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var("WINIT_UNIX_BACKEND").as_deref() != Ok("x11");
        let key = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
        let registration = if wayland {
            Err("Set a desktop shortcut to launch Find Anything on Wayland.".to_owned())
        } else {
            GlobalHotKeyManager::new()
                .and_then(|manager| {
                    manager.register(key)?;
                    Ok(manager)
                })
                .map_err(|e| format!("Ctrl+Shift+Space unavailable: {e}"))
        };
        let message = registration.as_ref().err().cloned();
        let manager = registration.ok();
        let can_hide = manager.is_some();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            let mut update = String::new();
            // Mailbox polling does not repaint the UI or keep the GPU active.
            while !stopping.load(Ordering::Acquire) {
                let mut changed = false;
                while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                    if event.id == key.id() && event.state == HotKeyState::Pressed {
                        let _ = tx.send(Event::Toggle);
                        changed = true;
                    }
                }
                if instance.take_activation_request() {
                    let _ = tx.send(Event::Show);
                    changed = true;
                }
                let status = findanything_core::updates::status();
                if update != status.message {
                    update = status.message;
                    let _ = tx.send(Event::Update);
                    changed = true;
                }
                if changed {
                    ctx.request_repaint();
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        Self {
            _hotkey: manager,
            can_hide,
            message,
            events,
            stop,
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub fn contrast_palette() -> Option<[u32; 4]> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW},
        };
        let mut value: HIGHCONTRASTW = std::mem::zeroed();
        value.cbSize = std::mem::size_of::<HIGHCONTRASTW>() as u32;
        let enabled = SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            value.cbSize,
            (&mut value as *mut HIGHCONTRASTW).cast(),
            0,
        ) != 0
            && value.dwFlags & HCF_HIGHCONTRASTON != 0;
        use windows_sys::Win32::Graphics::Gdi::{
            COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor,
        };
        enabled.then(|| {
            [
                COLOR_WINDOW,
                COLOR_WINDOWTEXT,
                COLOR_HIGHLIGHT,
                COLOR_HIGHLIGHTTEXT,
            ]
            .map(|index| {
                let bgr = GetSysColor(index);
                ((bgr & 255) << 16) | (bgr & 0xff00) | (bgr >> 16)
            })
        })
    }
    #[cfg(not(windows))]
    None
}
