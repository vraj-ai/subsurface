use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct LocalHttpFake {
    address: SocketAddr,
    shutdown: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl LocalHttpFake {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local HTTP fake");
        listener
            .set_nonblocking(true)
            .expect("make local HTTP fake nonblocking");
        let address = listener.local_addr().expect("local HTTP fake address");
        let (shutdown, shutdown_rx) = mpsc::channel();
        let thread = thread::spawn(move || loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }

            match listener.accept() {
                Ok((mut stream, _)) => {
                    let timeout = Some(Duration::from_millis(250));
                    let _ = stream.set_read_timeout(timeout);
                    let _ = stream.set_write_timeout(timeout);
                    let mut request = [0; 1024];
                    let _ = stream.read(&mut request);
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(_) => break,
            }
        });

        Self {
            address,
            shutdown,
            thread: Some(thread),
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
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
