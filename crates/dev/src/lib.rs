mod cli;
mod cli_help;
mod commands;
mod config;
mod core;
mod dispatch;
mod envfile;
mod gitops;
mod guard;
mod logging;
mod review;
mod scaffold;
mod setup;
mod tasks;
mod templates;
mod versioning;
mod walk;

pub fn run_main() -> anyhow::Result<()> {
    logging::init();
    let app = cli::parse()?;
    dispatch::run(app)
}
