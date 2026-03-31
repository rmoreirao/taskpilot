use eframe::egui;
use image::ImageReader;
use std::io::Cursor;
use std::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Open,
    Quit,
}

pub struct TrayManager {
    _tray_icon: TrayIcon,
    event_rx: mpsc::Receiver<TrayEvent>,
}

impl TrayManager {
    pub fn new(ctx: egui::Context) -> Result<Self, Box<dyn std::error::Error>> {
        let icon = Self::load_icon()?;

        let quit_item = MenuItem::new("Quit", true, None);
        let quit_id = quit_item.id().clone();

        let menu = Menu::new();
        menu.append(&quit_item)?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("TaskPilot")
            .with_icon(icon)
            .build()?;

        let (tx, rx) = mpsc::channel();

        // Listen for tray menu events (Quit) on a background thread
        let tx_menu = tx.clone();
        let ctx_menu = ctx.clone();
        std::thread::spawn(move || {
            loop {
                match MenuEvent::receiver().recv() {
                    Ok(event) => {
                        if event.id == quit_id {
                            if tx_menu.send(TrayEvent::Quit).is_err() {
                                break;
                            }
                            ctx_menu.request_repaint();
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Listen for tray icon clicks (Open) on a background thread
        std::thread::spawn(move || {
            loop {
                match TrayIconEvent::receiver().recv() {
                    Ok(TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }) => {
                        if tx.send(TrayEvent::Open).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            _tray_icon: tray_icon,
            event_rx: rx,
        })
    }

    pub fn check_event(&self) -> Option<TrayEvent> {
        self.event_rx.try_recv().ok()
    }

    fn load_icon() -> Result<Icon, Box<dyn std::error::Error>> {
        static ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
        let img = ImageReader::new(Cursor::new(ICON_PNG))
            .with_guessed_format()?
            .decode()?
            .into_rgba8();
        let (w, h) = img.dimensions();
        Ok(Icon::from_rgba(img.into_raw(), w, h)?)
    }
}
