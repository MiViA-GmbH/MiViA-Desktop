use std::path::PathBuf;
use std::time::Instant;

use mivia_domain::ids::BatchId;
use mivia_ports::ipc::history::HistoryFilter;
use mivia_ports::queries::Queries;
use mivia_store::{SqliteStore, StoreConfig};

use crate::common::{account, fixture, new_file, seed};
use crate::history::{SCOPE, folder, series};
use crate::reports::closed;

const FILES_PER_BATCH: usize = 10;
const CHUNK: usize = 1_000;
const BIG_MEMBERS: usize = 1_000;
const SAMPLES: usize = 200;

fn batches() -> usize {
    std::env::var("MIVIA_PERF_BATCHES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_000)
}

fn p95(mut millis: Vec<f64>) -> f64 {
    millis.sort_by(f64::total_cmp);
    millis[(millis.len() * 95).div_ceil(100) - 1]
}

async fn fill(store: &SqliteStore, count: usize) -> BatchId {
    seed(store, |tx| {
        account(tx, "a");
    })
    .await;
    let mut big = BatchId(0);
    for start in (0..count).step_by(CHUNK) {
        let end = (start + CHUNK).min(count);
        big = seed(store, move |tx| {
            let origin = folder(tx, SCOPE, &format!(r"D:\Lab{start}"));
            let mut last = BatchId(0);
            for index in start..end {
                let batch = series(tx, origin, &format!("Probe {index:07} Haerte"), "grain");
                for file in 0..FILES_PER_BATCH {
                    let name = format!("img_{index}_{file}.tif");
                    tx.source_files(SCOPE).add(&new_file(batch, &name)).unwrap();
                }
                closed(tx, SCOPE, batch);
                last = batch;
            }
            last
        })
        .await;
    }
    big
}

async fn members(store: &SqliteStore) -> BatchId {
    seed(store, |tx| {
        let origin = folder(tx, SCOPE, r"D:\Big");
        let batch = series(tx, origin, "Big series", "grain");
        for file in 0..BIG_MEMBERS {
            tx.source_files(SCOPE)
                .add(&new_file(batch, &format!("big_{file}.tif")))
                .unwrap();
        }
        closed(tx, SCOPE, batch);
        batch
    })
    .await
}

async fn page_times(store: &SqliteStore, filter: HistoryFilter) -> Vec<f64> {
    let mut times = Vec::new();
    let mut after = None;
    while times.len() < SAMPLES {
        let started = Instant::now();
        let page = store
            .list_history(SCOPE, filter.clone(), after.clone())
            .await
            .unwrap();
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    times
}

#[tokio::test]
#[ignore = "perf scenario, run by cargo xtask perf history"]
async fn history_pages_are_fast() {
    let fx = fixture(StoreConfig::production()).await;
    let count = batches();
    let started = Instant::now();
    fill(&fx.store, count).await;
    let big = members(&fx.store).await;
    let seeded_s = started.elapsed().as_secs_f64();
    let search = HistoryFilter {
        q: Some("Haerte".to_owned()),
        ..HistoryFilter::default()
    };
    let plain = page_times(&fx.store, HistoryFilter::default()).await;
    let searched = page_times(&fx.store, search).await;
    let mut items = Vec::new();
    let mut after = None;
    while items.len() < BIG_MEMBERS {
        let begun = Instant::now();
        let page = fx.store.list_batch_items(SCOPE, big, after).await.unwrap();
        items.push(begun.elapsed().as_secs_f64() * 1000.0);
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    let report = format!(
        "{{\"batches\":{count},\"seededS\":{seeded_s:.1},\"pagePlain\":{:.3},\"pageSearch\":{:.3},\"items\":{:.3}}}",
        p95(plain),
        p95(searched),
        p95(items)
    );
    std::fs::write(
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("perf-history.json"),
        report,
    )
    .unwrap();
}
