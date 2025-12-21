mod cli;
mod config;
mod envfile;
mod gitops;
mod logging;
mod review;
mod runner;
mod scaffold;
mod setup;
mod dockergen;
mod tasks;
mod versioning;
mod walk;

fn main() -> anyhow::Result<()> {
    logging::init();
    let app = cli::parse();
    runner::run(app)
}
