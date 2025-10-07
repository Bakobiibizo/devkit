use std::collections::BTreeMap;

use anyhow::{bail, Result};

use crate::config::{DevConfig, Task as TaskConfig};

/// Flattened representation of runnable tasks.
#[derive(Default)]
pub struct TaskIndex {
    tasks: BTreeMap<String, Task>,
}

impl TaskIndex {
    pub fn get(&self, name: &str) -> Option<&Task> {
        self.tasks.get(name)
    }

    pub fn insert(&mut self, name: String, task: Task) {
        self.tasks.insert(name, task);
    }
}

/// Simplified executable task. The full model will expand composite references.
#[derive(Clone)]
pub struct Task {
    pub name: String,
    pub steps: Vec<TaskStep>,
    pub allow_fail: bool,
}

#[derive(Clone)]
pub enum TaskStep {
    Command(Vec<String>),
    TaskRef(String),
}

/// Build an index from the provided configuration. Currently a stub that validates presence.
pub fn build_index(config: &DevConfig) -> Result<TaskIndex> {
    let mut index = TaskIndex::default();
    let Some(tasks) = &config.tasks else {
        return Ok(index);
    };

    for (name, task) in tasks {
        index.insert(name.clone(), parse_task(name, task)?);
    }

    Ok(index)
}

fn parse_task(name: &str, task: &TaskConfig) -> Result<Task> {
    let mut steps = Vec::new();
    for value in &task.commands {
        match value {
            toml::Value::String(reference) => steps.push(TaskStep::TaskRef(reference.clone())),
            toml::Value::Array(items) => {
                let mut command = Vec::new();
                for item in items {
                    let Some(arg) = item.as_str() else {
                        bail!("task {name} contains non-string command argument: {item:?}");
                    };
                    command.push(arg.to_owned());
                }
                steps.push(TaskStep::Command(command));
            }
            other => {
                bail!("task {name} contains unsupported command value: {other:?}");
            }
        }
    }

    Ok(Task {
        name: name.to_owned(),
        steps,
        allow_fail: task.allow_fail,
    })
}
