//! FRB config API (FR-10): load, save and update the persistent
//! config.json from Dart. Thin wrapper over [`crate::config`].

use crate::api::transcription::{set_language, TranscriptionLanguage};
use crate::config::{self, AppConfig};

/// Mirror of [`AppConfig`] for the FRB scanner (only `api::` types
/// cross the bridge, same pattern as the input DTOs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfigDto {
    pub language: String,
    pub paused: bool,
    pub log_level: String,
    pub ui_language: String,
    pub model_verified: bool,
}

impl From<AppConfig> for AppConfigDto {
    fn from(cfg: AppConfig) -> Self {
        Self {
            language: cfg.language,
            paused: cfg.paused,
            log_level: cfg.log_level,
            ui_language: cfg.ui_language,
            model_verified: cfg.model_verified,
        }
    }
}

impl From<AppConfigDto> for AppConfig {
    fn from(dto: AppConfigDto) -> Self {
        Self {
            language: dto.language,
            paused: dto.paused,
            log_level: dto.log_level,
            ui_language: dto.ui_language,
            model_verified: dto.model_verified,
        }
    }
}

/// Typed error exposed to Dart for config operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigApiError {
    #[error("unsupported language: {0} (expected es or en)")]
    UnsupportedLanguage(String),
    #[error("config persistence failed: {0}")]
    Persist(String),
}

/// Loads config.json (defaults when absent or invalid), applies it to
/// the runtime globals and returns it to Dart.
///
/// Design choice, per the plan: apply happens INSIDE load_config so
/// main.dart stays a single call with no duplicated mapping logic.
///
/// The merged result is also written back to disk. The write-back is
/// what creates the file with defaults on first launch (FR-10.AC-1)
/// and what heals a corrupt file into valid JSON (FR-10.AC-2). A
/// write-back failure is logged and swallowed on purpose, because a
/// read-only config directory must not abort startup.
pub fn load_config() -> AppConfigDto {
    let cfg = config::load();
    config::apply(&cfg);
    if let Err(e) = config::save(&cfg) {
        tracing::warn!(error = %e, "load_config: write-back failed");
    }
    cfg.into()
}

/// Persists the given config atomically (save on change, FR-10).
pub fn save_config(cfg: AppConfigDto) -> Result<(), ConfigApiError> {
    config::save(&AppConfig::from(cfg)).map_err(|e| ConfigApiError::Persist(e.to_string()))
}

/// Validates the language, persists it into config.json and applies
/// it to the transcription global. The tray selector (Task 2, FR-07)
/// will call this. Validation happens before any file access, so an
/// unsupported value never touches the disk.
pub fn update_language(lang: String) -> Result<(), ConfigApiError> {
    let normalized = lang.trim().to_lowercase();
    let engine_lang = match normalized.as_str() {
        "es" => TranscriptionLanguage::Es,
        "en" => TranscriptionLanguage::En,
        _ => return Err(ConfigApiError::UnsupportedLanguage(lang)),
    };
    let mut cfg = config::load();
    cfg.language = normalized;
    config::save(&cfg).map_err(|e| ConfigApiError::Persist(e.to_string()))?;
    set_language(engine_lang);
    Ok(())
}

/// Validates the UI language, persists it into config.json and leaves
/// the transcription language untouched (FR-13). The tray interface
/// language selector calls this. Validation happens before any file
/// access, so an unsupported value never touches the disk.
pub fn update_ui_language(lang: String) -> Result<(), ConfigApiError> {
    let normalized = lang.trim().to_lowercase();
    if normalized != "es" && normalized != "en" {
        return Err(ConfigApiError::UnsupportedLanguage(lang));
    }
    let mut cfg = config::load();
    cfg.ui_language = normalized;
    config::save(&cfg).map_err(|e| ConfigApiError::Persist(e.to_string()))?;
    Ok(())
}

/// Persists the FR-11 pause flag with a conservative merge (I-1): loads
/// the current config, mutates only `paused` and saves atomically, so a
/// pause/resume toggle can never clobber `model_verified` or any other
/// field written elsewhere (main.dart, the on-demand verify). This is
/// why the tray must call this instead of a wholesale `save_config`.
pub fn update_paused(paused: bool) -> Result<(), ConfigApiError> {
    let mut cfg = config::load();
    cfg.paused = paused;
    config::save(&cfg).map_err(|e| ConfigApiError::Persist(e.to_string()))
}

/// Persists the FR-09 first-run marker. main.dart sets it true after a
/// successful full-verify startup so later launches skip the expensive
/// SHA-256 check; the on-demand tray verification sets it false on
/// failure so the next launch re-verifies. Merges over the current
/// config so no other field is disturbed.
pub fn update_model_verified(verified: bool) -> Result<(), ConfigApiError> {
    let mut cfg = config::load();
    cfg.model_verified = verified;
    config::save(&cfg).map_err(|e| ConfigApiError::Persist(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rejection path must fire before any disk access, so it is
    /// safe to exercise in a unit test (no APPDATA side effects).
    #[test]
    fn update_language_rejects_unsupported_value() {
        let result = update_language("fr".to_string());
        assert!(matches!(
            result,
            Err(ConfigApiError::UnsupportedLanguage(_))
        ));
    }

    #[test]
    fn dto_roundtrips_to_config_and_back() {
        let dto = AppConfigDto {
            language: "en".to_string(),
            paused: true,
            log_level: "debug".to_string(),
            ui_language: "es".to_string(),
            model_verified: true,
        };
        let cfg = AppConfig::from(dto.clone());
        assert_eq!(AppConfigDto::from(cfg), dto);
    }
    #[test]
    fn update_ui_language_rejects_unsupported_value() {
        let result = update_ui_language("de".to_string());
        assert!(matches!(
            result,
            Err(ConfigApiError::UnsupportedLanguage(_))
        ));
    }

    /// I-1: update_paused must merge, not clobber. Seed a config.json that
    /// has model_verified=true, flip paused via the merge-style API, and
    /// assert model_verified survived. Uses the same injectable-path seam
    /// the data root resolves through (%APPDATA%, paths.rs / TD-010): point
    /// it at a temp dir so no real user config is touched. Env mutation is
    /// process-global, so this is the crate's only APPDATA-writing test to
    /// keep it race-free under the default parallel runner.
    #[test]
    fn update_paused_preserves_model_verified() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::env::set_var("APPDATA", tmp.path());

        // Seed a persisted config with the marker set true and paused false.
        let seeded = AppConfig {
            language: "es".to_string(),
            paused: false,
            log_level: "info".to_string(),
            ui_language: "en".to_string(),
            model_verified: true,
        };
        config::save(&seeded).expect("seed save");

        update_paused(true).expect("update_paused");

        let reloaded = config::load();
        assert!(reloaded.paused, "paused must be flipped to true");
        assert!(
            reloaded.model_verified,
            "model_verified must survive a pause toggle (I-1)"
        );

        std::env::remove_var("APPDATA");
    }
}
