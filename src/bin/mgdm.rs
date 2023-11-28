use std::fs;
use std::io::Read;
use std::io::Write;
use std::{path::PathBuf, time::Duration};
use zip::{write::FileOptions, ZipWriter};

use clap::{ArgAction, Args, Parser};
use mangadex::{ChapterDownloadRequest, ChapterDownloader, GetChapters, MangaQuery, Volume};
use std::path::Path;
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
    #[arg(long, help = "make cbz file")]
    make_cbz: bool,
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

    let mut query = if args.manga.contains("mangadex.org") {
        MangaQuery::from_url(&args.manga)?
    } else {
        MangaQuery::new(&args.manga)
    };

    query = query.language(&args.language);
    for group in &args.groups {
        query = query.group(group);
    }

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
        .rate_limit(1, Duration::from_secs(2))
        .service(ChapterDownloader);

    let width = chapters
        .last()
        .and_then(|c| c.chapter().as_ref())
        .map(|&c| c.log10().floor() as usize)
        .unwrap_or(0)
        + 1;

    let mut downloaded_paths = Vec::new();
    for chapter in chapters {
        let chapter_name = match chapter.chapter() {
            Some(c) => format!("chapter_{c:0width$}", width = width),
            None => String::from("chapter_none"),
        };

        println!("Download {chapter_name}");

        let download_path = args.path.join(&chapter_name);
        download_service
            .ready()
            .await?
            .call(
                ChapterDownloadRequest::new(chapter.id())
                    .data_saver(args.data_saver)
                    .path(&download_path),
            )
            .await?;
        downloaded_paths.push(download_path);
    }

    if args.make_cbz {
        println!("Making cbz file...");
        make_cbz(downloaded_paths)?;
        println!("Done.");
    }

    Ok(())
}

fn make_cbz<T1, T2>(paths: T1) -> Result<(), std::io::Error>
where
    T1: IntoIterator<Item = T2>,
    T2: AsRef<Path>,
{
    let mut new_names = Vec::new();
    let mut parent = None;
    for (i, path) in paths.into_iter().enumerate() {
        let path = path.as_ref();
        parent = Some(path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let current_name = path.file_name().unwrap();
        let new_name = format!("{:05}_{}", i, current_name.to_string_lossy());
        let new_path = path.with_file_name(&new_name);
        fs::rename(path, &new_path)?;
        new_names.push(new_name);
    }

    if new_names.is_empty() {
        return Ok(());
    }

    let parent = parent.unwrap();

    // zip all folder and create cbz file
    let file = fs::File::create(parent.join("manga.cbz"))?;
    let mut writer = ZipWriter::new(file);
    let mut buf = Vec::new();
    for name in new_names.iter() {
        // writer.add_directory(name, FileOptions::default())?;
        for entry in fs::read_dir(parent.join(name))? {
            let file_path = entry?.path();
            if file_path.is_file() {
                writer.start_file(
                    format!(
                        "{}/{}",
                        name,
                        file_path.file_name().unwrap().to_string_lossy()
                    ),
                    FileOptions::default(),
                )?;

                fs::File::open(file_path)?.read_to_end(&mut buf)?;
                writer.write_all(&buf)?;
                buf.clear();
            }
        }
        // The folder has been added to cbz, delete it
        let _ = fs::remove_dir_all(parent.join(name));
    }

    Ok(())
}
