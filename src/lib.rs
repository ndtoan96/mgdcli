mod service;
mod query;

pub use service::{ChapterDownloadRequest, ChapterDownloader};
pub use query::{Chapter, GetChapters, MangaQuery, Volume};

#[derive(Debug, thiserror::Error)]
pub enum MangadexError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error(transparent)]
    DeserializeError(#[from] serde_json::Error),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("invalid url '{0}'")]
    UrlParseError(String),
}