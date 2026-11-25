//! A tiny in-process HTTP server for unit tests of `VapixClient`.
//!
//! It listens on 127.0.0.1 on a free port, hands every request (head and body,
//! as text) to a handler and writes back whatever the handler decides. No
//! authentication is demanded, so the client's Digest probe sees a plain 200.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::credentials::Credentials;
use crate::vapix::client::VapixClient;

/// What the fake camera does with one request.
#[allow(dead_code)]
pub enum Reply {
    /// Answer with this status and JSON body.
    Json(u16, &'static str),
    /// Answer with this status and plain text body.
    Text(u16, &'static str),
    /// Answer 401 with a `WWW-Authenticate` header (the full header value).
    Unauthorized(&'static str),
    /// Close the connection without answering (a camera that is rebooting).
    Drop,
    /// Read the request, then stay silent for this long before closing.
    Stall(Duration),
}

pub struct TestServer {
    pub port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    #[allow(dead_code)]
    served: Arc<AtomicUsize>,
}

impl TestServer {
    /// Start serving. The thread lives until the process ends.
    pub fn start(handler: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let served = Arc::new(AtomicUsize::new(0));
        let (reqs, count) = (requests.clone(), served.clone());
        let handler = Arc::new(handler);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (reqs, count, handler) = (reqs.clone(), count.clone(), handler.clone());
                std::thread::spawn(move || serve(stream, &*handler, &reqs, &count));
            }
        });
        Self { port, requests, served }
    }

    /// A client pointed at this server (plain HTTP, 5 second timeout).
    pub fn client(&self) -> VapixClient {
        VapixClient::new("127.0.0.1", self.port, test_creds(self.port), 5)
    }

    /// Every request received so far, head and body as text.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    /// Number of requests that reached the handler.
    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }
}

pub fn test_creds(port: u16) -> Credentials {
    Credentials {
        user: "root".into(),
        pass: "pass".into(),
        https: false,
        verify_ssl: true,
        port,
        timeout: 5,
    }
}

fn serve(
    mut stream: TcpStream,
    handler: &(dyn Fn(&str) -> Reply + Send + Sync),
    requests: &Mutex<Vec<String>>,
    served: &AtomicUsize,
) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    // A connection may carry several requests (keep-alive); answer each in turn.
    loop {
        let Some(request) = read_request(&mut stream) else { return };
        served.fetch_add(1, Ordering::SeqCst);
        requests.lock().unwrap().push(request.clone());
        let mut extra = String::new();
        let (status, content_type, body) = match handler(&request) {
            Reply::Unauthorized(challenge) => {
                extra = format!("WWW-Authenticate: {challenge}\r\n");
                (401, "text/plain", "")
            }
            Reply::Json(s, b) => (s, "application/json", b),
            Reply::Text(s, b) => (s, "text/plain", b),
            Reply::Drop => return,
            Reply::Stall(d) => {
                std::thread::sleep(d);
                return;
            }
        };
        let head = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\n{extra}Content-Length: {}\r\n\r\n",
            body.len()
        );
        if stream.write_all(head.as_bytes()).is_err() || stream.write_all(body.as_bytes()).is_err() {
            return;
        }
    }
}

/// Read one request: the head, then `Content-Length` bytes of body.
fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let length = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length").then(|| v.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    while buf.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    Some(String::from_utf8_lossy(&buf).to_string())
}

/// Run `f` with every tracing event (down to TRACE) captured, and return what
/// was logged. Only events emitted on the calling thread are seen.
pub fn capture_logs(f: impl FnOnce()) -> String {
    #[derive(Clone)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buf {
        type Writer = Buf;
        fn make_writer(&'a self) -> Buf {
            self.clone()
        }
    }

    let buf = Buf(Arc::new(Mutex::new(Vec::new())));
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(buf.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    let out = buf.0.lock().unwrap().clone();
    String::from_utf8_lossy(&out).to_string()
}
