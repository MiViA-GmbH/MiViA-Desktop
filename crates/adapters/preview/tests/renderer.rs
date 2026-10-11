use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use image::{Rgb, RgbImage};
use mivia_domain::path::WinPath;
use mivia_ports::preview::{PreviewError, PreviewRenderer, Unavailable};
use mivia_preview::{Background, Renderer};

fn win(path: &std::path::Path) -> WinPath {
    WinPath::parse(&path.to_string_lossy()).unwrap()
}

fn setup(background: Background) -> (tempfile::TempDir, Renderer, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("previews");
    let input = dir.path().join("in.png");
    RgbImage::from_pixel(900, 600, Rgb([10, 200, 30]))
        .save(&input)
        .unwrap();
    let renderer = Renderer::new(&win(&root), background).unwrap();
    (dir, renderer, input)
}

struct Guard(Arc<AtomicU32>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn idle() -> Background {
    Arc::new(|| Box::new(()))
}

#[tokio::test]
async fn renders_through_the_port_and_cleans_on_the_first_use() {
    let (dir, renderer, input) = setup(idle());
    let root = dir.path().join("previews");
    std::fs::create_dir_all(root.join("v0")).unwrap();
    std::fs::create_dir_all(root.join("tmp")).unwrap();
    std::fs::write(root.join("tmp").join("download.png"), [1]).unwrap();
    let out = renderer.render(&win(&input)).await.unwrap();
    assert_eq!((out.width, out.height), (900, 600));
    assert!(out.view.is_none());
    assert!(PathBuf::from(out.display.to_string()).is_file());
    assert!(!root.join("v0").exists());
    assert!(!root.join("tmp").join("download.png").exists());
    assert_eq!(renderer.tmp_dir(), win(&root.join("tmp")));
}

#[tokio::test]
async fn the_background_hook_wraps_every_decode() {
    let started = Arc::new(AtomicU32::new(0));
    let ended = Arc::new(AtomicU32::new(0));
    let (s, e) = (Arc::clone(&started), Arc::clone(&ended));
    let hook: Background = Arc::new(move || {
        s.fetch_add(1, Ordering::SeqCst);
        Box::new(Guard(Arc::clone(&e)))
    });
    let (_dir, renderer, input) = setup(hook);
    renderer.render(&win(&input)).await.unwrap();
    assert_eq!(
        (started.load(Ordering::SeqCst), ended.load(Ordering::SeqCst)),
        (1, 1)
    );
}

#[tokio::test]
async fn closing_the_preview_cancels_queued_renders_but_not_the_running_one() {
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = Arc::new(parking_lot::Mutex::new(Some(gate)));
    let hook: Background = Arc::new(move || {
        if let Some(wait) = gate.lock().take() {
            wait.recv().unwrap();
        }
        Box::new(())
    });
    let (dir, renderer, input) = setup(hook);
    let renderer = Arc::new(renderer);
    let spawn = |path: PathBuf| {
        let renderer = Arc::clone(&renderer);
        tokio::spawn(async move { renderer.render(&win(&path)).await })
    };
    let second_input = dir.path().join("second.png");
    RgbImage::from_pixel(400, 300, Rgb([1, 2, 3]))
        .save(&second_input)
        .unwrap();
    let running = spawn(input.clone());
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let queued = spawn(second_input);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    renderer.cancel_queued();
    release.send(()).unwrap();
    assert!(running.await.unwrap().is_ok());
    assert_eq!(queued.await.unwrap(), Err(PreviewError::Cancelled));
    assert!(renderer.render(&win(&input)).await.is_ok());
}

#[tokio::test]
async fn unavailable_files_come_back_as_a_reason() {
    let (dir, renderer, _input) = setup(idle());
    let missing = win(&dir.path().join("gone.png"));
    assert_eq!(
        renderer.render(&missing).await,
        Err(PreviewError::Unavailable(Unavailable::Missing))
    );
}
