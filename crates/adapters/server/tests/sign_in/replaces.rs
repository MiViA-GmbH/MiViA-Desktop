use mivia_domain::naming::Language;
use serde_json::json;
use wiremock::MockServer;

use super::{Return, api, serve_token_side};

#[tokio::test]
async fn a_later_sign_in_replaces_the_key_issued_to_that_account() {
    let mock = MockServer::start().await;
    serve_token_side(&mock).await;
    let issuing = api(&mock);
    let api = api(&mock);
    let sign_in = api
        .start_browser(Language::En, "MiViA Desktop on LAB".to_owned())
        .await
        .unwrap();
    let back = Return::of(&sign_in);
    let finish = tokio::spawn(async move { sign_in.finish(&api).await });
    back.send(back.valid().as_bytes()).await;
    let signed_in = finish.await.unwrap().unwrap();
    let issued = issuing
        .issue_key(signed_in, Some("key-0".to_owned()))
        .await
        .unwrap();
    assert_eq!(issued.id, "key-1");

    let requests = mock.received_requests().await.unwrap();
    let issue = requests
        .iter()
        .find(|request| request.url.path() == "/api/api-keys/issue")
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&issue.body).unwrap();
    assert_eq!(
        body,
        json!({ "name": "MiViA Desktop on LAB", "replaces": "key-0" })
    );
}
