use std::sync::Arc;

use mivia_domain::ids::Timestamp;
use mivia_domain::path::WinPath;
use mivia_ports::clock::Clock;
use mivia_ports::fs::{FileSystem, FsError};
use mivia_ports::id_gen::IdGen;
use mivia_ports::server::{ServerApi, ServerError, ServerJobStatus, SubmitRequest};
use mivia_testkit::clock::ManualClock;
use mivia_testkit::contract::server::{grain, upload_request};
use mivia_testkit::fs::{FsOp, InMemoryFs};
use mivia_testkit::id_gen::SeqIdGen;
use mivia_testkit::server::{FakeServerApi, Fault, Op};
use uuid::Uuid;

fn path(text: &str) -> WinPath {
    WinPath::parse(text).unwrap()
}

#[tokio::test]
async fn manual_clock_tracks_waiters_and_jumps_backwards() {
    let clock = Arc::new(ManualClock::new(Timestamp(100)));
    let sleeper = {
        let clock = Arc::clone(&clock);
        tokio::spawn(async move { clock.sleep_until(Timestamp(200)).await })
    };
    tokio::task::yield_now().await;
    assert_eq!(clock.pending_waiters(), [Timestamp(200)]);
    clock.set(Timestamp(50));
    assert_eq!(clock.now(), Timestamp(50));
    assert!(!sleeper.is_finished());
    clock.advance(150);
    sleeper.await.unwrap();
    assert_eq!(clock.pending_waiters(), []);
}

#[test]
fn seq_ids_count_up_from_one() {
    let ids = SeqIdGen::default();
    assert_eq!(ids.uuid().as_u128(), 1);
    assert_eq!(ids.uuid().as_u128(), 2);
}

#[tokio::test]
async fn memory_fs_models_windows_names_and_faults() {
    let fs = InMemoryFs::windows();
    fs.put(&path(r"D:\Lab\a.tif"), b"abc");
    assert!(fs.stat(&path(r"d:\LAB\A.TIF. ")).await.is_ok());
    fs.grow(&path(r"D:\Lab\a.tif"), b"de");
    fs.set_mtime(&path(r"D:\Lab\a.tif"), Timestamp(9));
    let stat = fs.stat(&path(r"D:\Lab\a.tif")).await.unwrap();
    assert_eq!((stat.size, stat.mtime), (5, Timestamp(9)));
    assert_eq!(
        fs.walk(&path(r"D:\Lab\a.tif")).await,
        Err(FsError::NotADirectory)
    );
    fs.fail_next(FsOp::Stat, FsError::AccessDenied);
    assert_eq!(
        fs.stat(&path(r"D:\Lab\a.tif")).await,
        Err(FsError::AccessDenied)
    );
    fs.set_free_space(1);
    assert_eq!(
        fs.write_atomic(&path(r"D:\Lab\b.pdf"), b"xy").await,
        Err(FsError::DiskFull)
    );
    assert_eq!(
        fs.write_atomic(&path(r"D:\Missing\b.pdf"), b"").await,
        Err(FsError::NotFound)
    );
    assert_eq!(
        fs.write_atomic(&path(r"D:\Lab\lpt1.txt"), b"").await,
        Err(FsError::InvalidName)
    );
    fs.vanish(&path(r"D:\Lab\a.tif"));
    assert_eq!(
        fs.stat(&path(r"D:\Lab\a.tif")).await,
        Err(FsError::NotFound)
    );
}

#[tokio::test]
async fn fake_server_injects_faults_and_journals_requests() {
    let server = FakeServerApi::default();
    server.fail_next(Op::Version, Fault::Before(ServerError::Unreachable));
    assert_eq!(server.version().await, Err(ServerError::Unreachable));
    assert_eq!(server.version().await.unwrap().version, "2.45.0");
    server.with_sim(|sim| sim.max_upload_bytes = 1);
    let upload = upload_request("big.tif", &[0; 2]);
    assert_eq!(
        server.upload(upload).await,
        Err(ServerError::TooLarge { by_proxy: false })
    );
    server.set_down(
        &[Op::Upload],
        Some(&ServerError::Unavailable { status: 503 }),
    );
    let upload = upload_request("small.tif", &[0]);
    assert_eq!(
        server.upload(upload).await,
        Err(ServerError::Unavailable { status: 503 })
    );
    let ops: Vec<Op> = server.journal().into_iter().map(|(op, _)| op).collect();
    assert_eq!(ops, [Op::Version, Op::Version, Op::Upload, Op::Upload]);
}

#[tokio::test]
async fn partial_submit_creates_some_jobs_and_fails_the_last() {
    let server = FakeServerApi::default();
    let mut images = Vec::new();
    for bytes in [b"a", b"b", b"c"] {
        images.push(
            server
                .upload(upload_request("x.png", bytes))
                .await
                .unwrap()
                .image_id,
        );
    }
    server.fail_next(Op::Submit, Fault::PartialSubmit { created: 2 });
    let request = SubmitRequest {
        key: Uuid::from_u128(1),
        choice: grain(),
        image_ids: images,
        force: false,
    };
    assert_eq!(
        server.submit(request).await,
        Err(ServerError::Unavailable { status: 500 })
    );
    let statuses: Vec<ServerJobStatus> =
        server.with_sim(|sim| sim.jobs().iter().map(|job| job.status.clone()).collect());
    assert_eq!(
        statuses,
        [ServerJobStatus::Pending, ServerJobStatus::Failed]
    );
}
