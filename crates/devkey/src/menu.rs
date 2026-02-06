//! Menu state machine for navigation

use crate::secrets::SecretItem;
use std::path::PathBuf;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Secrets loaded from a background thread, ready to hydrate into the menu.
#[derive(Debug, Clone)]
pub struct LoadedSecrets {
    pub production: Vec<SecretItem>,
    pub development: Vec<SecretItem>,
}

#[derive(Debug, Clone)]
pub enum MenuItem {
    Command { name: String, task: String },
    Submenu { name: String, items: Vec<MenuItem> },
    EnvVar { key: String, value: String },
    Secret { name: String, account: String, item_id: String },
    Back,
}

impl MenuItem {
    pub fn display_name(&self) -> &str {
        match self {
            MenuItem::Command { name, .. } => name,
            MenuItem::Submenu { name, .. } => name,
            MenuItem::EnvVar { key, .. } => key,
            MenuItem::Secret { name, .. } => name,
            MenuItem::Back => "← Back",
        }
    }
}

/// Input mode for adding/editing secrets
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    None,
    AddSecret { account: String, field: InputField },
    EditSecret { account: String, item_id: String, field: InputField },
    ConfirmDelete { account: String, item_id: String, name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputField {
    Name,
    Value,
}

#[derive(Debug, Clone)]
pub struct MenuState {
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub breadcrumb: Vec<String>,
    root_items: Vec<MenuItem>,
    pub input_mode: InputMode,
    pub input_name: String,
    pub input_value: String,
    pub reveal_value: bool,
}

impl MenuState {
    pub fn new() -> Self {
        let root_items = build_root_menu();
        Self {
            items: root_items.clone(),
            selected: 0,
            breadcrumb: vec!["devkey".to_string()],
            root_items,
            input_mode: InputMode::None,
            input_name: String::new(),
            input_value: String::new(),
            reveal_value: false,
        }
    }

    /// Replace the "Loading secrets..." placeholder with actual secret items.
    /// Updates both root_items and the current view if the user is still
    /// looking at the top-level or secrets submenu.
    pub fn hydrate_secrets(&mut self, loaded: LoadedSecrets) {
        // Build the full secrets menu items
        let mut secrets_items: Vec<MenuItem> = vec![
            MenuItem::Command {
                name: "+ Add to PROD".to_string(),
                task: "vault-add-prod".to_string(),
            },
            MenuItem::Command {
                name: "+ Add to DEV".to_string(),
                task: "vault-add-dev".to_string(),
            },
        ];

        for s in loaded.production {
            secrets_items.push(MenuItem::Secret {
                name: s.title,
                account: "production".to_string(),
                item_id: s.id,
            });
        }
        for s in loaded.development {
            secrets_items.push(MenuItem::Secret {
                name: s.title,
                account: "development".to_string(),
                item_id: s.id,
            });
        }

        // Update the secrets submenu in root_items
        for item in &mut self.root_items {
            if let MenuItem::Submenu { name, items } = item {
                if name == "secrets" {
                    *items = secrets_items.clone();
                    break;
                }
            }
        }

        // If the user is currently viewing the secrets submenu, update live
        if self.breadcrumb.last().map(|s| s.as_str()) == Some("secrets") {
            // Preserve the Back item at position 0, replace the rest
            let mut new_items = vec![MenuItem::Back];
            new_items.extend(secrets_items);
            self.items = new_items;
            // Clamp selection to valid range
            if self.selected >= self.items.len() {
                self.selected = self.items.len().saturating_sub(1);
            }
        }

        // If still at root level, rebuild from root_items so the submenu ref is current
        if self.breadcrumb.len() == 1 {
            self.items = self.root_items.clone();
            if self.selected >= self.items.len() {
                self.selected = self.items.len().saturating_sub(1);
            }
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.input_mode != InputMode::None
    }

    pub fn input_char(&mut self, c: char) {
        match &self.input_mode {
            InputMode::AddSecret { field, .. } | InputMode::EditSecret { field, .. } => {
                match field {
                    InputField::Name => self.input_name.push(c),
                    InputField::Value => self.input_value.push(c),
                }
            }
            _ => {}
        }
    }

    pub fn input_paste(&mut self, text: &str) {
        match &self.input_mode {
            InputMode::AddSecret { field, .. } | InputMode::EditSecret { field, .. } => {
                match field {
                    InputField::Name => self.input_name.push_str(text),
                    InputField::Value => self.input_value.push_str(text),
                }
            }
            _ => {}
        }
    }

    pub fn input_backspace(&mut self) {
        match &self.input_mode {
            InputMode::AddSecret { field, .. } | InputMode::EditSecret { field, .. } => {
                match field {
                    InputField::Name => { self.input_name.pop(); }
                    InputField::Value => { self.input_value.pop(); }
                }
            }
            _ => {}
        }
    }

    pub fn toggle_reveal(&mut self) {
        self.reveal_value = !self.reveal_value;
    }

    pub fn input_next_field(&mut self) {
        match &self.input_mode {
            InputMode::AddSecret { account, field } => {
                if *field == InputField::Name && !self.input_name.is_empty() {
                    self.input_mode = InputMode::AddSecret {
                        account: account.clone(),
                        field: InputField::Value,
                    };
                }
            }
            InputMode::EditSecret { account, item_id, field } => {
                if *field == InputField::Name && !self.input_name.is_empty() {
                    self.input_mode = InputMode::EditSecret {
                        account: account.clone(),
                        item_id: item_id.clone(),
                        field: InputField::Value,
                    };
                }
            }
            _ => {}
        }
    }

    pub fn input_submit(&mut self) -> bool {
        match &self.input_mode {
            InputMode::AddSecret { account, field } => {
                if *field == InputField::Value && !self.input_name.is_empty() && !self.input_value.is_empty() {
                    let mut cmd = std::process::Command::new("dev");
                    cmd.args(["vault", "set", &self.input_name, &self.input_value, "--account", account]);
                    #[cfg(windows)]
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = cmd.spawn();
                    self.reset_input();
                    return true;
                }
            }
            InputMode::EditSecret { account, field, .. } => {
                if *field == InputField::Value && !self.input_name.is_empty() && !self.input_value.is_empty() {
                    let mut cmd = std::process::Command::new("dev");
                    cmd.args(["vault", "set", &self.input_name, &self.input_value, "--account", account]);
                    #[cfg(windows)]
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    let _ = cmd.spawn();
                    self.reset_input();
                    return true;
                }
            }
            InputMode::ConfirmDelete { account, item_id, .. } => {
                let mut cmd = std::process::Command::new("dev");
                cmd.args(["vault", "delete", item_id, "--account", account]);
                #[cfg(windows)]
                cmd.creation_flags(CREATE_NO_WINDOW);
                let _ = cmd.spawn();
                self.reset_input();
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn cancel_input(&mut self) {
        self.reset_input();
    }

    fn reset_input(&mut self) {
        self.input_mode = InputMode::None;
        self.input_name.clear();
        self.input_value.clear();
        self.reveal_value = false;
    }

    pub fn start_edit(&mut self, account: &str, item_id: &str, name: &str) {
        self.input_mode = InputMode::EditSecret {
            account: account.to_string(),
            item_id: item_id.to_string(),
            field: InputField::Value,
        };
        self.input_name = name.to_string();
        self.input_value.clear();
    }

    pub fn start_delete(&mut self, account: &str, item_id: &str, name: &str) {
        self.input_mode = InputMode::ConfirmDelete {
            account: account.to_string(),
            item_id: item_id.to_string(),
            name: name.to_string(),
        };
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
            MenuItem::Secret { account, item_id, .. } => {
                // Fetch the secret value from 1Password
                crate::secrets::get_secret_value(&account, &item_id)
            }
            MenuItem::Command { task, .. } => {
                // Handle special vault add commands - enter input mode
                if task == "vault-add-prod" {
                    self.input_mode = InputMode::AddSecret {
                        account: "production".to_string(),
                        field: InputField::Name,
                    };
                    return None;
                } else if task == "vault-add-dev" {
                    self.input_mode = InputMode::AddSecret {
                        account: "development".to_string(),
                        field: InputField::Name,
                    };
                    return None;
                }

                // Inject the command text
                let cmd = format!("dev {}", task);
                Some(cmd)
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

    // Secrets submenu - show with placeholder while loading async
    let secrets_items = build_secrets_placeholder();
    items.push(MenuItem::Submenu {
        name: "secrets".to_string(),
        items: secrets_items,
    });

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

/// Build initial secrets menu with add commands and a loading indicator.
/// Actual secret items are hydrated asynchronously after the window opens.
fn build_secrets_placeholder() -> Vec<MenuItem> {
    vec![
        MenuItem::Command {
            name: "+ Add to PROD".to_string(),
            task: "vault-add-prod".to_string(),
        },
        MenuItem::Command {
            name: "+ Add to DEV".to_string(),
            task: "vault-add-dev".to_string(),
        },
        MenuItem::Command {
            name: "Loading secrets...".to_string(),
            task: "noop".to_string(),
        },
    ]
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

/// Fetch secrets from 1Password synchronously. Intended to be called from
/// a background thread so the GUI is not blocked.
pub fn fetch_secrets_blocking() -> LoadedSecrets {
    let production = crate::secrets::list_secrets("production");
    let development = crate::secrets::list_secrets("development");
    LoadedSecrets { production, development }
}
