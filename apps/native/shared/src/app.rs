use crate::{
    model::{Fixture, Model},
    platform::{Desktop, Event},
};
use eframe::egui::{
    self, Align2, Color32, FontId, Key, Rect, RichText, Sense, Stroke, Vec2, pos2, vec2,
};
use findanything_core::model::EntityKind;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[allow(dead_code)]
mod tokens {
    include!("../../design/graphite.rs");
}
mod lucide {
    include!("../../design/lucide.rs");
}

fn color(rgb: u32) -> Color32 {
    let rgb = contrast().map_or(rgb, |[background, text, selection, _]| match rgb {
        tokens::BG | tokens::CHROME | tokens::CONTROL | tokens::CONTROL_HOVER | tokens::POPOVER => {
            background
        }
        tokens::SELECTED | tokens::ACCENT => selection,
        _ => text,
    });
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn contrast() -> Option<[u32; 4]> {
    static PALETTE: std::sync::OnceLock<Option<[u32; 4]>> = std::sync::OnceLock::new();
    *PALETTE.get_or_init(crate::platform::contrast_palette)
}

fn row_ink(selected: bool, normal: u32) -> Color32 {
    if selected && let Some([_, _, _, rgb]) = contrast() {
        return Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    }
    color(normal)
}
fn font(size: i32) -> FontId {
    FontId::proportional(size as f32)
}
fn semibold(size: i32) -> FontId {
    FontId::new(size as f32, egui::FontFamily::Name("semibold".into()))
}

fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            "inter",
            include_bytes!("../../design/fonts/Inter-Regular.ttf").as_slice(),
        ),
        (
            "semibold",
            include_bytes!("../../design/fonts/Inter-SemiBold.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), egui::FontData::from_static(bytes).into());
    }
    let mut fallback = fonts.families[&egui::FontFamily::Proportional].clone();
    fallback.insert(0, "semibold".into());
    fonts
        .families
        .insert(egui::FontFamily::Name("semibold".into()), fallback);
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "inter".into());
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = color(tokens::BG);
    style.visuals.window_fill = color(tokens::POPOVER);
    style.visuals.override_text_color = Some(color(tokens::TEXT));
    style.visuals.selection.bg_fill = color(tokens::SELECTED);
    style.visuals.selection.stroke = Stroke::new(1., color(tokens::ACCENT));
    style.spacing.item_spacing = vec2(8., 8.);
    style.spacing.button_padding = vec2(12., 8.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, font(tokens::BODY_SIZE));
    style
        .text_styles
        .insert(egui::TextStyle::Button, font(tokens::METADATA_SIZE));
    ctx.set_style_of(egui::Theme::Dark, style);
}

fn icon(ui: &egui::Ui, icon: lucide::Icon, rect: Rect, ink: Color32) {
    let scale = rect.width().min(rect.height()) / lucide::VIEWBOX as f32;
    for path in icon.paths() {
        let points = path
            .iter()
            .map(|&(x, y)| rect.min + vec2(x as f32, y as f32) * scale)
            .collect();
        ui.painter().add(egui::Shape::line(
            points,
            Stroke::new(lucide::STROKE_WIDTH as f32 * scale, ink),
        ));
    }
}

fn icon_button(
    ui: &mut egui::Ui,
    id: &str,
    rect: Rect,
    glyph: lucide::Icon,
    label: &str,
) -> egui::Response {
    let response = ui.interact(rect, ui.scope_id().with(id), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS as u8, color(tokens::CONTROL_HOVER));
    }
    icon(
        ui,
        glyph,
        Rect::from_center_size(rect.center(), vec2(16., 16.)),
        color(tokens::SECONDARY),
    );
    response.on_hover_text(label)
}

#[derive(Default)]
struct Options {
    fixture: Option<Fixture>,
    state: String,
    screenshot: Option<PathBuf>,
    probe: Option<PathBuf>,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = args
            .into_iter()
            .flat_map(|arg| {
                if arg.starts_with("--")
                    && let Some((flag, value)) = arg.split_once('=')
                {
                    vec![flag.to_owned(), value.to_owned()]
                } else {
                    vec![arg]
                }
            })
            .peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--fixture" => {
                    result.state = args
                        .next_if(|s| !s.starts_with('-'))
                        .unwrap_or_else(|| "results".into());
                    result.fixture = Some(match result.state.as_str() {
                        "empty" => Fixture::Empty,
                        "error" => Fixture::Error,
                        "results" | "selected" | "query" | "minimum" => Fixture::Results,
                        state => return Err(format!("Unknown fixture: {state}")),
                    });
                }
                "--screenshot" | "--probe" => {
                    let path = args
                        .next_if(|s| !s.starts_with("--") && !s.is_empty())
                        .ok_or_else(|| format!("{arg} requires a path"))?;
                    if arg == "--screenshot" {
                        result.screenshot = Some(path.into());
                    } else {
                        result.probe = Some(path.into());
                    }
                }
                "--theme" | "--appearance" => match args.next().as_deref() {
                    Some("light" | "dark") => {}
                    _ => return Err(format!("{arg} requires light or dark")),
                },
                _ => return Err(format!("Unknown argument: {arg}")),
            }
        }
        if (result.screenshot.is_some() || result.probe.is_some()) && result.fixture.is_none() {
            return Err("Screenshots/probes require isolated --fixture mode".into());
        }
        Ok(result)
    }
}

struct Launcher {
    model: Model,
    desktop: Option<Desktop>,
    focus_query: bool,
    hidden: bool,
    menu: bool,
    quitting: bool,
    warming: Instant,
    screenshot: Option<PathBuf>,
    probe: Option<PathBuf>,
    started: Instant,
    scroll_selected: bool,
}

impl Launcher {
    fn new(
        ctx: &egui::Context,
        options: Options,
        instance: Option<findanything_core::instance::Instance>,
    ) -> Self {
        configure(ctx);
        let mut model = Model::new(options.fixture, ctx.clone());
        if options.state == "query" {
            model.query = "display".into();
            model.search();
        }
        if options.state == "selected" {
            model.move_selection(1);
        }
        Self {
            model,
            desktop: instance.map(|i| Desktop::new(ctx.clone(), i)),
            focus_query: true,
            hidden: false,
            menu: false,
            quitting: false,
            warming: Instant::now(),
            screenshot: options.screenshot,
            probe: options.probe,
            started: Instant::now(),
            scroll_selected: false,
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        self.focus_query = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn dismiss(&mut self, ctx: &egui::Context) {
        self.menu = false;
        if self.desktop.as_ref().is_some_and(|d| d.can_hide) {
            self.hidden = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let (header, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), tokens::SEARCH_HEIGHT as f32),
            Sense::hover(),
        );
        let menu_rect = Rect::from_min_max(pos2(header.right() - 52., header.top()), header.max);
        let search = Rect::from_min_max(header.min, pos2(menu_rect.left() - 8., header.bottom()));
        ui.painter()
            .rect_filled(search, tokens::RADIUS as u8, color(tokens::CONTROL));
        icon(
            ui,
            lucide::Icon::Search,
            Rect::from_center_size(pos2(search.left() + 20., search.center().y), vec2(16., 16.)),
            color(tokens::SECONDARY),
        );
        let text_rect = Rect::from_min_max(
            pos2(search.left() + 38., search.center().y - 13.),
            pos2(search.right() - 38., search.center().y + 13.),
        );
        if self.focus_query {
            ui.memory_mut(|m| m.request_focus(egui::Id::unique("query")));
            self.focus_query = false;
        }
        let response = ui
            .add_enabled_ui(!self.model.activating, |ui| {
                ui.put(
                    text_rect,
                    egui::TextEdit::singleline(&mut self.model.query)
                        .id(egui::Id::unique("query"))
                        .event_filter(egui::EventFilter {
                            vertical_arrows: true,
                            tab: false,
                            ..Default::default()
                        })
                        .font(font(tokens::SEARCH_SIZE))
                        .frame(egui::Frame::NONE)
                        .hint_text("Find anything…")
                        .margin(Vec2::ZERO)
                        .desired_width(text_rect.width()),
                )
            })
            .inner;
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::TextEdit,
                !self.model.activating,
                "Find anything",
            )
        });
        ui.painter().rect_stroke(
            search,
            tokens::RADIUS as u8,
            Stroke::new(
                1.,
                color(if response.has_focus() {
                    tokens::ACCENT
                } else {
                    tokens::BORDER_STRONG
                }),
            ),
            egui::StrokeKind::Inside,
        );
        if response.changed() {
            self.model.selected = 0;
            self.model.search();
        }
        if !self.model.query.is_empty() {
            let clear = Rect::from_center_size(
                pos2(search.right() - 20., search.center().y),
                vec2(30., 30.),
            );
            if icon_button(ui, "clear", clear, lucide::Icon::X, "Clear search").clicked()
                && !self.model.activating
            {
                self.model.query.clear();
                self.model.search();
                self.focus_query = true;
                ui.ctx().request_repaint();
            }
        }
        ui.painter()
            .rect_filled(menu_rect, tokens::RADIUS as u8, color(tokens::CONTROL));
        let menu_button = icon_button(ui, "menu", menu_rect, lucide::Icon::Menu, "Menu");
        if menu_button.clicked() {
            self.menu = !self.menu;
        }
        egui::Popup::from_response(&menu_button)
            .open_bool(&mut self.menu)
            .align(egui::RectAlign::BOTTOM_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(224.);
                let status = findanything_core::updates::status();
                ui.label(
                    RichText::new(&status.message)
                        .size(12.)
                        .color(color(tokens::MUTED)),
                );
                if ui
                    .add_enabled(
                        self.desktop.is_some(),
                        egui::Button::new("Check for updates"),
                    )
                    .clicked()
                {
                    findanything_core::updates::check();
                }
                if ui
                    .add_enabled(
                        status.state == "ready",
                        egui::Button::new("Restart to update"),
                    )
                    .clicked()
                    && let Err(e) = findanything_core::updates::apply()
                {
                    self.model.error = Some(e);
                }
                if ui.button("Quit Find Anything").clicked() {
                    self.quitting = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
    }

    fn results(&mut self, ui: &mut egui::Ui, height: f32) {
        let available = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
        if let Some(error) = self.model.error.clone() {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(available)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
                |ui| {
                    ui.add_space((height - 90.).max(0.) / 2.);
                    ui.label(
                        RichText::new("Search unavailable")
                            .font(semibold(tokens::BODY_SIZE))
                            .color(color(tokens::DANGER)),
                    );
                    ui.label(RichText::new(error).size(12.));
                    if ui.button("Retry").clicked() {
                        self.model.retry();
                    }
                },
            );
            ui.advance_cursor_after_rect(available);
            return;
        }
        if self.model.response.results.is_empty() {
            ui.painter().text(
                available.center(),
                Align2::CENTER_CENTER,
                if self.model.pending {
                    "Starting search…"
                } else {
                    "No results"
                },
                font(tokens::BODY_SIZE),
                color(tokens::MUTED),
            );
            ui.advance_cursor_after_rect(available);
            return;
        }
        let mut activate = None;
        egui::ScrollArea::vertical()
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                for (i, result) in self.model.response.results.iter().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), tokens::ROW_HEIGHT as f32),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            !self.model.pending,
                            i == self.model.selected,
                            format!("{} — {}", result.title, result.subtitle),
                        )
                    });
                    let selected = i == self.model.selected;
                    if selected && self.scroll_selected {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                    let row = rect.shrink2(vec2(0., 2.));
                    if selected || response.hovered() {
                        ui.painter().rect_filled(
                            row,
                            tokens::RADIUS as u8,
                            color(if selected {
                                tokens::SELECTED
                            } else {
                                tokens::CONTROL_HOVER
                            }),
                        );
                    }
                    if selected {
                        ui.painter().rect_filled(
                            Rect::from_min_size(row.min, vec2(2., row.height())),
                            1.,
                            color(tokens::ACCENT),
                        );
                    }
                    let glyph = match result.kind {
                        EntityKind::Application => lucide::Icon::AppWindow,
                        EntityKind::File => lucide::Icon::File,
                        EntityKind::SystemAction => lucide::Icon::SlidersHorizontal,
                    };
                    icon(
                        ui,
                        glyph,
                        Rect::from_center_size(
                            pos2(rect.left() + 22., rect.center().y),
                            vec2(tokens::ICON_SIZE as f32, tokens::ICON_SIZE as f32),
                        ),
                        row_ink(selected, tokens::SECONDARY),
                    );
                    let left = rect.left() + 44.;
                    let painter = ui.painter().with_clip_rect(Rect::from_min_max(
                        pos2(left, rect.top()),
                        pos2(rect.right() - 12., rect.bottom()),
                    ));
                    painter.text(
                        pos2(left, rect.center().y - 11.),
                        Align2::LEFT_CENTER,
                        &result.title,
                        semibold(tokens::BODY_SIZE),
                        row_ink(selected, tokens::TEXT_STRONG),
                    );
                    painter.text(
                        pos2(left, rect.center().y + 11.),
                        Align2::LEFT_CENTER,
                        format!("{} · {}", result.subtitle, result.reason),
                        font(tokens::METADATA_SIZE),
                        row_ink(selected, tokens::MUTED),
                    );
                    if response.clicked() {
                        activate = Some(i);
                    }
                }
            });
        if let Some(index) = activate {
            self.model.activate(index);
        }
        self.scroll_selected = false;
    }
}

impl eframe::App for Launcher {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let events: Vec<_> = self
            .desktop
            .as_ref()
            .map(|d| d.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Show => self.show(ctx),
                Event::Toggle => {
                    if !self.hidden && ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
                        self.dismiss(ctx)
                    } else {
                        self.show(ctx)
                    }
                }
                Event::Update => {}
            }
        }
        if self.model.poll() {
            self.dismiss(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quitting
            && self.desktop.as_ref().is_some_and(|d| d.can_hide)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dismiss(ctx);
        }
        if !self.hidden
            && self.model.response.semantic_status == "warming"
            && !self.model.pending
            && !self.model.activating
            && self.model.error.is_none()
        {
            if self.warming.elapsed() >= Duration::from_millis(500) {
                self.warming = Instant::now();
                self.model.search();
            }
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl Launcher {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Consume navigation before TextEdit sees it; text/IME/clipboard still use egui's editor.
        if !self.menu && ctx.memory(|m| m.has_focus(egui::Id::unique("query"))) {
            let down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
            let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
            if (down || up) && !ctx.input(|i| i.key_pressed(Key::Tab)) {
                // egui schedules focus movement before draw. On the first pass
                // after restoring focus the editor's arrow filter is not active
                // yet, so cancel the movement for the arrow we just consumed.
                ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            }
            if down {
                self.model.move_selection(1);
                self.scroll_selected = true;
            }
            if up {
                self.model.move_selection(-1);
                self.scroll_selected = true;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
                self.model.activate(self.model.selected);
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            if self.menu {
                self.menu = false;
                self.focus_query = true;
            } else {
                self.dismiss(&ctx);
            }
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(color(tokens::BG))
                    .inner_margin(tokens::INSET as i8),
            )
            .show(ui, |ui| {
                self.header(ui);
                ui.add_space(4.);
                ui.label(
                    RichText::new(if self.model.query.is_empty() {
                        "Apps & actions"
                    } else {
                        "Best matches"
                    })
                    .font(semibold(tokens::METADATA_SIZE))
                    .color(color(tokens::MUTED)),
                );
                let height = (ui.available_height() - tokens::FOOTER_HEIGHT as f32 - 8.).max(0.);
                self.results(ui, height);
                ui.separator();
                ui.horizontal(|ui| {
                    let status = if self.model.activating {
                        "Opening…"
                    } else if self.model.pending {
                        "Searching…"
                    } else if self.model.response.semantic_status == "ready" {
                        "Semantic search"
                    } else {
                        "Keyword mode"
                    };
                    ui.label(
                        RichText::new(status)
                            .font(font(tokens::METADATA_SIZE))
                            .color(color(tokens::MUTED)),
                    )
                    .on_hover_text(
                        self.desktop
                            .as_ref()
                            .and_then(|d| d.message.as_deref())
                            .unwrap_or("Search stays on this device"),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("↑↓ Navigate   Enter Open   Esc Close")
                                .font(font(tokens::METADATA_SIZE))
                                .color(color(tokens::MUTED)),
                        );
                    });
                });
            });
        if let Some(path) = &self.probe {
            let state = serde_json::json!({"query":self.model.query,"selected":self.model.selected,
                "results":self.model.response.results.iter().map(|r|&r.title).collect::<Vec<_>>(),
                "queryFocused":ctx.memory(|m|m.has_focus(egui::Id::unique("query"))),"menu":self.menu});
            std::fs::write(path, serde_json::to_vec(&state).unwrap()).expect("write fixture probe");
        }
        if self.screenshot.is_some() {
            if self.started.elapsed() > Duration::from_millis(500) {
                let path = self.screenshot.take().unwrap();
                let context = ctx.clone();
                let snapshot = serde_json::json!({"query":self.model.query,"selected":self.model.selected,"results":self.model.response.results.iter().map(|r|&r.title).collect::<Vec<_>>(),"error":self.model.error});
                ctx.request_screenshot(move |image| {
                    let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    image::save_buffer(
                        &path,
                        &pixels,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    )
                    .expect("write fixture screenshot");
                    std::fs::write(
                        path.with_extension("json"),
                        serde_json::to_vec_pretty(&snapshot).unwrap(),
                    )
                    .expect("write fixture state");
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                    context.request_repaint();
                });
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
    }
}

pub fn run() {
    // Windows installers invoke lifecycle hooks before the normal application.
    // Let Velopack consume those arguments, never open a launcher for a hook.
    if matches!(
        std::env::args()
            .nth(1)
            .map(|arg| arg.to_ascii_lowercase())
            .as_deref(),
        Some(
            "--veloapp-install"
                | "--veloapp-updated"
                | "--veloapp-obsolete"
                | "--veloapp-uninstall"
        )
    ) {
        findanything_core::updates::initialize();
        return;
    }
    let options = Options::parse(std::env::args().skip(1)).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let instance = if options.fixture.is_none() {
        findanything_core::updates::initialize();
        match findanything_core::instance::Instance::acquire() {
            Ok(Some(i)) => Some(i),
            Ok(None) => return,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    if instance.is_some() {
        findanything_core::updates::start();
    }
    let minimum = options.state == "minimum";
    let size = if minimum {
        [tokens::MINIMUM_WIDTH as f32, tokens::MINIMUM_HEIGHT as f32]
    } else {
        [tokens::WINDOW_WIDTH as f32, tokens::WINDOW_HEIGHT as f32]
    };
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Find Anything")
            .with_inner_size(size)
            .with_min_inner_size([tokens::MINIMUM_WIDTH as f32, tokens::MINIMUM_HEIGHT as f32]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "Find Anything",
        native,
        Box::new(move |cc| Ok(Box::new(Launcher::new(&cc.egui_ctx, options, instance)))),
    ) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_arguments_cannot_accidentally_start_live_engine() {
        let parse = |args: &[&str]| Options::parse(args.iter().map(|s| s.to_string()));
        let options = parse(&["--fixture=empty", "--screenshot", "/tmp/a=b.png"]).unwrap();
        assert!(matches!(options.fixture, Some(Fixture::Empty)));
        assert_eq!(options.screenshot.unwrap(), PathBuf::from("/tmp/a=b.png"));
        assert!(matches!(
            parse(&["--fixture", "error"]).unwrap().fixture,
            Some(Fixture::Error)
        ));
        for args in [
            vec!["--fixure=empty"],
            vec!["--fixture=unknown"],
            vec!["--screenshot"],
            vec!["--probe", "--fixture"],
            vec!["--screenshot=/tmp/image.png"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }

    fn frame(
        ctx: &egui::Context,
        app: &mut Launcher,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0., 0.), vec2(680., 440.))),
                events,
                ..Default::default()
            },
            |ui| app.draw(ui),
        );
        output.textures_delta.clear(); // Headless interaction tests have no GPU uploader.
        output
    }

    fn key(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn real_editor_input_filters_and_preserves_unicode() {
        let ctx = egui::Context::default();
        let mut app = Launcher::new(
            &ctx,
            Options {
                fixture: Some(Fixture::Results),
                ..Default::default()
            },
            None,
        );
        let _ = frame(&ctx, &mut app, vec![]);
        let _ = frame(&ctx, &mut app, vec![]); // Settle initial focus before native input.
        let _ = frame(&ctx, &mut app, vec![key(Key::ArrowDown)]);
        assert_eq!(app.model.selected, 1);
        assert!(
            ctx.memory(|m| m.has_focus(egui::Id::unique("query"))),
            "Result navigation must preserve typing focus"
        );
        let _ = frame(&ctx, &mut app, vec![egui::Event::Text("display".into())]);
        assert_eq!(app.model.query, "display");
        assert_eq!(app.model.response.results.len(), 1);
        assert_eq!(app.model.response.results[0].title, "Displays");
        let _ = frame(&ctx, &mut app, vec![egui::Event::Text("資料🚀".into())]);
        assert_eq!(app.model.query, "display資料🚀");
        assert!(app.model.response.results.is_empty());
        let _ = frame(
            &ctx,
            &mut app,
            vec![egui::Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            }],
        );
        assert!(ctx.memory(|m| m.has_focus(egui::Id::unique("query"))));
        let _ = frame(&ctx, &mut app, vec![egui::Event::Paste("資料🚀".into())]);
        assert_eq!(
            app.model.query, "資料🚀",
            "Paste must replace the entire selected query"
        );
    }

    #[test]
    fn clear_click_restores_results_focus_and_escape_closes_only_menu() {
        let ctx = egui::Context::default();
        ctx.options_mut(|options| options.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        let mut app = Launcher::new(
            &ctx,
            Options {
                fixture: Some(Fixture::Results),
                state: "query".into(),
                ..Default::default()
            },
            None,
        );
        let _ = frame(&ctx, &mut app, vec![]);
        // Right edge of the 580-wide search field: x=580, y=46.
        let pos = pos2(580., 46.);
        for pressed in [true, false] {
            let _ = frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let _ = frame(&ctx, &mut app, vec![]);
        assert_eq!(app.model.query, "");
        assert_eq!(app.model.response.results.len(), 3);
        assert!(ctx.memory(|m| m.has_focus(egui::Id::unique("query"))));
        let _ = frame(&ctx, &mut app, vec![key(Key::ArrowDown)]);
        assert_eq!(app.model.selected, 1);
        assert!(
            ctx.memory(|m| m.has_focus(egui::Id::unique("query"))),
            "Down immediately after clear must retain focus after end_pass"
        );
        app.menu = true;
        let out = frame(&ctx, &mut app, vec![key(Key::Escape)]);
        assert!(!app.menu);
        assert!(
            !out.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Minimized(true)))
        );
        let _ = frame(&ctx, &mut app, vec![key(Key::ArrowDown), key(Key::Tab)]);
        assert!(
            !ctx.memory(|m| m.has_focus(egui::Id::unique("query"))),
            "Consuming result arrows must not swallow Tab focus navigation"
        );
    }
}
