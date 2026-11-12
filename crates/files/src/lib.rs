//! The file store (D-033): uploaded files, thumbnails, avatars and logos as files in one
//! directory, named by keys like `2026/10/0192b7c4-….png`. The database keeps the keys; this
//! crate only writes, reads and removes files. Several nodes share the directory.
//!
//! A file arrives in `tmp/` while it is counted and hashed, and moves under its key only when it
//! is whole and within the limit, so a key never names a partial file.

use std::io;
use std::path::{Component, Path, PathBuf};

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// How many first bytes [`Received::head`] keeps, for recognizing the kind of file.
pub const HEAD_LEN: usize = 512;

/// A file written by [`FileStore::receive`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    pub key: String,
    pub size: u64,
    pub sha256: [u8; 32],
    /// The first bytes, at most [`HEAD_LEN`].
    pub head: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum ReceiveError {
    /// The file is larger than the limit; nothing was kept.
    #[error("the file is larger than {0} bytes")]
    TooLarge(u64),
    /// The request body broke off.
    #[error("the upload was interrupted: {0}")]
    Body(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Creates the directory and its `tmp/` part; called at startup.
    pub async fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(self.root.join("tmp")).await
    }

    /// A new key for a file of this extension (without the dot; empty for none).
    fn new_key(extension: &str) -> String {
        let now = time::OffsetDateTime::now_utc();
        let id = uuid::Uuid::now_v7();
        if extension.is_empty() {
            format!("{:04}/{:02}/{id}", now.year(), u8::from(now.month()))
        } else {
            format!(
                "{:04}/{:02}/{id}.{extension}",
                now.year(),
                u8::from(now.month())
            )
        }
    }

    /// Moves a whole file from `tmp/` under a new key.
    async fn settle(&self, temp: &Path, extension: &str) -> io::Result<String> {
        let key = Self::new_key(extension);
        let target = self.root.join(&key);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::rename(temp, &target).await?;
        Ok(key)
    }

    fn temp_path(&self) -> PathBuf {
        self.root.join("tmp").join(uuid::Uuid::new_v4().to_string())
    }

    /// Writes a streamed file, counting and hashing it; refuses it past `max` bytes.
    pub async fn receive<S, E>(&self, mut stream: S, max: u64) -> Result<Received, ReceiveError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin,
        E: std::fmt::Display,
    {
        let temp = self.temp_path();
        let written = async {
            let mut file = fs::File::create(&temp).await?;
            let mut hasher = Sha256::new();
            let mut size: u64 = 0;
            let mut head = Vec::with_capacity(HEAD_LEN);
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| ReceiveError::Body(e.to_string()))?;
                size += chunk.len() as u64;
                if size > max {
                    return Err(ReceiveError::TooLarge(max));
                }
                if head.len() < HEAD_LEN {
                    let take = (HEAD_LEN - head.len()).min(chunk.len());
                    head.extend_from_slice(&chunk[..take]);
                }
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
            }
            file.flush().await?;
            file.sync_all().await?;
            Ok((size, hasher.finalize().into(), head))
        }
        .await;
        match written {
            Ok((size, sha256, head)) => match self.settle(&temp, "").await {
                Ok(key) => Ok(Received {
                    key,
                    size,
                    sha256,
                    head,
                }),
                Err(error) => {
                    let _ = fs::remove_file(&temp).await;
                    Err(error.into())
                }
            },
            Err(error) => {
                let _ = fs::remove_file(&temp).await;
                Err(error)
            }
        }
    }

    /// Writes a file made by the server (a thumbnail, an avatar) under a new key.
    pub async fn put(&self, bytes: &[u8], extension: &str) -> io::Result<String> {
        let temp = self.temp_path();
        let written = async {
            let mut file = fs::File::create(&temp).await?;
            file.write_all(bytes).await?;
            file.flush().await?;
            file.sync_all().await
        }
        .await;
        let settled = match written {
            Ok(()) => self.settle(&temp, extension).await,
            Err(error) => Err(error),
        };
        if settled.is_err() {
            let _ = fs::remove_file(&temp).await;
        }
        settled
    }

    /// Where a key's file is; `None` for a key that is not ours (`..`, absolute paths, odd
    /// characters), so a stored key can never point outside the directory.
    pub fn path(&self, key: &str) -> Option<PathBuf> {
        valid_key(key).then(|| self.root.join(key))
    }

    pub async fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        let path = self
            .path(key)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file key"))?;
        fs::read(path).await
    }

    /// Removes files; a missing one is fine, a key that is not ours is skipped.
    pub async fn remove(&self, keys: &[String]) -> io::Result<()> {
        for key in keys {
            let Some(path) = self.path(key) else {
                continue;
            };
            match fs::remove_file(&path).await {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

/// Whether a key has the shape this store makes: relative segments of letters, digits, `.`,
/// `-` and `_`, none of them `.` or `..`.
pub fn valid_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 200 {
        return false;
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/');
    if !key.chars().all(allowed) {
        return false;
    }
    Path::new(key)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        && !key
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn chunks(parts: &[&[u8]]) -> impl Stream<Item = Result<Bytes, String>> + Unpin {
        futures_util::stream::iter(
            parts
                .iter()
                .map(|p| Ok(Bytes::copy_from_slice(p)))
                .collect::<Vec<_>>(),
        )
    }

    #[tokio::test]
    async fn receives_counts_and_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.prepare().await.unwrap();
        let got = store
            .receive(chunks(&[b"hello ", b"world"]), 100)
            .await
            .unwrap();
        assert_eq!(got.size, 11);
        assert_eq!(got.head, b"hello world");
        let expected: [u8; 32] = Sha256::digest(b"hello world").into();
        assert_eq!(got.sha256, expected);
        assert!(valid_key(&got.key), "{}", got.key);
        assert_eq!(store.read(&got.key).await.unwrap(), b"hello world");
        // Nothing stays behind in tmp/.
        let mut left = fs::read_dir(dir.path().join("tmp")).await.unwrap();
        assert!(left.next_entry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn refuses_past_the_limit_and_keeps_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.prepare().await.unwrap();
        let refused = store.receive(chunks(&[b"12345", b"678"]), 7).await;
        assert!(matches!(refused, Err(ReceiveError::TooLarge(7))));
        let exact = store.receive(chunks(&[b"1234567"]), 7).await.unwrap();
        assert_eq!(exact.size, 7);
        let mut left = fs::read_dir(dir.path().join("tmp")).await.unwrap();
        assert!(left.next_entry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_broken_body_keeps_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.prepare().await.unwrap();
        let stream = futures_util::stream::iter(vec![
            Ok(Bytes::from_static(b"part")),
            Err("connection reset".to_owned()),
        ]);
        assert!(matches!(
            store.receive(stream, 100).await,
            Err(ReceiveError::Body(_))
        ));
        let mut left = fs::read_dir(dir.path().join("tmp")).await.unwrap();
        assert!(left.next_entry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn puts_and_removes_missing_files_quietly() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.prepare().await.unwrap();
        let key = store.put(b"png bytes", "png").await.unwrap();
        assert!(key.ends_with(".png"));
        assert!(store.path(&key).unwrap().exists());
        store
            .remove(&[key.clone(), "2020/01/missing".to_owned()])
            .await
            .unwrap();
        assert!(!store.path(&key).unwrap().exists());
        store.remove(&[key]).await.unwrap();
    }

    #[test]
    fn keys_cannot_leave_the_directory() {
        let store = FileStore::new("/data/files");
        assert!(store.path("2026/10/0192.png").is_some());
        for bad in [
            "",
            "../etc/passwd",
            "/etc/passwd",
            "2026/../../x",
            "2026//x",
            "./x",
            "a\\b",
            "a b",
            "2026/10/x\0",
        ] {
            assert!(store.path(bad).is_none(), "{bad:?}");
        }
    }
}
