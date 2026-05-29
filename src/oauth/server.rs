//! Browser launch, loopback callback listener, and constant-time byte comparison.
//!
//! Anthropic uses the browser opener plus pasted-code state verification.
//! ChatGPT uses the same opener with a localhost callback listener.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

const PER_CONN_READ_TIMEOUT: Duration = Duration::from_secs(30);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(300);

pub struct CallbackListener {
    pub redirect_uri: String,
    callback_path: String,
    listener: TcpListener,
}

pub struct CallbackResult {
    pub code: String,
}

impl CallbackListener {
    /// Bind to `127.0.0.1:port` (use 0 for an ephemeral port) and accept the
    /// OAuth redirect on `path`. The advertised `redirect_uri` uses
    /// `http://localhost:...` (not `127.0.0.1`) so it matches OAuth clients
    /// that registered the loopback under the `localhost` literal.
    pub fn bind(port: u16, path: &str) -> Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
            .map_err(|e| Error::OAuth(format!("bind loopback :{port}: {e}")))?;
        let actual_port = listener
            .local_addr()
            .map_err(|e| Error::OAuth(format!("local_addr: {e}")))?
            .port();
        let redirect_uri = format!("http://localhost:{actual_port}{path}");
        Ok(Self {
            redirect_uri,
            callback_path: path.to_string(),
            listener,
        })
    }

    pub fn accept(self, expected_state: &str) -> Result<CallbackResult> {
        let deadline = Instant::now() + TOTAL_TIMEOUT;
        self.listener
            .set_nonblocking(false)
            .map_err(|e| Error::OAuth(format!("set_nonblocking: {e}")))?;

        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| {
                    Error::OAuth(format!(
                        "no callback received within {}s — aborting",
                        TOTAL_TIMEOUT.as_secs()
                    ))
                })?;
            let accept_wait = remaining.min(Duration::from_secs(60));

            let (stream, _) = match accept_with_timeout(&self.listener, accept_wait) {
                Ok(s) => s,
                Err(AcceptErr::Timeout) => continue,
                Err(AcceptErr::Io(e)) => return Err(Error::OAuth(format!("accept: {e}"))),
            };
            let mut stream = stream;
            stream
                .set_read_timeout(Some(PER_CONN_READ_TIMEOUT))
                .map_err(|e| Error::OAuth(format!("set_read_timeout: {e}")))?;

            let request = match read_request(&mut stream) {
                Some(r) => r,
                None => {
                    let _ = respond(&mut stream, "400 Bad Request", "<h2>plz: bad request</h2>");
                    continue;
                }
            };

            if request.path != self.callback_path {
                let _ = respond(
                    &mut stream,
                    "404 Not Found",
                    "<h2>plz: not the callback</h2>",
                );
                continue;
            }

            let Params { code, state, err } = parse_query(&request.query);

            if let Some(e) = err {
                let _ = respond(
                    &mut stream,
                    "400 Bad Request",
                    "<h2>plz: login failed</h2><p>Check the terminal.</p>",
                );
                return Err(Error::OAuth(format!("provider returned error: {e}")));
            }

            let (Some(code), Some(state_recv)) = (code, state) else {
                let _ = respond(
                    &mut stream,
                    "400 Bad Request",
                    "<h2>plz: callback missing code or state</h2>",
                );
                continue;
            };

            if !constant_time_eq(state_recv.as_bytes(), expected_state.as_bytes()) {
                let _ = respond(
                    &mut stream,
                    "404 Not Found",
                    "<h2>plz: not the callback</h2>",
                );
                continue;
            }

            let _ = respond(
                &mut stream,
                "200 OK",
                "<h2>You're signed in to plz.</h2><p>You can close this tab.</p>",
            );
            return Ok(CallbackResult { code });
        }
    }
}

struct Params {
    code: Option<String>,
    state: Option<String>,
    err: Option<String>,
}

fn parse_query(query: &str) -> Params {
    let mut code = None;
    let mut state = None;
    let mut err: Option<String> = None;
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        let decoded = urlencoding::decode(v)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| v.to_string());
        match k {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            "error" => err = Some(decoded),
            "error_description" => {
                if let Some(existing) = err.as_mut() {
                    existing.push_str(": ");
                    existing.push_str(&decoded);
                } else {
                    err = Some(decoded);
                }
            }
            _ => {}
        }
    }
    Params { code, state, err }
}

struct ParsedRequest {
    path: String,
    query: String,
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<ParsedRequest> {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).ok()?;
    let req = std::str::from_utf8(&buf[..n]).ok()?;
    let first_line = req.lines().next()?;
    let target = first_line.split_whitespace().nth(1)?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };
    Some(ParsedRequest { path, query })
}

fn respond(stream: &mut std::net::TcpStream, status: &str, body_html: &str) -> std::io::Result<()> {
    let body = format!(
        "<!doctype html><html><body style=\"font-family:sans-serif;text-align:center;padding:3em\">{body_html}</body></html>"
    );
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes())?;
    stream.flush()
}

enum AcceptErr {
    Timeout,
    Io(std::io::Error),
}

fn accept_with_timeout(
    listener: &TcpListener,
    wait: Duration,
) -> std::result::Result<(std::net::TcpStream, std::net::SocketAddr), AcceptErr> {
    let deadline = Instant::now() + wait;
    listener.set_nonblocking(true).map_err(AcceptErr::Io)?;
    let result = loop {
        match listener.accept() {
            Ok(c) => break Ok(c),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    break Err(AcceptErr::Timeout);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => break Err(AcceptErr::Io(e)),
        }
    };
    let _ = listener.set_nonblocking(false);
    result
}

/// XOR-fold equality — does not short-circuit on first mismatching byte.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Open a URL in the user's default browser. Best-effort — if it fails,
/// the caller still prints the URL so the user can open it manually.
pub fn open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = ("open", url);
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = ("xdg-open", url);

    #[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
    {
        let _ = std::process::Command::new(cmd.0).arg(cmd.1).status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = url;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_when_equal() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn constant_time_eq_rejects_different_content() {
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"xbc"));
    }

    #[test]
    fn constant_time_eq_rejects_different_lengths() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
