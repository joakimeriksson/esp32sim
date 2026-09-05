//! Minimal HTTP + WebSocket (RFC 6455) server for the board UI — no external crates.
//! Text frames carry JSON; binary frames carry TFT frames (type 1) and audio (type 2).
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// One connected browser: frames are queued and written by a dedicated thread so a slow or
/// frozen tab can never block the emulator. When the queue is full, old frames are dropped.
pub struct Client { tx: std::sync::mpsc::SyncSender<Vec<u8>>, pub peer: Option<std::net::SocketAddr> }
impl Client {
    fn send(&self, f: Vec<u8>) -> bool {
        match self.tx.try_send(f) {
            Ok(()) => true,
            Err(std::sync::mpsc::TrySendError::Full(_)) => true,          // drop this frame, keep the client
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false, // writer thread gone: client closed
        }
    }
}

pub struct Shared {
    /// queue mode (the wasm build): messages go to `outbox` for the JS glue instead of to sockets
    pub queue: bool,
    pub outbox: VecDeque<(u8, Vec<u8>)>,
    pub clients: Vec<Client>,
    pub incoming: VecDeque<String>,
    pub incoming_bin: VecDeque<Vec<u8>>,
    pub web_dir: String,
    pub hello: Vec<Vec<u8>>,     // frames sent to every new client (board description etc.)
}

#[derive(Clone)]
pub struct WebServer { pub shared: Arc<Mutex<Shared>>, pub port: u16 }

fn sha1(msg: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut m = msg.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 { m.push(0); }
    m.extend_from_slice(&((msg.len() as u64) * 8).to_be_bytes());
    for chunk in m.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in w[..16].iter_mut().enumerate() { *word = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]); }
        for i in 16..80 { w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1); }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i { 0..=19 => ((b & c) | (!b & d), 0x5A827999), 20..=39 => (b ^ c ^ d, 0x6ED9EBA1), 40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC), _ => (b ^ c ^ d, 0xCA62C1D6) };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(word);
            e = d; d = c; c = b.rotate_left(30); b = a; a = t;
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b); h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d); h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for i in 0..5 { out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

fn b64(d: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::new();
    for c in d.chunks(3) {
        let v = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(v >> 18 & 63) as usize] as char); o.push(T[(v >> 12 & 63) as usize] as char);
        o.push(if c.len() > 1 { T[(v >> 6 & 63) as usize] as char } else { '=' }); o.push(if c.len() > 2 { T[(v & 63) as usize] as char } else { '=' });
    }
    o
}

fn frame(opcode: u8, data: &[u8]) -> Vec<u8> {
    let mut f = vec![0x80 | opcode];
    let n = data.len();
    if n < 126 { f.push(n as u8); } else if n < 65536 { f.push(126); f.extend_from_slice(&(n as u16).to_be_bytes()); } else { f.push(127); f.extend_from_slice(&(n as u64).to_be_bytes()); }
    f.extend_from_slice(data);
    f
}

impl WebServer {
    pub fn start(port: u16, web_dir: String) -> std::io::Result<WebServer> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let shared = Arc::new(Mutex::new(Shared { queue: false, outbox: VecDeque::new(), clients: Vec::new(), incoming: VecDeque::new(), incoming_bin: VecDeque::new(), web_dir, hello: Vec::new() }));
        let s2 = shared.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let s3 = s2.clone();
                std::thread::spawn(move || handle_client(stream, s3));
            }
        });
        Ok(WebServer { shared, port })
    }
    /// A server with no sockets: everything sent is queued for whoever drains `take_outbox`
    /// (the wasm glue), and input arrives through `push_incoming*`. Same protocol, no transport.
    pub fn queued() -> WebServer {
        WebServer { shared: Arc::new(Mutex::new(Shared { queue: true, outbox: VecDeque::new(), clients: Vec::new(), incoming: VecDeque::new(), incoming_bin: VecDeque::new(), web_dir: String::new(), hello: Vec::new() })), port: 0 }
    }
    pub fn send_text(&self, s: &str) { self.emit(1, s.as_bytes()); }
    pub fn send_binary(&self, d: &[u8]) { self.emit(2, d); }
    fn emit(&self, kind: u8, d: &[u8]) {
        let queue = self.shared.lock().unwrap().queue;
        if queue { self.shared.lock().unwrap().outbox.push_back((kind, d.to_vec())); } else { self.broadcast(frame(kind, d)); }
    }
    fn broadcast(&self, f: Vec<u8>) {
        let mut sh = self.shared.lock().unwrap();
        sh.clients.retain(|c| c.send(f.clone()));
    }
    /// Queue mode: take everything sent since the last call, as (1 = text, 2 = binary) messages.
    pub fn take_outbox(&self) -> Vec<(u8, Vec<u8>)> { self.shared.lock().unwrap().outbox.drain(..).collect() }
    pub fn push_incoming(&self, s: String) { self.shared.lock().unwrap().incoming.push_back(s); }
    pub fn push_incoming_bin(&self, d: Vec<u8>) { self.shared.lock().unwrap().incoming_bin.push_back(d); }
    pub fn set_hello(&self, frames: Vec<Vec<u8>>) { self.shared.lock().unwrap().hello = frames; }
    /// Only socket clients replay a late-join snapshot; queued consumers use the live outbox.
    pub fn needs_hello(&self) -> bool { !self.shared.lock().unwrap().queue }
    pub fn poll_incoming(&self) -> Vec<String> { let mut sh = self.shared.lock().unwrap(); sh.incoming.drain(..).collect() }
    pub fn poll_incoming_bin(&self) -> Vec<Vec<u8>> { let mut sh = self.shared.lock().unwrap(); sh.incoming_bin.drain(..).collect() }
    pub fn clients(&self) -> usize { self.shared.lock().unwrap().clients.len() }
}

fn handle_client(mut stream: TcpStream, shared: Arc<Mutex<Shared>>) {
    let mut req = Vec::new();
    let mut buf = [0u8; 4096];
    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf) { Ok(0) | Err(_) => return, Ok(n) => req.extend_from_slice(&buf[..n]) }
        if req.len() > 65536 { return; }
    }
    let text = String::from_utf8_lossy(&req).to_string();
    let key = text.lines().find_map(|l| l.strip_prefix("Sec-WebSocket-Key:")).map(|k| k.trim().to_string());
    let Some(key) = key else {
        // plain HTTP: serve a file
        let path = text.split_whitespace().nth(1).unwrap_or("/");
        let path = if path == "/" { "/index.html" } else { path };
        let web_dir = shared.lock().unwrap().web_dir.clone();
        let safe = !path.contains("..");
        let body = if safe { std::fs::read(format!("{}{}", web_dir, path)).ok() } else { None };
        let (status, body, ctype) = match body { Some(b) => ("200 OK", b, if path.ends_with(".js") { "application/javascript" } else { "text/html; charset=utf-8" }), None => ("404 Not Found", b"not found".to_vec(), "text/plain") };
        let _ = stream.write_all(format!("HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", status, ctype, body.len()).as_bytes());
        let _ = stream.write_all(&body);
        return;
    };
    let accept = b64(&sha1(format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", key).as_bytes()));
    let _ = stream.write_all(format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", accept).as_bytes());
    let _ = stream.set_nodelay(true);
    {
        let mut sh = shared.lock().unwrap();
        let hello = sh.hello.clone();
        for f in hello { let _ = stream.write_all(&f); }
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(256);
        let mut out = stream.try_clone().unwrap();
        std::thread::spawn(move || { while let Ok(f) = rx.recv() { if out.write_all(&f).is_err() { break; } } });
        sh.clients.push(Client { tx, peer: stream.peer_addr().ok() });
    }
    // read loop
    let mut read_exact = |n: usize| -> Option<Vec<u8>> { let mut v = vec![0u8; n]; let mut got = 0; while got < n { match stream.read(&mut v[got..]) { Ok(0) | Err(_) => return None, Ok(k) => got += k } } Some(v) };
    while let Some(h) = read_exact(2) {
        let op = h[0] & 0xf; let masked = h[1] & 0x80 != 0; let mut len = (h[1] & 0x7f) as u64;
        if len == 126 { let Some(e) = read_exact(2) else { break }; len = u16::from_be_bytes([e[0], e[1]]) as u64; }
        else if len == 127 { let Some(e) = read_exact(8) else { break }; len = u64::from_be_bytes(e.try_into().unwrap()); }
        let mask = if masked { let Some(m) = read_exact(4) else { break }; m } else { vec![0; 4] };
        if len > 8 << 20 { break; }
        let Some(mut data) = read_exact(len as usize) else { break };
        if masked { for (i, b) in data.iter_mut().enumerate() { *b ^= mask[i & 3]; } }
        match op {
            8 => break,
            9 => {}
            1 => { shared.lock().unwrap().incoming.push_back(String::from_utf8_lossy(&data).to_string()); }
            2 => { let mut sh = shared.lock().unwrap(); if sh.incoming_bin.len() < 4 { sh.incoming_bin.push_back(data); } }
            _ => {}
        }
    }
    let mut sh = shared.lock().unwrap();
    let me = stream.peer_addr().ok();
    sh.clients.retain(|c| c.peer != me);
}

/// Tiny JSON helpers for the few message shapes the UI sends.
pub fn json_str(msg: &str, key: &str) -> Option<String> {
    let k = format!("\"{}\"", key);
    let p = msg.find(&k)? + k.len();
    let rest = msg[p..].trim_start().strip_prefix(':')?.trim_start();
    if let Some(r) = rest.strip_prefix('"') {
        let mut out = String::new(); let mut it = r.chars();
        while let Some(c) = it.next() { match c { '"' => break, '\\' => { if let Some(n) = it.next() { out.push(match n { 'n' => '\n', 't' => '\t', x => x }); } } x => out.push(x) } }
        Some(out)
    } else {
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '.')).unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}
pub fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() { match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '\n' => o.push_str("\\n"), '\r' => o.push_str("\\r"), c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)), c => o.push(c) } }
    o
}
