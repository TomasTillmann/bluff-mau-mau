//! One durable checkpoint per training run, with an exclusive lifetime writer lock.
//! A killed process can lose the unsaved training batch, but never expose a partial
//! checkpoint: only a synced temporary file is renamed over the previous one.
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

type Result<T> = std::result::Result<T, String>;
const MAGIC: &[u8; 8] = b"BMCCFR\0\0";
const VERSION: u32 = 1;
const HEADER_LEN: usize = 28;
const CHECKSUM_LEN: usize = 32;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub struct RunStore {
    directory: PathBuf,
    model_id: String,
    _lock: File,
}

impl RunStore {
    pub fn open(path: &Path, model_id: &str) -> Result<Self> {
        if model_id.is_empty() || path.as_os_str().is_empty() {
            return Err("Run directory and model identity must not be empty".into());
        }
        // Sync newly created directory entries as well as checkpoint replacements.
        let mut missing = Vec::new();
        for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
            if ancestor.try_exists().map_err(|error| error.to_string())? {
                break;
            }
            missing.push(ancestor.to_path_buf());
        }
        fs::create_dir_all(path).map_err(|error| error.to_string())?;
        for directory in missing.iter().rev() {
            sync_directory(
                directory
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?;
        }
        let directory = fs::canonicalize(path).map_err(|error| error.to_string())?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(".writer.lock"))
            .map_err(|error| error.to_string())?;
        FileExt::try_lock_exclusive(&lock)
            .map_err(|error| format!("Cannot acquire training-run writer lock: {error}"))?;
        let store = Self {
            directory,
            model_id: model_id.to_owned(),
            _lock: lock,
        };
        store.load()?;
        Ok(store)
    }

    pub fn load(&self) -> Result<Option<Vec<u8>>> {
        let bytes = match fs::read(self.directory.join("checkpoint.bin")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Cannot read checkpoint: {error}")),
        };
        if bytes.len() < HEADER_LEN + CHECKSUM_LEN || &bytes[..8] != MAGIC {
            return Err("Checkpoint is truncated or has an invalid format".into());
        }
        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let model_len = usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap()))
            .map_err(|_| "Checkpoint model length is too large")?;
        let payload_len = usize::try_from(u64::from_le_bytes(bytes[20..28].try_into().unwrap()))
            .map_err(|_| "Checkpoint payload length is too large")?;
        let payload_start = HEADER_LEN
            .checked_add(model_len)
            .ok_or("Checkpoint length overflow")?;
        let payload_end = payload_start
            .checked_add(payload_len)
            .ok_or("Checkpoint length overflow")?;
        if payload_end.checked_add(CHECKSUM_LEN) != Some(bytes.len()) {
            return Err("Checkpoint is truncated or has an invalid length".into());
        }
        if Sha256::digest(&bytes[..payload_end]).as_slice() != &bytes[payload_end..] {
            return Err("Checkpoint checksum mismatch".into());
        }
        if version != VERSION {
            return Err(format!("Unsupported checkpoint version {version}"));
        }
        if &bytes[HEADER_LEN..payload_start] != self.model_id.as_bytes() {
            return Err("Checkpoint belongs to a different model".into());
        }
        Ok(Some(bytes[payload_start..payload_end].to_vec()))
    }

    pub fn save(&mut self, payload: &[u8]) -> Result<()> {
        // Refuse to overwrite a corrupt or externally replaced checkpoint.
        self.load()?;
        let (temporary, mut output) = loop {
            let number = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                self.directory
                    .join(format!(".checkpoint.{}.{}.tmp", std::process::id(), number));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => break (path, file),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("Cannot create checkpoint temporary file: {error}"));
                }
            }
        };
        let result = (|| -> Result<()> {
            let mut header = Vec::with_capacity(HEADER_LEN);
            header.extend_from_slice(MAGIC);
            header.extend_from_slice(&VERSION.to_le_bytes());
            header.extend_from_slice(&(self.model_id.len() as u64).to_le_bytes());
            header.extend_from_slice(&(payload.len() as u64).to_le_bytes());
            let mut checksum = Sha256::new();
            for part in [header.as_slice(), self.model_id.as_bytes(), payload] {
                checksum.update(part);
                output
                    .write_all(part)
                    .map_err(|error| format!("Cannot write checkpoint: {error}"))?;
            }
            output
                .write_all(&checksum.finalize())
                .map_err(|error| error.to_string())?;
            output
                .sync_all()
                .map_err(|error| format!("Cannot sync checkpoint: {error}"))?;
            fs::rename(&temporary, self.directory.join("checkpoint.bin"))
                .map_err(|error| format!("Cannot replace checkpoint: {error}"))?;
            sync_directory(&self.directory)
                .map_err(|error| format!("Checkpoint replaced, but directory sync failed: {error}"))
        })();
        drop(output);
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            loop {
                let number = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "bluff-training-store-test-{}-{number}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create test directory: {error}"),
                }
            }
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trip_model_binding_and_exclusive_writer() {
        let directory = TestDirectory::new();
        let nested = directory.0.join("parent/run");
        let mut store = RunStore::open(&nested, "model-v1").unwrap();
        assert_eq!(store.load().unwrap(), None);
        assert!(RunStore::open(&nested, "model-v1").is_err());
        store.save(&[0, 255, 17]).unwrap();
        assert_eq!(store.load().unwrap(), Some(vec![0, 255, 17]));
        store.save(b"next completed iteration").unwrap();
        drop(store);
        assert!(RunStore::open(&nested, "model-v2").is_err());
        let store = RunStore::open(&nested, "model-v1").unwrap();
        assert_eq!(
            store.load().unwrap(),
            Some(b"next completed iteration".to_vec())
        );
    }

    #[test]
    fn corrupt_and_truncated_checkpoints_are_never_overwritten() {
        for truncate in [false, true] {
            let directory = TestDirectory::new();
            let mut store = RunStore::open(&directory.0, "model").unwrap();
            store.save(b"complete payload").unwrap();
            let path = directory.0.join("checkpoint.bin");
            let mut corrupt = fs::read(&path).unwrap();
            if truncate {
                corrupt.truncate(13);
            } else {
                corrupt[HEADER_LEN + "model".len()] ^= 1;
            }
            fs::write(&path, &corrupt).unwrap();
            assert!(store.load().is_err());
            assert!(store.save(b"must not overwrite").is_err());
            assert_eq!(fs::read(&path).unwrap(), corrupt);
            drop(store);
            assert!(RunStore::open(&directory.0, "model").is_err());
        }
    }

    #[test]
    fn interrupted_temporary_file_does_not_replace_completed_checkpoint() {
        let directory = TestDirectory::new();
        let mut store = RunStore::open(&directory.0, "model").unwrap();
        store.save(b"durable checkpoint").unwrap();
        let checkpoint = fs::read(directory.0.join("checkpoint.bin")).unwrap();
        drop(store);
        fs::write(directory.0.join(".checkpoint.123.0.tmp"), b"partial").unwrap();
        let mut resumed = RunStore::open(&directory.0, "model").unwrap();
        assert_eq!(
            resumed.load().unwrap(),
            Some(b"durable checkpoint".to_vec())
        );
        assert_eq!(
            fs::read(directory.0.join("checkpoint.bin")).unwrap(),
            checkpoint
        );
        resumed.save(b"next checkpoint").unwrap();
        assert_eq!(resumed.load().unwrap(), Some(b"next checkpoint".to_vec()));
    }
}
