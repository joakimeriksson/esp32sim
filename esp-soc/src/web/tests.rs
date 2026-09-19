use super::*;
use std::net::Shutdown;
use std::sync::mpsc;
use std::time::Duration;

struct Connection {
    socket: TcpStream,
    handler: Option<std::thread::JoinHandle<()>>,
}

impl Connection {
    fn new(web: &WebServer) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let shared = web.shared.clone();
        let handler = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_client(stream, shared);
        });
        Self { socket, handler: Some(handler) }
    }

    fn handshake(&mut self, origin: Option<&str>) {
        let origin = origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default();
        write!(self.socket, "GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{origin}\r\n").unwrap();
    }

    fn head(&mut self) -> String {
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            self.socket.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
            assert!(bytes.len() < 4096);
        }
        String::from_utf8(bytes).unwrap()
    }

    fn expect_frame(&mut self, opcode: u8, payload: &[u8]) {
        let expected = frame(opcode, payload);
        let mut actual = vec![0; expected.len()];
        self.socket.read_exact(&mut actual).unwrap();
        assert_eq!(actual, expected);
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(Shutdown::Both);
        self.handler.take().unwrap().join().unwrap();
    }
}

fn socket_web() -> WebServer {
    let web = WebServer::queued();
    web.shared.lock().unwrap().queue = false;
    web
}

fn masked_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() < 126);
    let mask = [0x31, 0x42, 0x53, 0x64];
    let mut bytes = vec![0x80 | opcode, 0x80 | payload.len() as u8];
    bytes.extend_from_slice(&mask);
    bytes.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i & 3]));
    bytes
}

#[test]
fn websocket_key_header_matches_any_case() {
    for line in ["Sec-WebSocket-Key: abc==", "sec-websocket-key: abc==", "SEC-WEBSOCKET-KEY:abc==", "Sec-Websocket-Key :  abc==  "] {
        let head = format!("GET /ws HTTP/1.1\r\nHost: x\r\n{line}\r\nUpgrade: websocket\r\n\r\n");
        assert_eq!(header(&head, "Sec-WebSocket-Key"), Some("abc=="), "{line}");
    }
    assert_eq!(header("GET / HTTP/1.1\r\nHost: x\r\n\r\n", "Sec-WebSocket-Key"), None);
    assert_eq!(header("GET /Sec-WebSocket-Key: HTTP/1.1\r\n\r\n", "GET /Sec-WebSocket-Key"), None);
    assert_eq!(header("GET / HTTP/1.1\r\n\r\nOrigin: fake", "Origin"), None);
}

#[test]
fn websocket_accept_matches_rfc_6455_example() {
    let key = "dGhlIHNhbXBsZSBub25jZQ==";
    assert_eq!(b64(&sha1(format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", key).as_bytes())), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
}

#[test]
fn json_fields_are_top_level_and_strings_decode_unicode() {
    assert_eq!(json_str(r#"{"nested":{"t":"knob"},"t":"serial"}"#, "t").as_deref(), Some("serial"));
    assert_eq!(json_str(r#"{"t":"serial","line":"\uD83D\uDE80"}"#, "line").as_deref(), Some("🚀"));
    assert_eq!(json_str(r#"{"line":"unfinished}"#, "line"), None);
    assert_eq!(json_str(r#"{"d":1e2}"#, "d").as_deref(), Some("100"));
}

#[test]
fn websocket_origin_requires_this_loopback_port_when_present() {
    for origin in ["http://127.0.0.1:8766", "http://localhost:8766"] {
        assert!(local_origin(&format!("GET / HTTP/1.1\r\nOrigin: {origin}\r\n\r\n"), 8766));
    }
    for origin in ["null", "https://localhost:8766", "http://localhost:8765", "http://localhost.evil:8766", "http://127.0.0.1:8766/", "http://127.0.0.1:8766@evil"] {
        assert!(!local_origin(&format!("GET / HTTP/1.1\r\nOrigin: {origin}\r\n\r\n"), 8766), "{origin}");
    }
    assert!(local_origin("GET / HTTP/1.1\r\n\r\n", 8766), "native tools omit Origin");
    assert!(local_origin("GET / HTTP/1.1\r\nOrigin: http://localhost\r\n\r\n", 80));
}

#[test]
fn websocket_rejects_foreign_origin_before_registering_client() {
    let web = socket_web();
    let mut connection = Connection::new(&web);
    connection.handshake(Some("https://example.com"));
    assert!(connection.head().starts_with("HTTP/1.1 403 Forbidden\r\n"));
    assert_eq!(web.clients(), 0);
}

#[test]
fn websocket_snapshot_precedes_live_frames_and_ping_gets_pong() {
    let web = socket_web();
    web.set_hello(vec![frame(1, b"board"), frame(2, b"snapshot")]);
    let mut connection = Connection::new(&web);
    let origin = format!("http://localhost:{}", connection.socket.peer_addr().unwrap().port());
    connection.handshake(Some(&origin));
    assert!(connection.head().starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
    connection.expect_frame(1, b"board");
    web.send_text("live");
    connection.socket.write_all(&masked_frame(9, b"ping payload")).unwrap();
    connection.socket.write_all(&masked_frame(9, b"")).unwrap();
    connection.expect_frame(2, b"snapshot");
    connection.expect_frame(1, b"live");
    connection.expect_frame(10, b"ping payload");
    connection.expect_frame(10, b"");
    assert!(web.poll_incoming().is_empty());
}

#[test]
fn websocket_preserves_frame_received_with_http_head() {
    let web = socket_web();
    let mut connection = Connection::new(&web);
    let mut request = b"GET / HTTP/1.1\r\nSec-WebSocket-Key: abc==\r\n\r\n".to_vec();
    request.extend(masked_frame(9, b"already buffered"));
    connection.socket.write_all(&request).unwrap();
    assert!(connection.head().starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
    connection.expect_frame(10, b"already buffered");
}

#[test]
fn slow_snapshot_client_does_not_hold_emulator_mutex() {
    let web = socket_web();
    web.set_hello(vec![frame(2, &vec![0x5a; 16 << 20])]);
    let mut connection = Connection::new(&web);
    connection.handshake(None);
    assert!(connection.head().starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
    let mut frame_header = [0; 10];
    connection.socket.read_exact(&mut frame_header).unwrap();
    assert_eq!(frame_header[0], 0x82);
    // Stop reading mid-snapshot. A socket write now blocks independently of the
    // emulator; its state and bounded live queue must remain usable.
    let (tx, rx) = mpsc::channel();
    let emulator = std::thread::spawn(move || {
        for _ in 0..300 { web.send_text("live"); }
        web.push_incoming("input".into());
        tx.send(web.poll_incoming()).unwrap();
    });
    let result = rx.recv_timeout(Duration::from_secs(2));
    drop(connection);
    emulator.join().unwrap();
    assert_eq!(result.unwrap(), vec!["input"]);
}

struct StaticRoot(std::path::PathBuf);

impl StaticRoot {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("esp-soc-web-{}-{id}", std::process::id()));
        std::fs::create_dir_all(path.join("public")).unwrap();
        Self(path)
    }
    fn public(&self) -> String { self.0.join("public").to_string_lossy().into_owned() }
}

impl Drop for StaticRoot {
    fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); }
}

#[test]
fn static_files_stay_in_root_and_unknown_content_is_not_html() {
    let root = StaticRoot::new();
    std::fs::write(root.0.join("secret"), b"private").unwrap();
    std::fs::write(root.0.join("public/blob.bin"), b"<script>bad()</script>").unwrap();
    assert_eq!(static_file(&root.public(), "/../secret"), None);
    assert_eq!(static_file(&root.public(), "//secret"), None);
    assert_eq!(static_file(&root.public(), "secret"), None);
    let web = socket_web();
    web.shared.lock().unwrap().web_dir = root.public();
    let mut connection = Connection::new(&web);
    connection.socket.write_all(b"GET /blob.bin HTTP/1.1\r\n\r\n").unwrap();
    let head = connection.head();
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(head.contains("Content-Type: application/octet-stream\r\n"));
    assert!(head.contains("X-Content-Type-Options: nosniff\r\n"));
    let mut body = Vec::new();
    connection.socket.read_to_end(&mut body).unwrap();
    assert_eq!(body, b"<script>bad()</script>");
    assert_eq!(content_type("/run.html"), "text/html; charset=utf-8");
}

#[cfg(unix)]
#[test]
fn static_symlinks_cannot_escape_root() {
    let root = StaticRoot::new();
    std::fs::write(root.0.join("secret"), b"private").unwrap();
    std::fs::write(root.0.join("public/data.txt"), b"public").unwrap();
    std::os::unix::fs::symlink("../secret", root.0.join("public/leak.txt")).unwrap();
    std::os::unix::fs::symlink("data.txt", root.0.join("public/alias.txt")).unwrap();
    assert_eq!(static_file(&root.public(), "/leak.txt"), None);
    assert_eq!(static_file(&root.public(), "/alias.txt"), Some(b"public".to_vec()));
}
