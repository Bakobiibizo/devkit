use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

use crate::config::{DevConfig, Task as TaskConfig};

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub origin: String,
    pub argv: Vec<String>,
    pub allow_fail: bool,
}

#[derive(Default)]
pub struct TaskIndex {
    tasks: BTreeMap<String, Task>,
}

impl TaskIndex {
    pub fn from_config(config: &DevConfig) -> Result<Self> {
        let mut index = TaskIndex::default();
        let Some(tasks) = &config.tasks else {
            return Ok(index);
        };

        for (name, task) in tasks {
            index.tasks.insert(
                name.clone(),
                parse_task(name, task).with_context(|| format!("parsing task `{name}`"))?,
            );
        }

        Ok(index)
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn task_names(&self) -> impl Iterator<Item = &String> {
        self.tasks.keys()
    }

    pub fn flatten(&self, task: &str) -> Result<Vec<CommandSpec>> {
        let mut stack = Vec::new();
        self.flatten_internal(task, false, &mut stack)
    }

    fn flatten_internal(
        &self,
        task: &str,
        inherited_allow_fail: bool,
        stack: &mut Vec<String>,
    ) -> Result<Vec<CommandSpec>> {
        if stack.contains(&task.to_owned()) {
            let cycle = stack
                .iter()
                .chain(std::iter::once(&task.to_owned()))
                .cloned()
                .collect::<Vec<_>>()
                .join(" -> ");
            bail!("task recursion detected: {cycle}");
        }

        let definition = self
            .tasks
            .get(task)
            .with_context(|| format!("unknown task `{task}`"))?;

        stack.push(task.to_owned());
        let mut commands = Vec::new();
        let allow_fail = inherited_allow_fail || definition.allow_fail;
        for step in &definition.steps {
            match step {
                TaskStep::Command(argv) => {
                    if argv.is_empty() {
                        bail!("task `{task}` contains an empty command");
                    }
                    commands.push(CommandSpec {
                        origin: task.to_owned(),
                        argv: argv.clone(),
                        allow_fail,
                    });
                }
                TaskStep::TaskRef(name) => {
                    let mut nested = self.flatten_internal(name, allow_fail, stack)?;
                    commands.append(&mut nested);
                }
            }
        }
        stack.pop();
        Ok(commands)
    }
}

#[derive(Clone)]
struct Task {
    pub allow_fail: bool,
    pub steps: Vec<TaskStep>,
}

#[derive(Clone)]
enum TaskStep {
    Command(Vec<String>),
    TaskRef(String),
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
                        bail!("task `{name}` contains non-string command argument: {item:?}");
                    };
                    command.push(arg.to_owned());
                }
                steps.push(TaskStep::Command(command));
            }
            other => {
                bail!("task `{name}` contains unsupported command value: {other:?}");
            }
        }
    }

    Ok(Task {
        allow_fail: task.allow_fail,
        steps,
    })
}
