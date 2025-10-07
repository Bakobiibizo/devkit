mod cli;
mod config;
mod envfile;
mod gitops;
mod logging;
mod runner;
mod scaffold;
mod tasks;
mod util;
mod versioning;

fn main() -> anyhow::Result<()> {
    logging::init();
    let app = cli::parse();
    runner::run(app)
}
