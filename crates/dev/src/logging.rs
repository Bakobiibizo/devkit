use std::sync::OnceLock;

use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};

static INIT: OnceLock<()> = OnceLock::new();

/// Initialize logging/telemetry backends using `tracing`.
pub fn init() {
    INIT.get_or_init(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let subscriber = Registry::default()
            .with(filter)
            .with(fmt::layer().with_target(false));
        if tracing::subscriber::set_global_default(subscriber).is_err() {
            // Ignore error if a subscriber is already set (e.g., tests).
        }
    });
}
