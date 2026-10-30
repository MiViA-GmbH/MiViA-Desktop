use std::borrow::Cow;
use std::io;
use std::net::Ipv4Addr;
use std::time::Duration;

use httparse::{EMPTY_HEADER, Request, Status};
use mivia_domain::naming::Language;
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};
use url::form_urlencoded;

const REQUEST_LIMIT: usize = 8 * 1024;
const CONNECTION_LIMIT: Duration = Duration::from_secs(5);
const ACCEPT_RETRY: Duration = Duration::from_millis(200);
const MAX_HEADERS: usize = 32;

pub(super) enum Returned {
    Code(SecretString),
    Denied,
    Failed,
}

pub(super) struct Listener {
    socket: TcpListener,
    host: String,
    state: SecretString,
    language: Language,
}

fn page_text(language: Language, finished: bool) -> &'static str {
    match (language, finished) {
        (Language::En, true) => "Sign-in finished. Return to MiViA Desktop.",
        (Language::En, false) => "Sign-in failed. Return to MiViA Desktop.",
        (Language::De, true) => "Anmeldung abgeschlossen. Zur\u{fc}ck zu MiViA Desktop.",
        (Language::De, false) => "Anmeldung fehlgeschlagen. Zur\u{fc}ck zu MiViA Desktop.",
    }
}

fn page(language: Language, finished: bool) -> String {
    let (lang, text) = (
        match language {
            Language::En => "en",
            Language::De => "de",
        },
        page_text(language, finished),
    );
    let body = format!(
        "<!doctype html><html lang=\"{lang}\"><meta charset=\"utf-8\"><title>MiViA Desktop</title><p>{text}</p></html>"
    );
    response("200 OK", "text/html; charset=utf-8", &body)
}

fn response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nContent-Security-Policy: default-src 'none'\r\nReferrer-Policy: no-referrer\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn read_head(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 1024];
    while head.len() < REQUEST_LIMIT {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        head.extend_from_slice(chunk.get(..read)?);
        if head.len() > REQUEST_LIMIT {
            return None;
        }
        let mut headers = [EMPTY_HEADER; MAX_HEADERS];
        match Request::new(&mut headers).parse(&head) {
            Ok(Status::Complete(_)) => return Some(head),
            Ok(Status::Partial) => {}
            Err(_) => return None,
        }
    }
    None
}

impl Listener {
    pub(super) async fn bind(state: SecretString, language: Language) -> io::Result<Self> {
        let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let host = format!("127.0.0.1:{}", socket.local_addr()?.port());
        Ok(Self {
            socket,
            host,
            state,
            language,
        })
    }

    pub(super) fn redirect_uri(&self) -> String {
        format!("http://{}/callback", self.host)
    }

    pub(super) fn state(&self) -> &str {
        self.state.expose_secret()
    }

    pub(super) async fn wait(self) -> Returned {
        loop {
            match self.socket.accept().await {
                Ok((stream, _)) => {
                    if let Some(returned) = self.serve(stream).await {
                        return returned;
                    }
                }
                Err(_) => sleep(ACCEPT_RETRY).await,
            }
        }
    }

    async fn serve(&self, mut stream: TcpStream) -> Option<Returned> {
        let head = timeout(CONNECTION_LIMIT, read_head(&mut stream))
            .await
            .ok()
            .flatten();
        let returned = head.as_deref().and_then(|head| self.judge(head));
        let answer = match &returned {
            Some(Returned::Code(_)) => page(self.language, true),
            Some(Returned::Denied | Returned::Failed) => page(self.language, false),
            None => response("400 Bad Request", "text/plain", "Bad request"),
        };
        let write = async {
            stream.write_all(answer.as_bytes()).await?;
            stream.shutdown().await
        };
        if timeout(CONNECTION_LIMIT, write).await.is_err() {
            tracing::debug!("signIn.pageNotSent");
        }
        returned
    }

    fn judge(&self, head: &[u8]) -> Option<Returned> {
        let mut headers = [EMPTY_HEADER; MAX_HEADERS];
        let mut request = Request::new(&mut headers);
        request.parse(head).ok()?;
        if request.method != Some("GET") {
            return None;
        }
        let mut hosts = request
            .headers
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case("host"));
        let host = hosts.next()?;
        if hosts.next().is_some() || host.value != self.host.as_bytes() {
            return None;
        }
        let (path, query) = request.path?.split_once('?')?;
        if path != "/callback" {
            return None;
        }
        self.returned(query)
    }

    fn returned(&self, query: &str) -> Option<Returned> {
        let mut state: Option<Cow<'_, str>> = None;
        let mut code = None;
        let mut error = None;
        for (name, value) in form_urlencoded::parse(query.as_bytes()) {
            match name.as_ref() {
                "state" => state = Some(value),
                "code" => code = Some(value),
                "error" => error = Some(value),
                _ => {}
            }
        }
        let expected = self.state.expose_secret().as_bytes();
        if !bool::from(expected.ct_eq(state?.as_bytes())) {
            return None;
        }
        if let Some(error) = error {
            return Some(if error == "access_denied" {
                Returned::Denied
            } else {
                Returned::Failed
            });
        }
        code.filter(|code| !code.is_empty())
            .map(|code| Returned::Code(SecretString::from(code.into_owned())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_second_host_header_is_refused() {
        let listener = Listener::bind(SecretString::from("s3cret"), Language::En)
            .await
            .unwrap();
        let host = &listener.host;
        let single = format!("GET /callback?code=c&state=s3cret HTTP/1.1\r\nHost: {host}\r\n\r\n");
        let double = format!(
            "GET /callback?code=c&state=s3cret HTTP/1.1\r\nHost: {host}\r\nHost: evil.example\r\n\r\n"
        );
        assert!(matches!(
            listener.judge(single.as_bytes()),
            Some(Returned::Code(_))
        ));
        assert!(listener.judge(double.as_bytes()).is_none());
    }
}
