use crate::workspace::append_debug_log;
use eframe::egui;
use image::ImageReader;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

/// Use Win32 API to make the window visible and bring it to the foreground.
///
/// This bypasses the eframe viewport command queue, which cannot process
/// `Visible(true)` when the window is hidden because `WM_PAINT` (and therefore
/// `RedrawRequested`) is never delivered for hidden windows on Windows.
#[cfg(windows)]
fn restore_window_native(log_path: &PathBuf) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_SHOW,
    };

    let title: Vec<u16> = "TaskPilot\0".encode_utf16().collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if !hwnd.is_null() {
        let _ = append_debug_log(log_path, "tray", "Restoring window via Win32 ShowWindow");
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    } else {
        let _ = append_debug_log(
            log_path,
            "tray",
            "Win32 FindWindowW failed to locate TaskPilot window",
        );
    }
}

#[cfg(not(windows))]
fn restore_window_native(_log_path: &PathBuf) {
    // No-op on non-Windows platforms; viewport commands handle restore.
}

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
    pub fn new(ctx: egui::Context, log_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let icon = Self::load_icon()?;
        let _ = append_debug_log(&log_path, "tray", "Initializing tray manager");

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
        let menu_log_path = log_path.clone();
        std::thread::spawn(move || {
            let _ = append_debug_log(&menu_log_path, "tray", "Tray menu listener started");
            loop {
                match MenuEvent::receiver().recv() {
                    Ok(event) => {
                        let _ = append_debug_log(
                            &menu_log_path,
                            "tray",
                            &format!("Tray menu event received: {:?}", event.id),
                        );
                        if event.id == quit_id {
                            let _ = append_debug_log(
                                &menu_log_path,
                                "tray",
                                "Quit menu clicked; terminating process",
                            );
                            // Force-exit immediately. The UI event loop may not
                            // be running when the window is hidden, so we cannot
                            // rely on it to process a graceful quit.
                            std::process::exit(0);
                        }
                    }
                    Err(err) => {
                        let _ = append_debug_log(
                            &menu_log_path,
                            "tray",
                            &format!("Tray menu listener stopped: {}", err),
                        );
                        break;
                    }
                }
            }
        });

        // Listen for tray icon clicks (Open) on a background thread
        let tray_log_path = log_path.clone();
        std::thread::spawn(move || {
            let _ = append_debug_log(&tray_log_path, "tray", "Tray icon listener started");
            loop {
                match TrayIconEvent::receiver().recv() {
                    Ok(event) => {
                        let _ = append_debug_log(
                            &tray_log_path,
                            "tray",
                            &format!("Tray icon event received: {:?}", event),
                        );

                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            if tx.send(TrayEvent::Open).is_err() {
                                let _ = append_debug_log(
                                    &tray_log_path,
                                    "tray",
                                    "Failed to queue open event; stopping tray listener",
                                );
                                break;
                            }
                            let _ = append_debug_log(
                                &tray_log_path,
                                "tray",
                                "Queued open event and restoring viewport",
                            );
                            // Show the window via Win32 API first — viewport
                            // commands alone cannot restore a hidden window
                            // because the eframe event loop does not process
                            // them while the window is invisible (no WM_PAINT).
                            restore_window_native(&tray_log_path);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            ctx.request_repaint();
                        }
                    }
                    Err(err) => {
                        let _ = append_debug_log(
                            &tray_log_path,
                            "tray",
                            &format!("Tray icon listener stopped: {}", err),
                        );
                        break;
                    }
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
