//! Selectable request transport for independently deployable application modules.
//! Request construction, credentials and response decoding stay with the typed
//! client. There is no fallback or replay after a transport has been selected.
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

/// Trusted caller context installed by an ingress boundary. It is carried in
/// request extensions across spawned MCP handlers, never accepted from JSON or
/// from a caller-selected forwarding header.
#[derive(Clone)]
pub struct Caller {
    pub peer: std::net::SocketAddr,
    pub user_agent: Option<reqwest::header::HeaderValue>,
}

tokio::task_local! {
    static CALLER: Caller;
}

pub async fn with_caller<F: Future>(caller: Caller, future: F) -> F::Output {
    CALLER.scope(caller, future).await
}

pub fn current_caller() -> Option<Caller> {
    CALLER.try_with(Clone::clone).ok()
}

pub type TransportFuture =
    Pin<Box<dyn Future<Output = Result<reqwest::Response, TransportError>> + Send>>;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("module request timed out; completion is unknown")]
    Timeout,
    #[error("module transport unavailable: {0}")]
    Unavailable(String),
}

pub trait RequestTransport: Send + Sync {
    fn execute(&self, request: reqwest::Request) -> TransportFuture;
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    transport: Option<Arc<dyn RequestTransport>>,
    timeout: Duration,
}

impl Client {
    pub fn new(http: reqwest::Client, timeout: Duration) -> Self {
        Self {
            http,
            transport: None,
            timeout,
        }
    }

    pub fn with_transport(mut self, transport: Arc<dyn RequestTransport>) -> Self {
        self.transport = Some(transport);
        self
    }

    pub fn get(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.wrap(self.http.get(url))
    }

    pub fn post(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.wrap(self.http.post(url))
    }

    fn wrap(&self, request: reqwest::RequestBuilder) -> RequestBuilder {
        RequestBuilder {
            request,
            client: self.clone(),
        }
    }
}

pub struct RequestBuilder {
    request: reqwest::RequestBuilder,
    client: Client,
}

impl RequestBuilder {
    pub fn json<T: serde::Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.request = self.request.json(value);
        self
    }

    pub fn query<T: serde::Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.request = self.request.query(value);
        self
    }

    pub fn bearer_auth(mut self, token: impl std::fmt::Display) -> Self {
        self.request = self.request.bearer_auth(token);
        self
    }

    pub fn header(mut self, name: impl AsRef<str>, value: impl ToString) -> Self {
        self.request = self.request.header(name.as_ref(), value.to_string());
        self
    }

    pub async fn send(self) -> Result<reqwest::Response, TransportError> {
        let mut request = self.request.build()?;
        let timeout = request.timeout().copied().unwrap_or(self.client.timeout);
        // Preserve the bound even if the caller's HTTP client was constructed
        // without a timeout; embedded and HTTP requests share this policy.
        *request.timeout_mut() = Some(timeout);
        match self.client.transport {
            Some(transport) => tokio::time::timeout(timeout, transport.execute(request))
                .await
                .map_err(|_| TransportError::Timeout)?,
            None => Ok(self.client.http.execute(request).await?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Capture {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }
    impl RequestTransport for Capture {
        fn execute(&self, request: reqwest::Request) -> TransportFuture {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let fail = self.fail;
            Box::pin(async move {
                assert_eq!(request.method(), reqwest::Method::POST);
                assert_eq!(request.url().query(), Some("network=devnet"));
                assert_eq!(request.headers()["authorization"], "Bearer test-credential");
                assert_eq!(request.headers()["content-type"], "application/json");
                assert_eq!(
                    request.body().and_then(reqwest::Body::as_bytes),
                    Some(br#"{"value":7}"#.as_slice())
                );
                if fail {
                    return Err(TransportError::Unavailable("fixture failure".into()));
                }
                let response = http::Response::builder()
                    .status(409)
                    .body(Vec::from(b"{\"error\":\"conflict\"}".as_slice()))
                    .expect("valid fixture response");
                Ok(reqwest::Response::from(response))
            })
        }
    }

    async fn request(fail: bool) -> (Result<reqwest::Response, TransportError>, usize) {
        let calls = Arc::new(AtomicUsize::new(0));
        let client = Client::new(reqwest::Client::new(), Duration::from_secs(1)).with_transport(
            Arc::new(Capture {
                calls: calls.clone(),
                fail,
            }),
        );
        let result = client
            .post("https://unreachable.invalid/test")
            .query(&[("network", "devnet")])
            .bearer_auth("test-credential")
            .json(&serde_json::json!({"value": 7}))
            .send()
            .await;
        (result, calls.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn preserves_request_credentials_payload_and_failure_status() {
        let (result, calls) = request(false).await;
        let response = result.expect("transport response");
        assert_eq!(calls, 1);
        assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"],
            "conflict"
        );
    }

    #[tokio::test]
    async fn a_failed_selected_transport_is_never_retried_or_sent_over_http() {
        let (result, calls) = request(true).await;
        assert!(matches!(result, Err(TransportError::Unavailable(_))));
        assert_eq!(calls, 1);
    }
    struct PendingWrite {
        calls: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
    }
    struct ObserveCancellation(Arc<AtomicUsize>);
    impl Drop for ObserveCancellation {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl RequestTransport for PendingWrite {
        fn execute(&self, _: reqwest::Request) -> TransportFuture {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let cancellation = ObserveCancellation(Arc::clone(&self.cancelled));
            Box::pin(async move {
                let _cancellation = cancellation;
                std::future::pending().await
            })
        }
    }

    #[tokio::test]
    async fn uncertain_write_timeout_cancels_waiting_and_does_not_replay() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicUsize::new(0));
        let client = Client::new(reqwest::Client::new(), Duration::from_millis(10)).with_transport(
            Arc::new(PendingWrite {
                calls: Arc::clone(&calls),
                cancelled: Arc::clone(&cancelled),
            }),
        );
        let outcome = client
            .post("https://unreachable.invalid/write")
            .send()
            .await;
        assert!(matches!(outcome, Err(TransportError::Timeout)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn http_adapter_retains_credentials_query_payload_and_error_status() {
        use wiremock::matchers::{body_json, header, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/test"))
            .and(header("authorization", "Bearer test-credential"))
            .and(query_param("network", "devnet"))
            .and(body_json(serde_json::json!({"value": 7})))
            .respond_with(
                ResponseTemplate::new(409).set_body_json(serde_json::json!({"error": "conflict"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::new(reqwest::Client::new(), Duration::from_secs(1));
        let response = client
            .post(format!("{}/test", server.uri()))
            .bearer_auth("test-credential")
            .query(&[("network", "devnet")])
            .json(&serde_json::json!({"value": 7}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"],
            "conflict"
        );
    }
}
