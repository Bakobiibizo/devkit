//! Menu state machine for navigation

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum MenuItem {
    Command { name: String, task: String },
    Submenu { name: String, items: Vec<MenuItem> },
    EnvVar { key: String, value: String },
    Back,
}

impl MenuItem {
    pub fn display_name(&self) -> &str {
        match self {
            MenuItem::Command { name, .. } => name,
            MenuItem::Submenu { name, .. } => name,
            MenuItem::EnvVar { key, .. } => key,
            MenuItem::Back => "← Back",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MenuState {
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub breadcrumb: Vec<String>,
    root_items: Vec<MenuItem>,
}

impl MenuState {
    pub fn new() -> Self {
        let root_items = build_root_menu();
        Self {
            items: root_items.clone(),
            selected: 0,
            breadcrumb: vec!["devkey".to_string()],
            root_items,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected < self.items.len().saturating_sub(1) {
            self.selected += 1;
        }
    }

    /// Returns Some(value) if an env var was selected, None otherwise
    pub fn select(&mut self) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }

        let item = self.items[self.selected].clone();
        match item {
            MenuItem::Submenu { name, items } => {
                self.breadcrumb.push(name);
                self.items = items;
                // Add back item at the beginning
                self.items.insert(0, MenuItem::Back);
                self.selected = 0;
                None
            }
            MenuItem::EnvVar { value, .. } => Some(value),
            MenuItem::Command { task, .. } => {
                // Copy command to clipboard so user has it as fallback
                let cmd = format!("dev run {}", task);
                let _ = crate::inject::copy_to_clipboard(&cmd);

                // Execute dev command
                let _ = std::process::Command::new("dev")
                    .args(["run", &task])
                    .spawn();
                None
            }
            MenuItem::Back => {
                self.go_back();
                None
            }
        }
    }

    pub fn go_back(&mut self) -> bool {
        if self.breadcrumb.len() > 1 {
            self.breadcrumb.pop();
            // Rebuild menu based on breadcrumb
            self.rebuild_from_breadcrumb();
            true
        } else {
            false
        }
    }

    fn rebuild_from_breadcrumb(&mut self) {
        self.items = self.root_items.clone();
        self.selected = 0;

        // Navigate through breadcrumb (skip first "devkey")
        for crumb in self.breadcrumb.iter().skip(1) {
            for item in &self.items {
                if let MenuItem::Submenu { name, items } = item {
                    if name == crumb {
                        self.items = items.clone();
                        self.items.insert(0, MenuItem::Back);
                        break;
                    }
                }
            }
        }
    }

    pub fn current_title(&self) -> String {
        self.breadcrumb.join(" > ")
    }
}

fn build_root_menu() -> Vec<MenuItem> {
    let mut items = Vec::new();

    // Env submenu
    let env_items = crate::env::load_env_vars();
    if !env_items.is_empty() {
        items.push(MenuItem::Submenu {
            name: "env".to_string(),
            items: env_items
                .into_iter()
                .map(|(key, value)| MenuItem::EnvVar { key, value })
                .collect(),
        });
    }

    // Tasks submenu - load from dev config
    let tasks = load_dev_tasks();
    if !tasks.is_empty() {
        items.push(MenuItem::Submenu {
            name: "tasks".to_string(),
            items: tasks,
        });
    }

    // Quick access commands
    items.push(MenuItem::Command {
        name: "fmt".to_string(),
        task: "fmt".to_string(),
    });
    items.push(MenuItem::Command {
        name: "lint".to_string(),
        task: "lint".to_string(),
    });
    items.push(MenuItem::Command {
        name: "test".to_string(),
        task: "test".to_string(),
    });
    items.push(MenuItem::Command {
        name: "check".to_string(),
        task: "check".to_string(),
    });

    items
}

fn load_dev_tasks() -> Vec<MenuItem> {
    // Try to load config from ~/.dev/config.toml
    let config_path = dirs::home_dir()
        .map(|h| h.join(".dev").join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("~/.dev/config.toml"));

    if !config_path.exists() {
        return Vec::new();
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let config: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut tasks = Vec::new();

    if let Some(tasks_table) = config.get("tasks").and_then(|t| t.as_table()) {
        for (name, _) in tasks_table {
            tasks.push(MenuItem::Command {
                name: name.clone(),
                task: name.clone(),
            });
        }
    }

    tasks.sort_by(|a, b| a.display_name().cmp(b.display_name()));
    tasks
}
