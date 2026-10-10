mod api;
mod control;
mod errors;

use std::sync::Arc;

use axum::Router;
use mivia_testkit::server::FakeServerApi;
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct Stub {
    pub engine: Arc<FakeServerApi>,
    pub key: String,
}

impl Stub {
    pub fn new(key: &str) -> Self {
        Self {
            engine: Arc::new(FakeServerApi::default()),
            key: key.to_owned(),
        }
    }

    pub fn router(&self) -> Router {
        api::routes()
            .merge(control::routes())
            .with_state(self.clone())
    }

    pub async fn serve(&self, listener: TcpListener) -> std::io::Result<()> {
        axum::serve(listener, self.router()).await
    }
}
