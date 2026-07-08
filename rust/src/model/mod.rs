//! ModelManager (FR-09): locates and verifies the embedded Whisper model.
//! Downloads nothing: the model ships in the installer (or lives in
//! %LOCALAPPDATA%\Breeze\models\ during development).

use std::path::{Path, PathBuf};
use thiserror::Error;

pub const MODEL_FILE_NAME: &str = "ggml-large-v3-turbo-q5_0.bin";
pub const MODEL_SIZE_BYTES: u64 = 574_041_195;
pub const MODEL_SHA256: &str =
    "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2";

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("model not found in any search path")]
    NotFound,
    #[error("model size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("model hash mismatch")]
    HashMismatch,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Searches the given paths (in order) for the model and validates its size.
///
/// The first file found decides: if it exists but the size does not match,
/// returns `SizeMismatch` immediately (does not keep searching).
pub fn locate_model(search_dirs: &[PathBuf]) -> Result<PathBuf, ModelError> {
    for dir in search_dirs {
        let candidate = dir.join(MODEL_FILE_NAME);
        // Single fs::metadata call instead of exists() + metadata(): no
        // TOCTOU window, and permission errors surface as Io instead of
        // being silently treated as "not found".
        let metadata = match std::fs::metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(ModelError::Io(e)),
        };
        let actual = metadata.len();
        if actual == MODEL_SIZE_BYTES {
            return Ok(candidate);
        }
        return Err(ModelError::SizeMismatch {
            expected: MODEL_SIZE_BYTES,
            actual,
        });
    }
    Err(ModelError::NotFound)
}

/// Full integrity verification. Expensive (~2-4 s on the real model).
/// `expected_hex` is injected for testability; production uses MODEL_SHA256.
///
/// Reads the file in 1 MB chunks to avoid loading all 574 MB into RAM.
pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<(), ModelError> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let actual_hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    if actual_hex == expected_hex.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(ModelError::HashMismatch)
    }
}

/// Default search paths: exe dir + %LOCALAPPDATA%.
pub fn default_search_dirs() -> Vec<PathBuf> {
    let exe_models = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("models")));
    let local_models = dirs::data_local_dir().map(|d| d.join("Breeze").join("models"));
    [exe_models, local_models].into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn locate_model_not_found_in_empty_dirs() {
        let dir = tempdir().unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let result = locate_model(&dirs);
        assert!(matches!(result, Err(ModelError::NotFound)));
    }

    #[test]
    fn locate_model_rejects_wrong_size() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(MODEL_FILE_NAME);
        let file = File::create(&path).unwrap();
        file.set_len(10).unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let result = locate_model(&dirs);
        assert!(matches!(
            result,
            Err(ModelError::SizeMismatch {
                expected: MODEL_SIZE_BYTES,
                actual: 10
            })
        ));
    }

    #[test]
    fn locate_model_accepts_correct_size() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(MODEL_FILE_NAME);
        let file = File::create(&path).unwrap();
        file.set_len(MODEL_SIZE_BYTES).unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let result = locate_model(&dirs).unwrap();
        assert_eq!(result, path);
    }

    #[test]
    fn locate_model_prefers_first_dir() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();
        for dir in [&dir1, &dir2] {
            let path = dir.path().join(MODEL_FILE_NAME);
            let file = File::create(&path).unwrap();
            file.set_len(MODEL_SIZE_BYTES).unwrap();
        }
        let dirs = vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()];
        let result = locate_model(&dirs).unwrap();
        assert_eq!(result, dir1.path().join(MODEL_FILE_NAME));
    }

    #[test]
    fn verify_sha256_accepts_known_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known.bin");
        std::fs::write(&path, b"hello world").unwrap();
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        verify_sha256(&path, expected).unwrap();
    }

    #[test]
    fn verify_sha256_rejects_tampered() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known.bin");
        std::fs::write(&path, b"hello world").unwrap();
        let expected = "0000000000000000000000000000000000000000000000000000000000000000";
        let result = verify_sha256(&path, expected);
        assert!(matches!(result, Err(ModelError::HashMismatch)));
    }

    #[test]
    #[ignore = "requiere el modelo real en %LOCALAPPDATA%; ~3 s"]
    fn verify_real_model_hash() {
        let dirs = default_search_dirs();
        let path = locate_model(&dirs).expect("model must exist for this test");
        verify_sha256(&path, MODEL_SHA256).expect("hash must match");
    }
}
