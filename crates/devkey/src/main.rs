#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod env;
mod focus;
mod hotkey;
mod inject;
mod menu;
mod secrets;
mod window;

use anyhow::Result;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, MenuId},
    TrayIconBuilder,
};

fn main() -> Result<()> {
    // Start hotkey listener in background thread
    std::thread::spawn(move || {
        if let Err(e) = hotkey::run_hotkey_listener() {
            eprintln!("Hotkey listener error: {}", e);
        }
    });

    // Build tray menu
    let tray_menu = Menu::new();
    let show_item = MenuItem::new("Show", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    tray_menu.append(&show_item)?;
    tray_menu.append(&quit_item)?;

    // Create tray icon
    let icon = load_icon();
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_menu_on_left_click(false)
        .with_tooltip("devkey - Press Ctrl+; to open")
        .with_icon(icon)
        .build()?;

    // Handle tray menu events in a thread
    std::thread::spawn(move || {
        handle_menu_events(show_id, quit_id);
    });

    // Run the iced application (this runs forever until quit)
    if let Err(e) = window::run_app() {
        eprintln!("Window error: {}", e);
    }

    Ok(())
}

fn handle_menu_events(show_id: MenuId, quit_id: MenuId) {
    let menu_channel = MenuEvent::receiver();
    loop {
        if let Ok(event) = menu_channel.recv() {
            if event.id == show_id {
                window::request_show();
            } else if event.id == quit_id {
                std::process::exit(0);
            }
        }
    }
}

fn load_icon() -> tray_icon::Icon {
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx] = 40;
            rgba[idx + 1] = 40;
            rgba[idx + 2] = 40;
            rgba[idx + 3] = 255;

            let in_d_vertical = x >= 3 && x <= 5 && y >= 3 && y <= 12;
            let in_d_top = y >= 3 && y <= 5 && x >= 3 && x <= 10;
            let in_d_bottom = y >= 10 && y <= 12 && x >= 3 && x <= 10;
            let in_d_right = x >= 10 && x <= 12 && y >= 5 && y <= 10;

            if in_d_vertical || in_d_top || in_d_bottom || in_d_right {
                rgba[idx] = 100;
                rgba[idx + 1] = 200;
                rgba[idx + 2] = 255;
            }

            let in_border = x == 0 || x == size - 1 || y == 0 || y == size - 1;
            if in_border {
                rgba[idx] = 60;
                rgba[idx + 1] = 60;
                rgba[idx + 2] = 60;
            }
        }
    }

    tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create icon")
}
