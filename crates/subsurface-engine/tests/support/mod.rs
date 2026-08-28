use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone)]
pub struct StubResponse {
    status: u16,
    body: String,
    delay: Duration,
    location: Option<String>,
}

impl StubResponse {
    pub fn json(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
            delay: Duration::ZERO,
            location: None,
        }
    }

    #[allow(dead_code)]
    pub fn redirect(status: u16, location: impl Into<String>) -> Self {
        Self {
            status,
            body: String::new(),
            delay: Duration::ZERO,
            location: Some(location.into()),
        }
    }

    #[allow(dead_code)]
    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn render(&self) -> String {
        let reason = match self.status {
            200 => "OK",
            201 => "Created",
            401 => "Unauthorized",
            403 => "Forbidden",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            _ => "Test Response",
        };
        let location = self
            .location
            .as_ref()
            .map(|value| format!("Location: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "HTTP/1.1 {} {}\r\n{}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            reason,
            location,
            self.body.len(),
            self.body
        )
    }
}

pub struct LocalHttpFake {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    shutdown: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl LocalHttpFake {
    #[allow(dead_code)]
    pub fn start() -> Self {
        Self::start_with(Vec::new())
    }

    pub fn start_with(responses: Vec<StubResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local HTTP fake");
        listener
            .set_nonblocking(true)
            .expect("make local HTTP fake nonblocking");
        let address = listener.local_addr().expect("local HTTP fake address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let scripted = !responses.is_empty();
        let mut responses = VecDeque::from(responses);
        let (shutdown, shutdown_rx) = mpsc::channel();
        let thread = thread::spawn(move || loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }

            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let request = read_request(&mut stream);
                    captured
                        .lock()
                        .expect("capture local HTTP request")
                        .push(String::from_utf8_lossy(&request).into_owned());
                    let response = responses.pop_front().unwrap_or_else(|| {
                        if scripted {
                            StubResponse::json(500, r#"{"error":"no scripted response"}"#)
                        } else {
                            StubResponse::json(200, r#"{"ok":true}"#)
                        }
                    });
                    // Handle each connection on its own thread so a delayed
                    // create can time out while a fingerprint search proceeds.
                    thread::spawn(move || {
                        thread::sleep(response.delay);
                        let _ = stream.write_all(response.render().as_bytes());
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => {
                    eprintln!("local HTTP fake stopped: {error}");
                    break;
                }
            }
        });

        Self {
            address,
            requests,
            shutdown,
            thread: Some(thread),
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("local HTTP requests").clone()
    }
}

impl Drop for LocalHttpFake {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let timeout = Some(Duration::from_millis(250));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    let mut request = Vec::new();
    let mut chunk = [0; 1024];

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                request.extend_from_slice(&chunk[..read]);
                if request_is_complete(&request) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(_) => break,
        }
    }
    request
}

fn request_is_complete(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    });
    content_length
        .map(|length| request.len() >= header_end + 4 + length)
        .unwrap_or(true)
}
