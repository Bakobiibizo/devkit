use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;
use clap::Parser;

use crate::cli::{Cli, Command, LanguageCommand, Verb};
use crate::config::DevConfig;
use crate::envfile;
use crate::tasks::TaskIndex;
use crate::{commands, config};

fn config_root_dir(config_path: &Utf8PathBuf) -> PathBuf {
    let p = Path::new(config_path.as_str());
    let parent = p.parent().unwrap_or(Path::new("."));

    if parent.file_name() == Some(std::ffi::OsStr::new(".dev")) {
        return parent.parent().unwrap_or(parent).to_path_buf();
    }

    if parent.file_name() == Some(std::ffi::OsStr::new("dev"))
        && let Some(tools) = parent.parent()
        && tools.file_name() == Some(std::ffi::OsStr::new("tools"))
    {
        return tools.parent().unwrap_or(tools).to_path_buf();
    }

    parent.to_path_buf()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigPathSource {
    Explicit,
    Discovered,
    HomeDefault,
}

impl ConfigPathSource {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            ConfigPathSource::Explicit => "explicit",
            ConfigPathSource::Discovered => "discovered",
            ConfigPathSource::HomeDefault => "home-default",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedConfigPath {
    pub(crate) path: Utf8PathBuf,
    pub(crate) source: ConfigPathSource,
}

fn normalize_external(cli: Cli) -> Result<Cli> {
    let Command::External(extra) = &cli.command else {
        return Ok(cli);
    };

    if extra.is_empty() {
        return Ok(cli);
    }

    let mut argv: Vec<String> = Vec::new();
    argv.push("dev".to_owned());

    if let Some(chdir) = &cli.chdir {
        argv.push("--chdir".to_owned());
        argv.push(chdir.to_string_lossy().to_string());
    }

    if let Some(file) = &cli.file {
        argv.push("--file".to_owned());
        argv.push(file.to_string_lossy().to_string());
    }

    if let Some(language) = &cli.language {
        argv.push("--language".to_owned());
        argv.push(language.clone());
    }

    if cli.dry_run {
        argv.push("--dry-run".to_owned());
    }

    if cli.no_color {
        argv.push("--no-color".to_owned());
    }

    for _ in 0..cli.verbose {
        argv.push("--verbose".to_owned());
    }

    argv.push("--project".to_owned());
    argv.push(extra[0].clone());

    argv.extend(extra[1..].iter().cloned());

    Cli::try_parse_from(argv).map_err(|err| anyhow!(err.to_string()))
}

pub fn run(cli: Cli) -> Result<()> {
    let cli = normalize_external(cli)?;
    let ctx = CliContext::from(&cli);
    ctx.apply_chdir()?;

    let _ = ctx.no_color;
    let _ = ctx.verbose;

    match cli.command {
        Command::Config { command } => commands::config::handle(&ctx, command),
        Command::Language {
            command: LanguageCommand::Set { name },
        } => commands::language::handle_set(&ctx, name),
        Command::Setup {
            command,
            skip_installed,
            no_deps,
        } => commands::setup::handle(&ctx, command, skip_installed, no_deps),
        Command::Review {
            output,
            include_working,
            main,
        } => commands::review::handle(&ctx, output, include_working, main),
        Command::Guard(args) => commands::guard::handle(&ctx, args),
        Command::Walk {
            directory,
            output,
            stdout,
            format: _format,
            max_depth,
            no_content,
            extensions,
            include_hidden,
        } => commands::walk::handle(
            &ctx,
            commands::walk::WalkRequest {
                directory,
                output,
                stdout,
                max_depth,
                no_content,
                extensions,
                include_hidden,
            },
        ),
        other => {
            let state = AppState::new(ctx)?;
            handle_with_state(&state, other)
        }
    }
}

fn handle_with_state(state: &AppState, command: Command) -> Result<()> {
    match command {
        Command::List => commands::task::handle_list(state),
        Command::Run { task } => commands::task::handle_run(state, &task),
        Command::Start(args) => commands::task::handle_start(state, args),
        Command::Fmt => commands::task::handle_verb(state, Verb::Fmt),
        Command::Lint => commands::task::handle_verb(state, Verb::Lint),
        Command::TypeCheck => commands::task::handle_verb(state, Verb::TypeCheck),
        Command::Test => commands::task::handle_verb(state, Verb::Test),
        Command::Fix => commands::task::handle_verb(state, Verb::Fix),
        Command::Check => commands::task::handle_verb(state, Verb::Check),
        Command::Ci => commands::task::handle_verb(state, Verb::Ci),
        Command::All { verb } => commands::task::handle_all(state, verb),
        Command::Install(args) => commands::task::handle_install(state, args),
        Command::Language { command } => commands::language::handle(state, command),
        Command::Git { command } => commands::git::handle(state, command),
        Command::Version { command } => commands::version::handle(state, command),
        Command::Update(args) => commands::update::handle(&state.ctx, args),
        Command::Env(args) => commands::env::handle(state, args),
        Command::Config { .. } => unreachable!("config commands handled earlier"),
        Command::Setup { .. } => unreachable!("setup commands handled earlier"),
        Command::Review { .. } => unreachable!("review commands handled earlier"),
        Command::Guard(_) => unreachable!("guard command handled earlier"),
        Command::Walk { .. } => unreachable!("walk commands handled earlier"),
        Command::External(extra) => {
            bail!("unknown command: {}", extra.join(" "))
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct CliContext {
    pub(crate) chdir: Option<PathBuf>,
    pub(crate) file: Option<PathBuf>,
    pub(crate) project: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) verbose: u8,
    pub(crate) no_color: bool,
}

impl CliContext {
    pub(crate) fn apply_chdir(&self) -> Result<()> {
        if let Some(path) = &self.chdir {
            std::env::set_current_dir(path)
                .with_context(|| format!("changing directory to {}", path.display()))?;
        }
        Ok(())
    }

    pub(crate) fn resolve_config_path(&self) -> Result<ResolvedConfigPath> {
        /// Return the platform suffix for config file selection.
        fn platform_config_suffix() -> &'static str {
            match std::env::consts::OS {
                "linux" => "linux",
                "windows" => "windows",
                "macos" => "macos",
                _ => "linux",
            }
        }

        if let Some(path) = &self.file {
            let path = Utf8PathBuf::from_path_buf(path.clone())
                .map_err(|_| anyhow!("config path must be valid UTF-8"))?;
            return Ok(ResolvedConfigPath {
                path,
                source: ConfigPathSource::Explicit,
            });
        }

        if let Ok(cwd) = std::env::current_dir()
            && let Ok(mut dir) = Utf8PathBuf::from_path_buf(cwd)
        {
            loop {
                let platform_suffix = platform_config_suffix();

                // .dev/ path — platform-specific first, then generic
                let platform_preferred = dir
                    .join(".dev")
                    .join(format!("config.{}.toml", platform_suffix));
                if platform_preferred.exists() {
                    return Ok(ResolvedConfigPath {
                        path: platform_preferred,
                        source: ConfigPathSource::Discovered,
                    });
                }

                let preferred = dir.join(".dev").join("config.toml");
                if preferred.exists() {
                    return Ok(ResolvedConfigPath {
                        path: preferred,
                        source: ConfigPathSource::Discovered,
                    });
                }

                // Legacy tools/dev/ path — same priority
                let legacy_platform = dir
                    .join("tools")
                    .join("dev")
                    .join(format!("config.{}.toml", platform_suffix));
                if legacy_platform.exists() {
                    return Ok(ResolvedConfigPath {
                        path: legacy_platform,
                        source: ConfigPathSource::Discovered,
                    });
                }

                let legacy = dir.join("tools").join("dev").join("config.toml");
                if legacy.exists() {
                    return Ok(ResolvedConfigPath {
                        path: legacy,
                        source: ConfigPathSource::Discovered,
                    });
                }

                let Some(parent) = dir.parent() else {
                    break;
                };
                dir = parent.to_path_buf();
            }
        }

        let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to determine home directory"))?;
        let dev_dir = Utf8PathBuf::from_path_buf(home.join(".dev"))
            .map_err(|_| anyhow!("config path must be valid UTF-8"))?;

        let platform_path = dev_dir.join(format!("config.{}.toml", platform_config_suffix()));
        if platform_path.exists() {
            return Ok(ResolvedConfigPath {
                path: platform_path,
                source: ConfigPathSource::HomeDefault,
            });
        }

        let path = dev_dir.join("config.toml");
        Ok(ResolvedConfigPath {
            path,
            source: ConfigPathSource::HomeDefault,
        })
    }

    pub(crate) fn effective_language(
        &self,
        config: &DevConfig,
        project_language: Option<&str>,
        override_lang: Option<String>,
    ) -> Option<String> {
        override_lang
            .or_else(|| self.language.clone())
            .or_else(|| project_language.map(|s| s.to_owned()))
            .or_else(|| config.default_language.clone())
    }
}

impl From<&Cli> for CliContext {
    fn from(cli: &Cli) -> Self {
        Self {
            chdir: cli.chdir.clone(),
            file: cli.file.clone(),
            project: cli.project.clone(),
            language: cli.language.clone(),
            dry_run: cli.dry_run,
            verbose: cli.verbose,
            no_color: cli.no_color,
        }
    }
}

pub(crate) struct AppState {
    pub(crate) ctx: CliContext,
    pub(crate) config_path: Utf8PathBuf,
    pub(crate) config_source: ConfigPathSource,
    pub(crate) config: DevConfig,
    pub(crate) project_language: Option<String>,
    pub(crate) tasks: TaskIndex,
}

impl AppState {
    pub(crate) fn new(ctx: CliContext) -> Result<Self> {
        let resolved = ctx.resolve_config_path()?;
        let config_path = resolved.path;
        let config_source = resolved.source;
        let config = if config_source == ConfigPathSource::HomeDefault && !config_path.exists() {
            DevConfig::empty()
        } else {
            config::load_from_path(&config_path)?
        };
        let config_root = config_root_dir(&config_path);

        let requested_project = ctx
            .project
            .clone()
            .or_else(|| config.default_project.clone());
        let mut project_language: Option<String> = None;

        if let Some(project) = requested_project.as_deref() {
            let projects = config.projects.as_ref().with_context(|| {
                format!("project `{}` requested but no projects configured", project)
            })?;
            let spec = projects
                .get(project)
                .with_context(|| format!("unknown project `{}`", project))?;

            if let Some(chdir) = &spec.chdir {
                let chdir_path = Path::new(chdir);
                let target = if chdir_path.is_absolute() {
                    chdir_path.to_path_buf()
                } else {
                    config_root.join(chdir_path)
                };

                std::env::set_current_dir(&target).with_context(|| {
                    format!(
                        "changing directory to project `{}` at {}",
                        project,
                        target.display()
                    )
                })?;
            }
            project_language = spec.language.clone();
        }

        let tasks = TaskIndex::from_config(&config)?;
        Ok(Self {
            ctx,
            config_path,
            config_source,
            config,
            project_language,
            tasks,
        })
    }

    pub(crate) fn effective_language(&self, override_lang: Option<String>) -> Option<String> {
        self.ctx.effective_language(
            &self.config,
            self.project_language.as_deref(),
            override_lang,
        )
    }

    pub(crate) fn env_path(&self) -> Result<Utf8PathBuf> {
        let cwd = envfile::current_working_dir()?;
        envfile::locate(&cwd)
    }
}

pub(crate) fn exit_code_display(status: std::process::ExitStatus) -> String {
    status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated".to_owned())
}
