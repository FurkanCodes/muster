//! A stand-in web service on a local port, for the services Muster
//! reaches by their APIs: each request is answered by a handler and
//! recorded.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex};

/// One request the stand-in was sent.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub(crate) method: String,
    /// Its path and query.
    pub(crate) target: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Its body, as JSON.
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// What was sent so far.
pub(crate) type Recorded = Arc<Mutex<Vec<Request>>>;

/// Serves `answer`, which gives each request a status and a JSON body, on
/// a local port: its address, and the requests as they come.
pub(crate) fn serve(
    answer: impl Fn(&Request, &[Request]) -> (u16, String) + Send + 'static,
) -> (String, Recorded) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let recorded: Recorded = Arc::default();
    let record = recorded.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (method, target) = (
                parts.next().unwrap_or_default().to_owned(),
                parts.next().unwrap_or_default().to_owned(),
            );
            let mut headers = Vec::new();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() {
                    break;
                }
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    headers.push((name.trim().to_owned(), value.trim().to_owned()));
                }
            }
            let length = headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let request = Request {
                method,
                target,
                headers,
                body: String::from_utf8_lossy(&body).into_owned(),
            };
            let (status, text) = {
                let before = record.lock().unwrap().clone();
                answer(&request, &before)
            };
            record.lock().unwrap().push(request);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    (address, recorded)
}
