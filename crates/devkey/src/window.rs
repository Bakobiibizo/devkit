//! Custom borderless GUI window using iced

use crate::menu::{MenuItem, MenuState};
use iced::keyboard::{self, Key};
use iced::widget::{column, container, scrollable, text, Column};
use iced::{
    event, window, Color, Element, Event, Length, Padding, Size, Subscription, Task, Theme,
};

pub fn run_window() -> iced::Result {
    iced::application("devkey", DevKey::update, DevKey::view)
        .subscription(DevKey::subscription)
        .theme(|_| Theme::Dark)
        .window(window::Settings {
            size: Size::new(300.0, 400.0),
            position: window::Position::Centered,
            decorations: false,
            transparent: true,
            level: window::Level::AlwaysOnTop,
            ..Default::default()
        })
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    KeyPressed(Key),
    WindowFocusLost,
    WindowOpened(window::Id),
}

struct DevKey {
    menu: MenuState,
    should_close: bool,
}

impl Default for DevKey {
    fn default() -> Self {
        Self {
            menu: MenuState::new(),
            should_close: false,
        }
    }
}

impl DevKey {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::KeyPressed(key) => {
                match key.as_ref() {
                    Key::Named(keyboard::key::Named::Escape) => {
                        if !self.menu.go_back() {
                            self.should_close = true;
                            return window::get_oldest().and_then(window::close);
                        }
                    }
                    Key::Named(keyboard::key::Named::ArrowUp) => {
                        self.menu.move_up();
                    }
                    Key::Named(keyboard::key::Named::ArrowDown) => {
                        self.menu.move_down();
                    }
                    Key::Named(keyboard::key::Named::Enter) => {
                        if let Some(value) = self.menu.select() {
                            // Inject the value and close
                            let _ = crate::inject::inject_text(&value);
                            self.should_close = true;
                            return window::get_oldest().and_then(window::close);
                        }
                    }
                    Key::Named(keyboard::key::Named::Backspace) => {
                        self.menu.go_back();
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::WindowFocusLost => {
                self.should_close = true;
                window::get_oldest().and_then(window::close)
            }
            Message::WindowOpened(id) => {
                // Focus the window when it opens
                window::gain_focus(id)
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // Title bar
        let title = text(self.menu.current_title())
            .size(14)
            .color(Color::from_rgb(0.7, 0.7, 0.7));

        let title_bar = container(title)
            .width(Length::Fill)
            .padding(Padding::from([8, 12]));

        // Menu items
        let mut items_column = Column::new().spacing(2).padding(Padding::from([4, 8]));

        for (idx, item) in self.menu.items.iter().enumerate() {
            let is_selected = idx == self.menu.selected;

            let item_text = match item {
                MenuItem::Submenu { name, .. } => format!("  {} →", name),
                MenuItem::EnvVar { key, .. } => format!("  {}", key),
                MenuItem::Command { name, .. } => format!("  {}", name),
                MenuItem::Back => "  ← Back".to_string(),
            };

            let label = text(item_text).size(13);

            let item_container = if is_selected {
                container(label)
                    .width(Length::Fill)
                    .padding(Padding::from([6, 8]))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(
                            0.2, 0.4, 0.6,
                        ))),
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

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, _status, id| match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                Some(Message::KeyPressed(key))
            }
            Event::Window(window::Event::Unfocused) => Some(Message::WindowFocusLost),
            Event::Window(window::Event::Opened { .. }) => Some(Message::WindowOpened(id)),
            _ => None,
        })
    }
}
