use std::io::{Read, Write};
use std::net::ToSocketAddrs;
use std::time::Duration;

pub fn domain(name: &str, timeout: &Duration) -> Result<Option<whos::domain::Domain>, whos::Error> {
    for (suffix, maybe_server) in whos::SUFFIX_SERVER_LIST.entries() {
        if name.ends_with(suffix) {
            return if let Some(server) = maybe_server {
                let raw = whois_raw(name, (server, 43), timeout)?;
                Ok(whos::domain::parse(&raw))
            } else {
                Err(whos::Error::NoServer)
            };
        }
    }
    Err(whos::Error::UnknownSuffix)
}

fn whois_raw(name: &str, server: (&str, u16), timeout: &Duration) -> std::io::Result<String> {
    let addr = server.to_socket_addrs()?.next().unwrap();
    let mut stream = std::net::TcpStream::connect_timeout(&addr, *timeout)?;
    stream.set_read_timeout(Some(*timeout))?;
    stream.set_write_timeout(Some(*timeout))?;
    stream.write_all(name.as_bytes())?;
    stream.write_all(b"\r\n")?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf)?;
    Ok(buf)
}
