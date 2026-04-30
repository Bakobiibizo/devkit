mod cli;
mod config;
mod dockergen;
mod envfile;
mod gitops;
mod init;
mod logging;
mod review;
mod runner;
mod scaffold;
mod setup;
mod tasks;
mod templates;
mod vault;
mod versioning;
mod walk;

fn main() -> anyhow::Result<()> {
    logging::init();
    let app = cli::parse();
    runner::run(app)
}
