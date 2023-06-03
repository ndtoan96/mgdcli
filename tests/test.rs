use mangadex;
use std::time::{self, Duration};
use tower::{Service, ServiceBuilder, ServiceExt};

#[tokio::test]
async fn test_limit_download_speed() {
    tracing_subscriber::fmt::init();
    let tmpdir = tempfile::tempdir().unwrap();
    let mut downloader = ServiceBuilder::new()
        .rate_limit(1, Duration::from_secs(5))
        .service(mangadex::ChapterDownloader);
    let ids = vec![
        "e5c1c16c-ec06-47d1-970c-b71499d48833",
        "dbe91557-6bb6-4fe9-a17c-7941313847f9",
    ];

    let clock = time::Instant::now();
    for id in ids {
        let req = mangadex::ChapterDownloadRequest::new(id)
            .data_saver(true)
            .path(tmpdir.path().join(id));
        downloader.ready().await.unwrap().call(req).await.unwrap();
    }
    assert!(clock.elapsed() > Duration::from_secs(5));
}

#[tokio::test]
async fn test_chapter_download_service() {
    tracing_subscriber::fmt::init();
    let tmpdir = tempfile::tempdir().unwrap();
    let mut downloader = mangadex::ChapterDownloader;
    let req = mangadex::ChapterDownloadRequest::new("af456519-3791-47c3-af8a-23ed894b5dd8")
        .data_saver(true)
        .path(tmpdir.path());
    downloader
        .ready()
        .await
        .unwrap()
        .call(req)
        .await
        .expect("Some error");

    assert!(tmpdir.path().join("page_000.jpg").exists());
}
