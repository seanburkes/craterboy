use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, PhysicalSize, SharedString};

slint::slint! {
    import { Button, TextEdit, ScrollView } from "std-widgets.slint";

    export component MenuWindow inherits Window {
        in-out property <string> rom_path;
        in property <string> status;
        in property <bool> has_rom;
        callback load_rom();
        callback resume();
        callback quit();
        callback browse_files();
        background: transparent;

        Rectangle {
            width: 100%;
            height: 100%;
            background: #000000aa;
        }

        Rectangle {
            width: min(parent.width * 0.8, 400px);
            height: min(parent.height * 0.8, 320px);
            x: (parent.width - self.width) / 2;
            y: (parent.height - self.height) / 2;
            background: #141a22;
            border-color: #ffffff22;
            border-width: 1px;
            border-radius: 8px;

            VerticalLayout {
                padding: 16px;
                spacing: 12px;
                alignment: start;

                Text {
                    text: "Paused";
                    font-size: 18px;
                    color: #f0f2f6;
                    font-weight: 700;
                }

                HorizontalLayout {
                    spacing: 8px;
                    alignment: start;

                    Text {
                        text: "ROM:";
                        color: #9aa0a6;
                        font-size: 12px;
                        vertical-alignment: center;
                        width: 40px;
                    }

                    Rectangle {
                        background: #0d1117;
                        border-radius: 4px;
                        border-color: #30363d;
                        border-width: 1px;
                        height: 28px;

                        Text {
                            text: root.rom_path;
                            color: root.rom_path != "" ? #f0f2f6 : #6e7681;
                            font-size: 11px;
                            overflow: elide;
                            x: 8px;
                            y: parent.height / 2 - self.height / 2;
                            width: parent.width - 80px;
                        }
                    }

                    Button {
                        text: "Browse...";
                        clicked => { root.browse_files(); }
                        width: 80px;
                        height: 28px;
                    }
                }

                HorizontalLayout {
                    spacing: 8px;
                    alignment: end;

                    Button {
                        text: "Load ROM";
                        enabled: root.rom_path != "";
                        clicked => { root.load_rom(); }
                    }
                    Button {
                        text: "Resume";
                        enabled: root.has_rom;
                        clicked => { root.resume(); }
                    }
                    Button {
                        text: "Quit";
                        clicked => { root.quit(); }
                    }
                }

                Rectangle {
                    height: 1px;
                    background: #30363d;
                }

                Text {
                    text: "Tip: Supports .gb and .gbc ROMs";
                    color: #6e7681;
                    font-size: 10px;
                }

                Text {
                    text: "Esc: menu";
                    color: #6e7681;
                    font-size: 9px;
                }

                Text {
                    text: root.status;
                    visible: root.status != "";
                    color: #ff8f8f;
                    font-size: 11px;
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum MenuAction {
    LoadRom(String),
    Resume,
    Quit,
    ShowFilePicker,
}

struct MenuPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for MenuPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

fn menu_window() -> Rc<MinimalSoftwareWindow> {
    static PLATFORM_SET: OnceLock<()> = OnceLock::new();
    thread_local! {
        static WINDOW: Rc<MinimalSoftwareWindow> =
            MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    }

    WINDOW.with(|window| {
        PLATFORM_SET.get_or_init(|| {
            slint::platform::set_platform(Box::new(MenuPlatform {
                window: window.clone(),
            }))
            .expect("set slint platform");
        });
        window.clone()
    })
}

pub struct MenuOverlay {
    window: Rc<MinimalSoftwareWindow>,
    ui: MenuWindow,
    buffer: Vec<PremultipliedRgbaColor>,
    rgba: Vec<u8>,
    width: usize,
    height: usize,
    actions: Rc<RefCell<Vec<MenuAction>>>,
}

impl MenuOverlay {
    pub fn new(width: usize, height: usize) -> Self {
        let window = menu_window();
        let ui = MenuWindow::new().expect("menu window");
        ui.show().expect("show menu window");
        window.set_size(PhysicalSize::new(width as u32, height as u32));
        window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });

        let actions = Rc::new(RefCell::new(Vec::new()));
        let actions_load = actions.clone();
        let ui_load = ui.as_weak();
        ui.on_load_rom(move || {
            if let Some(ui) = ui_load.upgrade() {
                actions_load
                    .borrow_mut()
                    .push(MenuAction::LoadRom(ui.get_rom_path().to_string()));
            }
        });

        let actions_resume = actions.clone();
        ui.on_resume(move || {
            actions_resume.borrow_mut().push(MenuAction::Resume);
        });

        let actions_browse = actions.clone();
        ui.on_browse_files(move || {
            actions_browse.borrow_mut().push(MenuAction::ShowFilePicker);
        });

        let actions_quit = actions.clone();
        ui.on_quit(move || {
            actions_quit.borrow_mut().push(MenuAction::Quit);
        });

        let buffer = vec![PremultipliedRgbaColor::default(); width * height];
        let rgba = vec![0u8; width * height * 4];

        Self {
            window,
            ui,
            buffer,
            rgba,
            width,
            height,
            actions,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width.max(1);
        self.height = height.max(1);
        self.buffer
            .resize(self.width * self.height, PremultipliedRgbaColor::default());
        self.rgba.resize(self.width * self.height * 4, 0);
        self.window
            .set_size(PhysicalSize::new(self.width as u32, self.height as u32));
        self.ui.window().request_redraw();
    }

    pub fn update_timers(&self) {
        slint::platform::update_timers_and_animations();
    }

    pub fn dispatch_event(&self, event: WindowEvent) {
        self.window.dispatch_event(event);
    }

    pub fn request_redraw(&self) {
        self.ui.window().request_redraw();
    }

    pub fn set_has_rom(&self, has_rom: bool) {
        self.ui.set_has_rom(has_rom);
    }

    pub fn set_rom_path(&self, path: impl Into<SharedString>) {
        self.ui.set_rom_path(path.into());
    }

    pub fn set_selected_path(&self, path: &std::path::Path) {
        self.ui
            .set_rom_path(path.to_string_lossy().to_string().into());
    }

    pub fn set_status(&self, status: impl Into<SharedString>) {
        self.ui.set_status(status.into());
    }

    pub fn take_actions(&self) -> Vec<MenuAction> {
        std::mem::take(&mut *self.actions.borrow_mut())
    }

    pub fn render_rgba(&mut self) -> (&[u8], usize, usize) {
        let redraw = self.window.draw_if_needed(|renderer| {
            self.buffer.fill(PremultipliedRgbaColor::default());
            renderer.render(&mut self.buffer, self.width);
        });
        if redraw {
            let expected = self.width * self.height * 4;
            if self.rgba.len() != expected {
                self.rgba.resize(expected, 0);
            }
            for (i, src_px) in self.buffer.iter().enumerate() {
                let idx = i * 4;
                self.rgba[idx] = src_px.red;
                self.rgba[idx + 1] = src_px.green;
                self.rgba[idx + 2] = src_px.blue;
                self.rgba[idx + 3] = src_px.alpha;
            }
        }
        (&self.rgba, self.width, self.height)
    }
}
