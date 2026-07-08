//! Persistent app configuration (FR-10), stored as config.json under
//! the shared Breeze data root (see [`crate::paths`]) - the exact same
//! launcher-independent chain the logs resolve through (TD-010), so
//! config and logs can never disagree on where Breeze lives.
//!
//! Two properties drive the design.
//!
//! - Conservative merge (FR-10). Each field is validated individually,
//!   so one invalid field never discards the valid ones, and a fully
//!   corrupt file simply yields the defaults.
//! - Atomic save. The config is written to a sibling tmp file and then
//!   renamed over config.json. On Windows the std fs rename replaces
//!   the destination atomically on the same volume, so a crash in the
//!   middle of a write leaves either the old file or the new one on
//!   disk, never a truncated mix.

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::transcription::{set_language, TranscriptionLanguage};

/// Languages the "language" field accepts (v1 scope, FR-07).
const SUPPORTED_LANGUAGES: [&str; 2] = ["es", "en"];

/// Persisted user preferences (FR-10). Overlay position and mic device
/// are v2 spec fields and are intentionally not modeled yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub language: String,
    pub paused: bool,
    pub log_level: String,
    pub ui_language: String,
    /// FR-09 first-run marker: false until the full SHA-256 model
    /// verification has succeeded once, then persisted true so later
    /// launches do the fast size check only.
    pub model_verified: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: "es".to_string(),
            paused: false,
            log_level: "info".to_string(),
            ui_language: "en".to_string(),
            model_verified: false,
        }
    }
}

/// Errors surfaced by [`save`]; [`load`] never fails (defaults win).
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config directory could not be resolved")]
    NoConfigDir,
    #[error("config io failed: {0}")]
    Io(#[from] io::Error),
    #[error("config serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Conservative merge of raw JSON text into an [`AppConfig`] (FR-10).
///
/// Implementation choice, documented per the plan. The text is parsed
/// into a serde_json Value and each field is extracted with explicit
/// validation, instead of deriving with serde(default), because serde
/// aborts the whole struct on the first wrong-typed field and that
/// would discard the remaining valid fields.
///
/// Rules. Missing or wrong-typed fields keep their default without
/// touching the valid ones; unparseable JSON yields pure defaults; a
/// language outside the supported set falls back to "es".
pub fn merge_config(json_text: &str) -> AppConfig {
    let mut cfg = AppConfig::default();
    let value: serde_json::Value = match serde_json::from_str(json_text) {
        Ok(v) => v,
        Err(_) => return cfg,
    };
    if let Some(lang) = value.get("language").and_then(|v| v.as_str()) {
        if SUPPORTED_LANGUAGES.contains(&lang) {
            cfg.language = lang.to_string();
        }
    }
    if let Some(paused) = value.get("paused").and_then(|v| v.as_bool()) {
        cfg.paused = paused;
    }
    if let Some(level) = value.get("log_level").and_then(|v| v.as_str()) {
        cfg.log_level = level.to_string();
    }
    // FR-13. The UI language is validated against the same supported set;
    // anything else keeps the default ("en").
    if let Some(ui) = value.get("ui_language").and_then(|v| v.as_str()) {
        if SUPPORTED_LANGUAGES.contains(&ui) {
            cfg.ui_language = ui.to_string();
        }
    }
    // FR-09. A wrong-typed or missing marker keeps the default (false),
    // which forces a full verification rather than trusting a bad value.
    if let Some(verified) = value.get("model_verified").and_then(|v| v.as_bool()) {
        cfg.model_verified = verified;
    }
    cfg
}

/// Absolute path of config.json under the shared Breeze data root.
///
/// Resolves through [`crate::paths::breeze_root`], the same
/// launcher-independent chain the logs use (TD-010): `%APPDATA%` env
/// var, then the Known Folder data dir, then an exe-relative folder,
/// then the current directory. The root always resolves, so this
/// never returns None; the `Option` is kept so `load`/`save` keep a
/// single call shape and a future fallible root would not ripple out.
pub fn config_path() -> Option<PathBuf> {
    Some(crate::paths::breeze_root().join("config.json"))
}

/// Reads and merges config.json. Missing file, unreadable file or
/// invalid content all degrade to defaults; load never fails (FR-10).
pub fn load() -> AppConfig {
    let Some(path) = config_path() else {
        tracing::warn!("config load: data dir unresolved, using defaults");
        return AppConfig::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let cfg = merge_config(&text);
            tracing::info!(
                path = %path.display(),
                language = %cfg.language,
                paused = cfg.paused,
                log_level = %cfg.log_level,
                ui_language = %cfg.ui_language,
                model_verified = cfg.model_verified,
                "config loaded"
            );
            cfg
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            tracing::info!(path = %path.display(), "config file absent, using defaults");
            AppConfig::default()
        }
        Err(e) => {
            tracing::warn!(error = %e, "config read failed, using defaults");
            AppConfig::default()
        }
    }
}

/// Persists the config atomically. See the module docs for why the
/// write goes through config.json.tmp plus rename.
pub fn save(cfg: &AppConfig) -> Result<(), ConfigError> {
    let path = config_path().ok_or(ConfigError::NoConfigDir)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    tracing::info!(
        path = %path.display(),
        language = %cfg.language,
        paused = cfg.paused,
        log_level = %cfg.log_level,
        ui_language = %cfg.ui_language,
        model_verified = cfg.model_verified,
        "config saved"
    );
    Ok(())
}

/// Applies the config to the runtime globals. Today only the language
/// reaches the transcription global; `paused` is persisted here but
/// will be applied by the tray in Task 2 (FR-11), and `log_level` is
/// reserved for a future logging filter hookup.
pub fn apply(cfg: &AppConfig) {
    let lang = match cfg.language.as_str() {
        "en" => TranscriptionLanguage::En,
        _ => TranscriptionLanguage::Es,
    };
    set_language(lang);
    tracing::info!(language = %cfg.language, "config applied");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_valid_json_takes_every_field() {
        let cfg = merge_config(r#"{"language":"en","paused":true,"log_level":"debug"}"#);
        assert_eq!(cfg.language, "en");
        assert!(cfg.paused);
        assert_eq!(cfg.log_level, "debug");
    }

    #[test]
    fn partial_json_keeps_valid_field_and_defaults_the_rest() {
        let cfg = merge_config(r#"{"language":"en"}"#);
        assert_eq!(cfg.language, "en");
        assert!(!cfg.paused);
        assert_eq!(cfg.log_level, "info");
    }

    #[test]
    fn corrupt_json_yields_defaults() {
        assert_eq!(merge_config("garbage{"), AppConfig::default());
    }

    #[test]
    fn wrong_typed_field_defaults_without_discarding_valid_ones() {
        let cfg = merge_config(r#"{"language":"en","paused":"yes"}"#);
        assert_eq!(cfg.language, "en");
        assert!(!cfg.paused);
        assert_eq!(cfg.log_level, "info");
    }

    #[test]
    fn empty_object_yields_defaults() {
        assert_eq!(merge_config("{}"), AppConfig::default());
    }

    #[test]
    fn unsupported_language_falls_back_to_es_keeping_other_fields() {
        let cfg = merge_config(r#"{"language":"fr","paused":true}"#);
        assert_eq!(cfg.language, "es");
        assert!(cfg.paused);
    }
    #[test]
    fn ui_language_defaults_to_en_when_missing() {
        let cfg = merge_config(r#"{"language":"es","paused":false}"#);
        assert_eq!(cfg.ui_language, "en");
    }

    #[test]
    fn ui_language_accepts_supported_values() {
        let cfg = merge_config(r#"{"ui_language":"es"}"#);
        assert_eq!(cfg.ui_language, "es");
        let cfg = merge_config(r#"{"ui_language":"en"}"#);
        assert_eq!(cfg.ui_language, "en");
    }

    #[test]
    fn unsupported_ui_language_falls_back_to_en_keeping_other_fields() {
        let cfg = merge_config(r#"{"ui_language":"fr","language":"es","paused":true}"#);
        assert_eq!(cfg.ui_language, "en");
        assert_eq!(cfg.language, "es");
        assert!(cfg.paused);
    }

    #[test]
    fn wrong_typed_ui_language_defaults_to_en() {
        let cfg = merge_config(r#"{"ui_language":42}"#);
        assert_eq!(cfg.ui_language, "en");
    }

    // FR-09: the first-run verification marker. Default false so a fresh
    // install performs the full SHA-256 check once; conservative merge
    // keeps it across writes and never poisons the other fields.
    #[test]
    fn model_verified_defaults_to_false_when_missing() {
        let cfg = merge_config(r#"{"language":"es","paused":false}"#);
        assert!(!cfg.model_verified);
    }

    #[test]
    fn model_verified_reads_true_from_json() {
        let cfg = merge_config(r#"{"model_verified":true}"#);
        assert!(cfg.model_verified);
    }

    #[test]
    fn wrong_typed_model_verified_defaults_to_false_keeping_other_fields() {
        let cfg = merge_config(r#"{"model_verified":"yes","language":"en","paused":true}"#);
        assert!(!cfg.model_verified);
        assert_eq!(cfg.language, "en");
        assert!(cfg.paused);
    }

    #[test]
    fn default_config_has_model_verified_false() {
        assert!(!AppConfig::default().model_verified);
    }
}
