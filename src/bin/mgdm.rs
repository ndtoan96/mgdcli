use std::{path::PathBuf, time::Duration};

use clap::{ArgAction, Args, Parser};
use mangadex::{ChapterDownloadRequest, ChapterDownloader, GetChapters, MangaQuery, Volume};
use tower::{Service, ServiceBuilder, ServiceExt};

#[derive(Debug, Parser)]
#[command(
    name = "mdgm",
    version,
    author,
    about = "CLI tool to download manga from mangadex"
)]
struct Arguments {
    #[arg(help = "manga id or url")]
    manga: String,
    #[arg(short, long, default_value_t= String::from("en"), help="translation language" )]
    language: String,
    #[arg(short, long, help = "translation group")]
    groups: Vec<String>,
    #[arg(short, long, group = "range")]
    chapters: Vec<f32>,
    #[arg(short, long, group = "range")]
    volumes: Vec<f32>,
    #[command(flatten)]
    chapter_range: ChapterRange,
    #[command(flatten)]
    volume_range: VolumeRange,
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

#[derive(Debug, Clone, Args)]
#[group(
    id = "chapter_range",
    multiple = true,
    conflicts_with = "range",
    conflicts_with = "volume_range"
)]
struct ChapterRange {
    #[arg(long)]
    min_chapter: Option<f32>,
    #[arg(long)]
    max_chapter: Option<f32>,
}

#[derive(Debug, Clone, Args)]
#[group(id = "volume_range", multiple = true, conflicts_with = "range")]
struct VolumeRange {
    #[arg(long)]
    min_volume: Option<f32>,
    #[arg(long)]
    max_volume: Option<f32>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Arguments::parse();

    let query = if args.manga.contains("mangadex.org") {
        MangaQuery::from_url(&args.manga)?
    } else {
        MangaQuery::new(&args.manga)
    };

    let manga_volumes = query.execute().await?;

    let chapters = if !args.volumes.is_empty() {
        let filtered_volumes: Vec<&Volume> = manga_volumes
            .iter()
            .filter(|x| args.volumes.contains(&x.volume().unwrap_or(f32::INFINITY)))
            .collect();
        filtered_volumes.get_chapters()
    } else if !args.chapters.is_empty() {
        (&manga_volumes)
            .get_chapters()
            .into_iter()
            .filter(|c| {
                args.chapters
                    .contains(&c.chapter().unwrap_or(f32::INFINITY))
            })
            .collect()
    } else if args.chapter_range.min_chapter.is_some() || args.chapter_range.max_chapter.is_some() {
        let min_chap = args.chapter_range.min_chapter.unwrap_or(f32::NEG_INFINITY);
        let max_chap = args.chapter_range.max_chapter.unwrap_or(f32::INFINITY);
        (&manga_volumes)
            .get_chapters()
            .into_iter()
            .filter(|c| {
                let c = c.chapter().unwrap_or(-1.0);
                c >= min_chap && c <= max_chap
            })
            .collect()
    } else {
        let min_vol = args.volume_range.min_volume.unwrap_or(f32::NEG_INFINITY);
        let max_vol = args.volume_range.max_volume.unwrap_or(f32::INFINITY);
        manga_volumes
            .iter()
            .filter(|v| {
                let v = v.volume().unwrap_or(-1.0);
                v >= min_vol && v <= max_vol
            })
            .get_chapters()
    };

    let mut download_service = ServiceBuilder::new()
        .rate_limit(3, Duration::from_secs(6))
        .service(ChapterDownloader);

    let width = chapters
        .last()
        .and_then(|c| c.chapter().as_ref())
        .map(|&c| c.log10().floor() as usize)
        .unwrap_or(0)
        + 1;

    for chapter in chapters {
        let chapter_name = match chapter.chapter() {
            Some(c) => format!("chapter_{c:0width$}", width = width),
            None => String::from("chapter_none"),
        };

        println!("Download {chapter_name}");

        download_service
            .ready()
            .await?
            .call(
                ChapterDownloadRequest::new(chapter.id())
                    .data_saver(args.data_saver)
                    .path(args.path.join(&chapter_name)),
            )
            .await?;
    }

    Ok(())
}
