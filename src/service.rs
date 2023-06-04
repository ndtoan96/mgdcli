use super::MangadexError;
use futures::Future;
use reqwest::IntoUrl;
use serde::Deserialize;
use std::fmt::Debug;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use tower::Service;
use tracing::debug;
use tracing::debug_span;
use tracing::instrument;

#[derive(Debug)]
pub struct ChapterDownloader;

#[derive(Debug)]
pub struct ChapterDownloadRequest {
    pub(crate) id: String,
    pub(crate) data_saver: bool,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterData {
    base_url: String,
    chapter: ChapterDownloadData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDownloadData {
    hash: String,
    data: Vec<String>,
    data_saver: Vec<String>,
}

impl ChapterData {
    pub async fn new(id: &str) -> Result<Self, MangadexError> {
        Ok(serde_json::from_slice(
            &reqwest::get(format!("https://api.mangadex.org/at-home/server/{id}"))
                .await?
                .error_for_status()?
                .bytes()
                .await?,
        )?)
    }
}

impl ChapterDownloadRequest {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            data_saver: true,
            path: PathBuf::from("."),
        }
    }

    pub fn from_url(url: impl IntoUrl + Clone + ToString) -> Result<Self, MangadexError> {
        let url = url
            .clone()
            .into_url()
            .map_err(|_e| MangadexError::UrlParseError(url.to_string()))?;
        if !url.domain().is_some_and(|x| x == "mangadex.org") {
            return Err(MangadexError::UrlParseError(url.to_string()));
        }
        if let Some(mut segments) = url.path_segments() {
            if segments.next().is_some_and(|x| x == "chapter") {
                if let Some(id) = segments.next() {
                    Ok(Self::new(id))
                } else {
                    Err(MangadexError::UrlParseError(url.to_string()))
                }
            } else {
                Err(MangadexError::UrlParseError(url.to_string()))
            }
        } else {
            Err(MangadexError::UrlParseError(url.to_string()))
        }
    }

    pub fn data_saver(mut self, data_saver: bool) -> Self {
        self.data_saver = data_saver;
        self
    }

    pub fn path(mut self, path: impl AsRef<Path>) -> Self {
        self.path = path.as_ref().to_path_buf();
        self
    }
}

impl Service<ChapterDownloadRequest> for ChapterDownloader {
    type Response = ();
    type Error = MangadexError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ChapterDownloadRequest) -> Self::Future {
        let span = debug_span!("chapter_downloader");
        let fut = async move {
            let _enter = span.enter();
            debug!(?req);
            let chapter_data = ChapterData::new(&req.id).await?;
            download_chapter(&chapter_data, &req.path, req.data_saver).await?;
            Ok(())
        };

        Box::pin(fut)
    }
}

#[instrument(skip(chapter))]
async fn download_chapter(
    chapter: &ChapterData,
    path: impl AsRef<Path> + Debug,
    data_saver: bool,
) -> Result<(), MangadexError> {
    async fn download_one(url: String, file: PathBuf) -> Result<(), MangadexError> {
        debug!("Download {}", file.display());
        let bytes = reqwest::get(url).await?.bytes().await?;
        fs::write(file, &bytes)?;
        Ok(())
    }

    let path = path.as_ref();
    fs::create_dir_all(path)?;
    let width = chapter.chapter.data.len().checked_ilog10().unwrap_or(0) + 1;
    let mut futures = Vec::new();
    let pages = if data_saver {
        chapter.chapter.data_saver.iter().enumerate()
    } else {
        chapter.chapter.data.iter().enumerate()
    };
    let quality = if data_saver { "data-saver" } else { "data" };
    for (i, x) in pages {
        let url = format!(
            "{}/{}/{}/{}",
            chapter.base_url, quality, chapter.chapter.hash, x
        );
        let ext = if x.contains(".png") { ".png" } else { ".jpg" };
        futures.push(download_one(
            url,
            path.join(format!("page_{i:0width$}{ext}", width = width as usize)),
        ));
    }
    if let Some(e) = futures::future::join_all(futures)
        .await
        .into_iter()
        .find(|x| x.is_err())
    {
        e
    } else {
        Ok(())
    }
}
