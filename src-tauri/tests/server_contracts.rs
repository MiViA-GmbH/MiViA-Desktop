use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mivia_domain::ids::AccountId;
use mivia_ports::auth::ServerOrigin;
use mivia_ports::server::ServerApi;
use mivia_server::{HttpServerApi, build_clients};
use mivia_stub::Stub;
use mivia_testkit::auth::FakeAuthorizer;
use mivia_testkit::contract::server::ServerHarness;
use mivia_testkit::server::ServerSim;
use mivia_testkit::server_contract_tests;

const KEY: &str = "stub-contract-key";

struct StubHarness {
    stub: Stub,
    api: Arc<HttpServerApi>,
}

impl StubHarness {
    fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let uri = format!("http://{}", listener.local_addr().unwrap());
        let stub = Stub::new(KEY);
        let serving = stub.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                serving.serve(listener).await.unwrap();
            });
        });
        let clients = build_clients("MiViA-Desktop/0.0.0 (Windows test; stable)").unwrap();
        let auth = FakeAuthorizer::for_origin(AccountId(1), &uri, KEY);
        let api = HttpServerApi::new(
            ServerOrigin(uri),
            AccountId(1),
            Arc::new(ArcSwap::from_pointee(clients)),
            Arc::new(auth),
        );
        Self {
            stub,
            api: Arc::new(api),
        }
    }
}

#[async_trait]
impl ServerHarness for StubHarness {
    fn sut(&self) -> Arc<dyn ServerApi> {
        Arc::clone(&self.api) as _
    }

    async fn finish_jobs(&self) {
        self.stub.engine.with_sim(ServerSim::finish_all);
    }

    async fn seed_results(&self, job_id: &str, results: &str, image: (&str, &[u8])) {
        self.stub.engine.with_sim(|sim| {
            sim.results.insert(job_id.to_owned(), results.to_owned());
            sim.result_images
                .insert((job_id.to_owned(), image.0.to_owned()), image.1.to_vec());
        });
    }
}

server_contract_tests!(StubHarness::new());
