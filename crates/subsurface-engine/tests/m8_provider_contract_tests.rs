mod support;

use std::io::{Read, Write};
use std::net::TcpStream;

use support::LocalHttpFake;

#[test]
fn local_http_fake_starts_and_stops() {
    let address;
    {
        let server = LocalHttpFake::start();
        address = server.address();
        for _ in 0..2 {
            let mut stream = TcpStream::connect(address).expect("connect to local HTTP fake");
            stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .expect("request local HTTP fake");
            let mut response = String::new();
            stream
                .read_to_string(&mut response)
                .expect("read local HTTP fake response");
            assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
            assert!(response.contains("\r\nContent-Length: 11\r\n"));
            assert!(response.ends_with("{\"ok\":true}"));
        }
    }

    assert!(TcpStream::connect(address).is_err());
}
