use crate::logging::init_logging;
use tracing::info;

#[flutter_rust_bridge::frb(sync)] // Synchronous mode for simplicity of the demo
pub fn greet(name: String) -> String {
    info!(name = %name, "greet invoked");
    format!("Hola, {}, desde Rust.", name)
}

#[flutter_rust_bridge::frb(sync)]
pub fn init() {
    init_logging();
    info!("inputvoice native library initialized");
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    flutter_rust_bridge::setup_default_user_utils();
    init_logging();
    info!("inputvoice frb init_app executed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_builds_hola_message() {
        let r = greet("World".into());
        assert_eq!(r, "Hola, World, desde Rust.");
    }

    #[test]
    fn greet_handles_empty() {
        let r = greet("".into());
        assert_eq!(r, "Hola, , desde Rust.");
    }
}
