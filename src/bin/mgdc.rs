use std::path::PathBuf;

use anyhow::Ok;
use clap::{Parser, ArgAction};
use mangadex::{ChapterDownloadRequest, ChapterDownloader};
use tower::Service;

#[derive(Debug, Parser)]
#[command(
    name = "mdgc",
    version,
    author,
    about = "CLI tool to download chapter from mangadex"
)]
struct Arguments {
    #[arg(help = "Chapter id or url")]
    chapter: String,
    #[arg(short, long, default_value = ".", help = "destination folder")]
    path: PathBuf,
    #[arg(
        short = 'r',
        long = "raw",
        action=ArgAction::SetFalse,
        default_value_t = true,
        help = "download uncompressed images"
    )]
    data_saver: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Arguments::parse();
    let req = if args.chapter.contains("mangadex.org") {
        ChapterDownloadRequest::from_url(&args.chapter)?
    } else {
        ChapterDownloadRequest::new(&args.chapter)
    };
    let req = req.path(&args.path).data_saver(args.data_saver);

    let mut download_service = ChapterDownloader;
    download_service.call(req).await?;
    Ok(())
}
