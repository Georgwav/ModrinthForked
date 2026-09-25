//! Threadrinth: downloads into the content store for files that only come
//! with a SHA-1 hash, like CurseForge and Feed the Beast files.

use crate::state::content_store::{ContentStore, GetFileResult};
use crate::util::fetch::{self, DownloadMeta, FetchProgressFn, FetchSemaphore};

impl ContentStore {
    /// Downloads a file checked against its SHA-1 (when known) and keeps it
    /// in the store under its SHA-512, like other downloads.
    pub(crate) async fn download_file_by_sha1(
        &self,
        mirrors: &[&str],
        sha1: Option<&str>,
        download_meta: Option<&DownloadMeta>,
        semaphore: &FetchSemaphore,
        progress: Option<&mut FetchProgressFn<'_>>,
    ) -> crate::Result<GetFileResult> {
        let download = fetch::fetch_file_mirrors_in(
            mirrors,
            sha1,
            download_meta,
            None,
            semaphore,
            &self.pool,
            progress,
            Some(&self.staging),
        )
        .await?;
        let sources = mirrors
            .iter()
            .filter(|source| {
                url::Url::parse(source).is_ok_and(|url| {
                    url.scheme() == "https"
                        && url.username().is_empty()
                        && url.password().is_none()
                        && url.query().is_none()
                })
            })
            .map(|source| source.to_string())
            .collect::<Vec<_>>();
        let stored_file = self
            .save_staged_file(download.into_staged()?, &sources)
            .await?;
        Ok(GetFileResult {
            stored_file,
            reused: false,
        })
    }
}
