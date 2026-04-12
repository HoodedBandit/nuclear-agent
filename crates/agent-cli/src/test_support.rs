use super::*;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

pub(crate) fn temp_storage() -> Storage {
    Storage::open_at(std::env::temp_dir().join(format!("nuclear-cli-test-{}", Uuid::new_v4())))
        .unwrap()
}

#[derive(Debug, Clone)]
pub(crate) struct CapturedHttpRequest {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) body: String,
}

#[derive(Debug, Clone)]
pub(crate) struct MockHttpExpectation {
    pub(crate) method: &'static str,
    pub(crate) path: String,
    pub(crate) response_body: String,
    pub(crate) status_line: &'static str,
    pub(crate) content_type: &'static str,
}

impl MockHttpExpectation {
    pub(crate) fn json<T: Serialize>(
        method: &'static str,
        path: impl Into<String>,
        response: &T,
    ) -> Self {
        Self {
            method,
            path: path.into(),
            response_body: serde_json::to_string(response).unwrap(),
            status_line: "200 OK",
            content_type: "application/json",
        }
    }
}

pub(crate) struct MockHttpServer {
    pub(crate) origin: String,
    requests: Arc<Mutex<Vec<CapturedHttpRequest>>>,
    handle: JoinHandle<Result<()>>,
}

impl MockHttpServer {
    pub(crate) async fn finish(self) -> Result<Vec<CapturedHttpRequest>> {
        self.handle.await??;
        Ok(self.requests.lock().unwrap().clone())
    }
}

pub(crate) async fn spawn_mock_http_server(
    expectations: Vec<MockHttpExpectation>,
    expected_auth: Option<String>,
) -> MockHttpServer {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_clone = Arc::clone(&requests);
    let expected_auth_clone = expected_auth.clone();
    let mut queue = VecDeque::from(expectations);
    let handle = tokio::spawn(async move {
        while let Some(expected) = queue.pop_front() {
            let (mut stream, _) = listener.accept().await?;
            let raw = read_local_http_request(&mut stream).await?;
            let captured = parse_http_request(&raw);
            assert_eq!(captured.method, expected.method);
            assert_eq!(captured.path, expected.path);
            if let Some(expected_auth) = expected_auth_clone.as_deref() {
                assert_eq!(
                    captured.headers.get("authorization").map(String::as_str),
                    Some(expected_auth)
                );
            }
            requests_clone.lock().unwrap().push(captured);
            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                expected.status_line,
                expected.content_type,
                expected.response_body.len(),
                expected.response_body
            );
            stream.write_all(response.as_bytes()).await?;
        }
        Ok(())
    });

    MockHttpServer {
        origin: format!("http://{addr}"),
        requests,
        handle,
    }
}

async fn read_local_http_request(stream: &mut tokio::net::TcpStream) -> Result<String> {
    let mut buffer = Vec::new();
    let mut header_end = None;
    let mut content_length = 0usize;
    loop {
        let mut chunk = [0u8; 1024];
        let bytes = stream.read(&mut chunk).await?;
        if bytes == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes]);
        if header_end.is_none() {
            if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                header_end = Some(index + 4);
                let headers = String::from_utf8_lossy(&buffer[..index + 4]);
                for line in headers.lines() {
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            content_length = value.trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }
            }
        }
        if let Some(end) = header_end {
            if buffer.len() >= end + content_length {
                break;
            }
        }
    }
    Ok(String::from_utf8(buffer)?)
}

fn parse_http_request(raw: &str) -> CapturedHttpRequest {
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    CapturedHttpRequest {
        method,
        path,
        headers,
        body: body.to_string(),
    }
}
