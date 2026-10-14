mod api;
mod control;
mod diagnostics;
mod errors;
mod reports;
mod results;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use mivia_testkit::server::FakeServerApi;
use parking_lot::Mutex;
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct Stub {
    pub engine: Arc<FakeServerApi>,
    pub key: String,
    pub scheme: &'static str,
    files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    served: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Stub {
    pub fn new(key: &str) -> Self {
        Self {
            engine: Arc::new(FakeServerApi::default()),
            key: key.to_owned(),
            scheme: "http",
            files: Arc::default(),
            served: Arc::default(),
        }
    }

    pub fn router(&self) -> Router {
        api::routes()
            .merge(control::routes())
            .merge(reports::routes())
            .merge(results::routes())
            .merge(diagnostics::routes())
            .with_state(self.clone())
    }

    pub async fn serve(&self, listener: TcpListener) -> std::io::Result<()> {
        axum::serve(listener, self.router()).await
    }
}
