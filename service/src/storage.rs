//! Object store abstraction for uploaded files.
//!
//! Provides a `LocalFileStore` implementation backed by the local filesystem
//! for development and MVP use. A production S3-compatible implementation
//! can be swapped in behind the same trait.

use std::path::PathBuf;

use async_trait::async_trait;

/// Trait for storing and retrieving uploaded files.
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Store bytes under `key`. `content_type` is persisted alongside the data.
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<(), anyhow::Error>;

    /// Retrieve bytes and content-type for `key`, or `None` if it doesn't exist.
    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, String)>, anyhow::Error>;

    /// Return the URL path that serves this file (e.g. `/api/v1/uploads/{key}`).
    fn url_path(&self, key: &str) -> String;
}

/// Local filesystem object store for development / MVP deployments.
///
/// Files are written under `base_dir/{key}`.  The MIME type is persisted in a
/// sidecar file at `{path}.ct` so the serve handler can echo the correct
/// `Content-Type` header without inspecting the bytes.
pub struct LocalFileStore {
    base_dir: PathBuf,
    serve_prefix: String,
}

impl LocalFileStore {
    /// Create a new store rooted at `base_dir`, serving paths under
    /// `serve_prefix` (e.g. `"/api/v1/uploads"`).
    ///
    /// Creates `base_dir` if it does not already exist.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the directory cannot be created.
    pub fn new(base_dir: PathBuf, serve_prefix: &str) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self {
            base_dir,
            serve_prefix: serve_prefix.to_string(),
        })
    }
}

#[async_trait]
impl ObjectStore for LocalFileStore {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<(), anyhow::Error> {
        let path = self.base_dir.join(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        tokio::fs::write(format!("{}.ct", path.display()), content_type).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, String)>, anyhow::Error> {
        let path = self.base_dir.join(key);
        if !path.exists() {
            return Ok(None);
        }
        let data = tokio::fs::read(&path).await?;
        let ct = tokio::fs::read_to_string(format!("{}.ct", path.display()))
            .await
            .unwrap_or_else(|_| "application/octet-stream".to_string());
        Ok(Some((data, ct)))
    }

    fn url_path(&self, key: &str) -> String {
        format!("{}/{}", self.serve_prefix, key)
    }
}
