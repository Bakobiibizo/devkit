mod cli;
mod cli_help;
mod commands;
mod config;
mod core;
mod dispatch;
mod dockergen;
mod envfile;
mod gitops;
mod logging;
mod review;
mod scaffold;
mod setup;
mod tasks;
mod templates;
mod vault;
mod versioning;
mod walk;

fn main() -> anyhow::Result<()> {
    logging::init();
    let app = cli::parse()?;
    dispatch::run(app)
}
