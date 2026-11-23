//! Shared WebSocket helpers for VAPIX data-stream endpoints
//! (events, streamstatus, …).

use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::debug;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{client_tls_with_config, Connector, WebSocket};

use crate::config::credentials::Credentials;
use crate::vapix::client::VapixClient;

/// Fetch a single-use WebSocket session token via `/axis-cgi/wssession.cgi`.
/// The token is short-lived; callers should connect immediately.
pub fn get_ws_session(client: &VapixClient) -> Result<String> {
    let text = client
        .get_text("/axis-cgi/wssession.cgi", &[])
        .context("Failed to fetch WebSocket session token")?;
    let token = text.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("wssession.cgi returned an empty token");
    }
    debug!("Got WS session token");
    Ok(token)
}

/// Build a `ws://` (or `wss://` when HTTPS is enabled) URL for
/// `/vapix/ws-data-stream` with the given source(s) and token.
pub fn build_ws_url(creds: &Credentials, host: &str, token: &str, sources: &str) -> String {
    let scheme = if creds.https { "wss" } else { "ws" };
    format!(
        "{}://{}:{}/vapix/ws-data-stream?wssession={}&sources={}",
        scheme, host, creds.port, token, sources
    )
}

/// A connected WebSocket, plain or TLS.
pub type WsSocket = WebSocket<MaybeTlsStream<TcpStream>>;

/// How long to wait for the TCP connection before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Open a WebSocket to `url`.
///
/// The TCP connect has its own timeout and the handshake (including TLS) is
/// bounded by the camera timeout, so an unreachable or silent camera cannot
/// hang the caller. `wss://` verifies the certificate against the same trust
/// roots as the HTTP client, unless `creds.verify_ssl` is off.
pub fn connect_ws(creds: &Credentials, url: &str) -> Result<WsSocket> {
    let uri: tungstenite::http::Uri = url.parse().context("Invalid WebSocket URL")?;
    let host = uri.host().context("WebSocket URL has no host")?;
    let tls = uri.scheme_str() == Some("wss");
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });

    let stream = connect_tcp(host, port)?;
    let handshake_limit = Duration::from_secs(creds.timeout.max(1));
    stream.set_read_timeout(Some(handshake_limit)).ok();
    stream.set_write_timeout(Some(handshake_limit)).ok();
    stream.set_nodelay(true).ok();

    let connector = if tls && !creds.verify_ssl { Some(Connector::Rustls(insecure_tls_config())) } else { None };
    let (mut socket, _response) = client_tls_with_config(url, stream, None, connector)
        .map_err(|e| anyhow::anyhow!("WebSocket handshake failed: {}", e))?;
    // The limit above is for the handshake only; a stream may stay quiet for long.
    set_read_timeout(&mut socket, None);
    Ok(socket)
}

/// Connect to `host:port`, trying each resolved address with a timeout.
fn connect_tcp(host: &str, port: u16) -> Result<TcpStream> {
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("Cannot resolve {host}"))?
        .collect();
    let mut last: Option<std::io::Error> = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = Some(e),
        }
    }
    match last {
        Some(e) => Err(e).with_context(|| format!("WebSocket connection to {host}:{port} failed")),
        None => anyhow::bail!("{host} did not resolve to any address"),
    }
}

/// Set the read timeout of the TCP stream under a plain or TLS WebSocket.
/// `None` makes reads block without limit.
pub fn set_read_timeout(socket: &mut WsSocket, timeout: Option<Duration>) {
    let _ = match socket.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(timeout),
        MaybeTlsStream::Rustls(s) => s.sock.set_read_timeout(timeout),
        _ => Ok(()),
    };
}

/// A TLS client configuration that accepts any certificate, for cameras with
/// a self-signed certificate when the user asked for `--insecure`
/// (`verify_ssl: false`). Signatures are still checked, so a connection is
/// encrypted, just not authenticated.
fn insecure_tls_config() -> Arc<rustls::ClientConfig> {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::{ring, verify_tls12_signature, verify_tls13_signature, CryptoProvider};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, SignatureScheme};

    #[derive(Debug)]
    struct AcceptAny(Arc<CryptoProvider>);

    impl ServerCertVerifier for AcceptAny {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp: &[u8],
            _now: UnixTime,
        ) -> std::result::Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
        }
        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    let provider = Arc::new(ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAny(provider)))
        .with_no_client_auth();
    Arc::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vapix::testserver::test_creds;
    use std::net::TcpListener;
    use std::time::Instant;
    use tungstenite::Message;

    fn listener() -> (TcpListener, u16) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        (l, port)
    }

    /// Accept one connection, upgrade it (through TLS if `tls`), send "hello".
    fn serve_hello(l: TcpListener, tls: bool) {
        std::thread::spawn(move || {
            let Ok((tcp, _)) = l.accept() else { return };
            if tls {
                let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
                let key = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());
                let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                    .with_safe_default_protocol_versions()
                    .unwrap()
                    .with_no_client_auth()
                    .with_single_cert(vec![cert.cert.der().clone()], key)
                    .unwrap();
                let conn = rustls::ServerConnection::new(Arc::new(config)).unwrap();
                let stream = rustls::StreamOwned::new(conn, tcp);
                if let Ok(mut ws) = tungstenite::accept(stream) {
                    let _ = ws.send(Message::Text("hello".into()));
                    let _ = ws.read();
                }
            } else if let Ok(mut ws) = tungstenite::accept(tcp) {
                let _ = ws.send(Message::Text("hello".into()));
                let _ = ws.read();
            }
        });
    }

    fn read_text(socket: &mut WsSocket) -> String {
        match socket.read().expect("read") {
            Message::Text(t) => t.to_string(),
            other => panic!("unexpected message {other:?}"),
        }
    }

    #[test]
    fn plain_websocket_connects() {
        let (l, port) = listener();
        serve_hello(l, false);
        let creds = test_creds(port);
        let mut socket = connect_ws(&creds, &format!("ws://127.0.0.1:{port}/vapix/ws-data-stream")).unwrap();
        assert_eq!(read_text(&mut socket), "hello");
    }

    #[test]
    fn wss_connects_when_verification_is_off() {
        let (l, port) = listener();
        serve_hello(l, true);
        let mut creds = test_creds(port);
        creds.https = true;
        creds.verify_ssl = false;
        let mut socket = connect_ws(&creds, &format!("wss://127.0.0.1:{port}/vapix/ws-data-stream")).unwrap();
        assert_eq!(read_text(&mut socket), "hello");
    }

    #[test]
    fn wss_refuses_an_unknown_certificate_when_verification_is_on() {
        let (l, port) = listener();
        serve_hello(l, true);
        let mut creds = test_creds(port);
        creds.https = true;
        creds.verify_ssl = true;
        let err = match connect_ws(&creds, &format!("wss://127.0.0.1:{port}/vapix/ws-data-stream")) {
            Ok(_) => panic!("a self-signed certificate must be refused"),
            Err(e) => e,
        };
        assert!(format!("{err:#}").to_lowercase().contains("handshake"), "{err:#}");
    }

    #[test]
    fn a_silent_camera_cannot_hold_the_handshake_forever() {
        // Accepts the TCP connection and never answers the upgrade request.
        let (l, port) = listener();
        std::thread::spawn(move || {
            let _held = l.accept();
            std::thread::sleep(Duration::from_secs(10));
        });
        let mut creds = test_creds(port);
        creds.timeout = 1;
        let started = Instant::now();
        let result = connect_ws(&creds, &format!("ws://127.0.0.1:{port}/vapix/ws-data-stream"));
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
    }
}
