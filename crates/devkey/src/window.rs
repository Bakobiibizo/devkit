//! Custom borderless GUI window using iced
//!
//! Uses a daemon-style architecture where the iced app runs continuously
//! and windows are opened/closed via messages from the hotkey thread.

use crate::menu::{LoadedSecrets, MenuItem, MenuState};
use iced::keyboard::{self, Key};
use iced::widget::{Column, Id, column, container, scrollable, text};
use iced::{
    Color, Element, Event, Length, Padding, Size, Subscription, Task, Theme, event, window,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

const ITEM_HEIGHT: f32 = 32.0; // Approximate height of each menu item

/// Flag for hotkey thread to request window show
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Request the window to show (called from hotkey thread)
pub fn request_show() {
    SHOW_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn run_app() -> iced::Result {
    iced::daemon(DevKey::new, DevKey::update, DevKey::view)
        .subscription(DevKey::subscription)
        .theme(DevKey::theme)
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    KeyPressed(window::Id, Key, Option<String>),
    WindowFocusLost(window::Id),
    WindowOpened(window::Id),
    SecretsLoaded(LoadedSecrets),
}

struct DevKey {
    /// Menu state per window
    windows: HashMap<window::Id, MenuState>,
}

impl DevKey {
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                windows: HashMap::new(),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                // Check if show was requested
                if SHOW_REQUESTED.swap(false, Ordering::SeqCst) {
                    // Only open if no window is currently open
                    if self.windows.is_empty() {
                        // Save foreground window before showing our popup
                        crate::focus::save_foreground_window();

                        // Open a new popup window immediately (no blocking)
                        let (id, open_task) = window::open(window::Settings {
                            size: Size::new(300.0, 400.0),
                            position: window::Position::Centered,
                            decorations: false,
                            transparent: true,
                            level: window::Level::AlwaysOnTop,
                            ..Default::default()
                        });

                        self.windows.insert(id, MenuState::new());

                        // Fetch secrets on a background thread so the window
                        // appears instantly and secrets hydrate when ready.
                        let secrets_task = Task::perform(
                            async {
                                tokio::task::spawn_blocking(crate::menu::fetch_secrets_blocking)
                                    .await
                                    .unwrap_or_else(|_| LoadedSecrets {
                                        production: Vec::new(),
                                        development: Vec::new(),
                                    })
                            },
                            Message::SecretsLoaded,
                        );

                        return Task::batch([
                            open_task.map(move |_| Message::WindowOpened(id)),
                            secrets_task,
                        ]);
                    }
                }
                Task::none()
            }
            Message::KeyPressed(id, key, text) => {
                let Some(menu) = self.windows.get_mut(&id) else {
                    return Task::none();
                };

                // Handle input mode differently
                if menu.is_input_mode() {
                    match key.as_ref() {
                        Key::Named(keyboard::key::Named::Escape) => {
                            menu.cancel_input();
                        }
                        Key::Named(keyboard::key::Named::Enter) => {
                            if menu.input_submit() {
                                return self.close_window(id);
                            }
                        }
                        Key::Named(keyboard::key::Named::Tab) => {
                            menu.input_next_field();
                        }
                        Key::Named(keyboard::key::Named::Backspace) => {
                            menu.input_backspace();
                        }
                        Key::Character(c) if c.to_lowercase().to_string() == "v" => {
                            // Check for Ctrl+V paste
                            if let Some(t) = &text {
                                if t == "\u{16}" {
                                    // Ctrl+V produces this control character
                                    if let Ok(clipboard) = crate::inject::read_clipboard() {
                                        menu.input_paste(&clipboard);
                                    }
                                    return Task::none();
                                }
                            }
                            // Otherwise treat as normal character
                            if let Some(t) = &text {
                                for ch in t.chars() {
                                    if !ch.is_control() {
                                        menu.input_char(ch);
                                    }
                                }
                            }
                        }
                        Key::Character(c) if c.to_lowercase().to_string() == "r" => {
                            // Check for Ctrl+R reveal toggle
                            if let Some(t) = &text {
                                if t == "\u{12}" {
                                    menu.toggle_reveal();
                                    return Task::none();
                                }
                            }
                            // Otherwise treat as normal character
                            if let Some(t) = &text {
                                for ch in t.chars() {
                                    if !ch.is_control() {
                                        menu.input_char(ch);
                                    }
                                }
                            }
                        }
                        _ => {
                            // Use the text field for character input (handles shift properly)
                            if let Some(t) = &text {
                                for ch in t.chars() {
                                    if !ch.is_control() {
                                        menu.input_char(ch);
                                    }
                                }
                            }
                        }
                    }
                    return Task::none();
                }

                // Normal menu navigation
                match key.as_ref() {
                    Key::Named(keyboard::key::Named::Escape) => {
                        if !menu.go_back() {
                            return self.close_window(id);
                        }
                    }
                    Key::Named(keyboard::key::Named::ArrowUp) => {
                        menu.move_up();
                        return self.scroll_to_selected(id);
                    }
                    Key::Named(keyboard::key::Named::ArrowDown) => {
                        menu.move_down();
                        return self.scroll_to_selected(id);
                    }
                    Key::Named(keyboard::key::Named::Enter) => {
                        if let Some(value) = menu.select() {
                            // Inject the value and close window
                            let _ = crate::inject::inject_text(&value);
                            return self.close_window(id);
                        }
                    }
                    Key::Named(keyboard::key::Named::Backspace) => {
                        menu.go_back();
                    }
                    Key::Character(c) if c.to_lowercase().to_string() == "e" => {
                        // Edit selected secret
                        if let Some(MenuItem::Secret {
                            name,
                            account,
                            item_id,
                        }) = menu.items.get(menu.selected)
                        {
                            let (name, account, item_id) =
                                (name.clone(), account.clone(), item_id.clone());
                            menu.start_edit(&account, &item_id, &name);
                        }
                    }
                    Key::Character(c) if c.to_lowercase().to_string() == "d" => {
                        // Delete selected secret
                        if let Some(MenuItem::Secret {
                            name,
                            account,
                            item_id,
                        }) = menu.items.get(menu.selected)
                        {
                            let (name, account, item_id) =
                                (name.clone(), account.clone(), item_id.clone());
                            menu.start_delete(&account, &item_id, &name);
                        }
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::SecretsLoaded(loaded) => {
                // Hydrate secrets into all open windows
                for menu in self.windows.values_mut() {
                    menu.hydrate_secrets(loaded.clone());
                }
                Task::none()
            }
            Message::WindowFocusLost(_id) => {
                // Don't close on focus lost - user might click elsewhere temporarily
                Task::none()
            }
            Message::WindowOpened(id) => window::gain_focus(id),
        }
    }

    fn close_window(&mut self, id: window::Id) -> Task<Message> {
        self.windows.remove(&id);
        window::close(id)
    }

    fn scroll_to_selected(&self, id: window::Id) -> Task<Message> {
        let Some(menu) = self.windows.get(&id) else {
            return Task::none();
        };

        let _offset = menu.selected as f32 * ITEM_HEIGHT;
        Task::none()
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        use crate::menu::{InputField, InputMode};

        let Some(menu) = self.windows.get(&id) else {
            return container(text("")).into();
        };

        // Check if in input mode
        match &menu.input_mode {
            InputMode::AddSecret { account, field }
            | InputMode::EditSecret { account, field, .. } => {
                let is_edit = matches!(&menu.input_mode, InputMode::EditSecret { .. });
                let title_text = if is_edit {
                    format!("Edit Secret in {}", account.to_uppercase())
                } else {
                    format!("Add Secret to {}", account.to_uppercase())
                };
                let title = text(title_text)
                    .size(14)
                    .color(Color::from_rgb(0.7, 0.7, 0.7));

                let title_bar = container(title)
                    .width(Length::Fill)
                    .padding(Padding::from([8, 12]));

                let name_label = text("Name:").size(12).color(Color::from_rgb(0.6, 0.6, 0.6));
                let name_display = if is_edit {
                    menu.input_name.clone()
                } else {
                    format!("{}_", &menu.input_name)
                };
                let name_value = text(name_display)
                    .size(13)
                    .color(if *field == InputField::Name {
                        Color::WHITE
                    } else {
                        Color::from_rgb(0.7, 0.7, 0.7)
                    });
                let name_row = container(column![name_label, name_value])
                    .width(Length::Fill)
                    .padding(Padding::from([6, 8]))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(if *field == InputField::Name {
                            Color::from_rgb(0.2, 0.4, 0.6)
                        } else {
                            Color::from_rgb(0.15, 0.15, 0.15)
                        })),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    });

                let value_label_text = if menu.reveal_value {
                    "Value:"
                } else {
                    "Value: (Ctrl+R to reveal)"
                };
                let value_label = text(value_label_text)
                    .size(12)
                    .color(Color::from_rgb(0.6, 0.6, 0.6));
                let value_display = if menu.input_value.is_empty() {
                    "_".to_string()
                } else if menu.reveal_value {
                    format!("{}_", &menu.input_value)
                } else {
                    format!("{}*", "*".repeat(menu.input_value.len().min(20)))
                };
                let value_value =
                    text(value_display)
                        .size(13)
                        .color(if *field == InputField::Value {
                            Color::WHITE
                        } else {
                            Color::from_rgb(0.7, 0.7, 0.7)
                        });
                let value_row = container(column![value_label, value_value])
                    .width(Length::Fill)
                    .padding(Padding::from([6, 8]))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(if *field == InputField::Value {
                            Color::from_rgb(0.2, 0.4, 0.6)
                        } else {
                            Color::from_rgb(0.15, 0.15, 0.15)
                        })),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    });

                let hint = text("Tab: next | Enter: save | Ctrl+V: paste | Esc: cancel")
                    .size(11)
                    .color(Color::from_rgb(0.5, 0.5, 0.5));

                let form = column![name_row, value_row, hint]
                    .spacing(8)
                    .padding(Padding::from([8, 8]));

                let content = column![title_bar, form];

                return container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(
                            0.12, 0.12, 0.12,
                        ))),
                        border: iced::Border {
                            radius: 8.0.into(),
                            width: 1.0,
                            color: Color::from_rgb(0.3, 0.3, 0.3),
                        },
                        ..Default::default()
                    })
                    .into();
            }
            InputMode::ConfirmDelete { name, account, .. } => {
                let title = text("Confirm Delete")
                    .size(14)
                    .color(Color::from_rgb(0.9, 0.4, 0.4));

                let title_bar = container(title)
                    .width(Length::Fill)
                    .padding(Padding::from([8, 12]));

                let msg = text(format!(
                    "Delete '{}' from {}?",
                    name,
                    account.to_uppercase()
                ))
                .size(13)
                .color(Color::WHITE);

                let msg_row = container(msg)
                    .width(Length::Fill)
                    .padding(Padding::from([12, 8]));

                let hint = text("Enter: confirm delete | Esc: cancel")
                    .size(11)
                    .color(Color::from_rgb(0.5, 0.5, 0.5));

                let form = column![msg_row, hint]
                    .spacing(8)
                    .padding(Padding::from([8, 8]));

                let content = column![title_bar, form];

                return container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(
                            0.12, 0.12, 0.12,
                        ))),
                        border: iced::Border {
                            radius: 8.0.into(),
                            width: 1.0,
                            color: Color::from_rgb(0.5, 0.3, 0.3),
                        },
                        ..Default::default()
                    })
                    .into();
            }
            InputMode::None => {}
        }

        // Title bar
        let title = text(menu.current_title())
            .size(14)
            .color(Color::from_rgb(0.7, 0.7, 0.7));

        let title_bar = container(title)
            .width(Length::Fill)
            .padding(Padding::from([8, 12]));

        // Menu items
        let mut items_column = Column::new().spacing(2).padding(Padding::from([4, 8]));

        for (idx, item) in menu.items.iter().enumerate() {
            let is_selected = idx == menu.selected;

            let item_text = match item {
                MenuItem::Submenu { name, .. } => format!("  {} →", name),
                MenuItem::EnvVar { key, .. } => format!("  {}", key),
                MenuItem::Command { name, .. } => format!("  {}", name),
                MenuItem::Secret { name, account, .. } => {
                    let tag = if account == "production" {
                        "[PROD]"
                    } else {
                        "[DEV]"
                    };
                    format!("  {} {}", tag, name)
                }
                MenuItem::Back => "  ← Back".to_string(),
            };

            let label = text(item_text).size(13);

            let item_container = if is_selected {
                container(label)
                    .width(Length::Fill)
                    .padding(Padding::from([6, 8]))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.4, 0.6))),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
            } else {
                container(label)
                    .width(Length::Fill)
                    .padding(Padding::from([6, 8]))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(
                            0.15, 0.15, 0.15,
                        ))),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
            };

            items_column = items_column.push(item_container);
        }

        let scrollable_items = scrollable(items_column)
            .id(Id::new("menu_scroll"))
            .width(Length::Fill)
            .height(Length::Fill);

        // Main container with dark background and rounded corners
        let content = column![title_bar, scrollable_items];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.12, 0.12, 0.12))),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.25, 0.25, 0.25),
                },
                ..Default::default()
            })
            .into()
    }

    fn theme(&self, _id: window::Id) -> Theme {
        Theme::Dark
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            // Poll for show requests
            iced::time::every(std::time::Duration::from_millis(50)).map(|_| Message::Tick),
            // Window events
            event::listen_with(|event, _status, id| match event {
                Event::Keyboard(keyboard::Event::KeyPressed { key, text, .. }) => {
                    Some(Message::KeyPressed(id, key, text.map(|s| s.to_string())))
                }
                Event::Window(window::Event::Unfocused) => Some(Message::WindowFocusLost(id)),
                Event::Window(window::Event::Opened { .. }) => Some(Message::WindowOpened(id)),
                _ => None,
            }),
        ])
    }
}

impl Default for DevKey {
    fn default() -> Self {
        Self::new().0
    }
}
