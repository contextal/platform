mod at;
mod au;
mod be;
mod br;
mod cn;
mod de;
mod dk;
mod edu;
mod fi;
mod fr;
mod gtld;
mod iana;
mod it;
mod jp;
mod kr;
mod nl;
mod no;
mod pl;
mod pt;
mod ru;
mod se;
mod uk;

use crate::DomainInfo;
use std::io::{Read, Write};
use std::net::ToSocketAddrs;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

// NOTE TO SELF
// The code dealing with the cache may appear quirky and slightly inefficient, but
// that's the only way to make the borrow checker happy
// See
// https://github.com/rust-lang/rfcs/blob/master/text/2094-nll.md#problem-case-3-conditional-control-flow-across-functions

#[macro_use]
mod macros {
    macro_rules! capture_til_eol {
        ($prefix:literal) => {
            regex::Regex::new(concat!(r"(?m)^", $prefix, r".*?([^\s].*)")).unwrap()
        };
    }

    macro_rules! capture_digits {
        ($prefix:literal) => {
            regex::Regex::new(concat!(r"(?m)^", $prefix, r".*?(\d+)")).unwrap()
        };
    }

    macro_rules! capture_til_ws {
        ($prefix:literal) => {
            regex::Regex::new(concat!(r"(?m)^", $prefix, r".*?([^\s]+)")).unwrap()
        };
    }

    macro_rules! capture_section {
        ($prefix:literal) => {
            regex::Regex::new(concat!(r"(?m)^", $prefix, r"((?s:.*?))^\s*?$")).unwrap()
        };
    }

    pub(super) use capture_digits;
    pub(super) use capture_section;
    pub(super) use capture_til_eol;
    pub(super) use capture_til_ws;
}

struct TldWhois {
    map_response: fn(resp: &str) -> Option<DomainInfo>,
    get_query_string: Option<fn(ascii_domain: &str) -> String>,
}

fn from_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&c| c as char).collect()
}

impl TldWhois {
    fn query(
        &self,
        domain: &str,
        server: &str,
        timeout: &std::time::Duration,
    ) -> Result<Option<DomainInfo>, std::io::Error> {
        let start = std::time::Instant::now();
        const MAX_SIZE: usize = 8192;
        let query = if let Some(gqs) = self.get_query_string {
            gqs(domain)
        } else {
            format!("{}\r\n", domain)
        };
        debug!("Connecting to {} for query on {}...", server, domain);
        let mut addrs = (server, 43)
            .to_socket_addrs()
            .inspect_err(|_| warn!("Failed to resolve {} for {}", server, domain))?;
        let mut s = loop {
            let addr = addrs.next().ok_or_else(|| {
                warn!("Connection to {} for {} failed", server, domain);
                std::io::Error::new(std::io::ErrorKind::TimedOut, "Connection failed")
            })?;
            let remaining = timeout.saturating_sub(start.elapsed());
            if let Ok(s) = std::net::TcpStream::connect_timeout(&addr, remaining) {
                break s;
            }
        };
        let remaining = timeout.saturating_sub(start.elapsed());
        s.set_write_timeout(Some(remaining))
            .and_then(|_| s.write_all(query.as_bytes()).map(|_| ()))
            .inspect_err(|e| warn!("Send to {} for {} failed: {e}", server, domain))?;
        debug!("Query sent to {} for {}", server, domain);
        let mut buf: Vec<u8> = Vec::with_capacity(MAX_SIZE);
        let remaining = timeout.saturating_sub(start.elapsed());
        s.set_read_timeout(Some(remaining))
            .and_then(|_| s.take(MAX_SIZE as u64).read_to_end(&mut buf).map(|_| ()))
            .inspect_err(|e| warn!("Recv from {} for {} failed: {e}", server, domain))?;
        let reply = if let Ok(s) = std::str::from_utf8(&buf) {
            s.to_string()
        } else {
            from_latin1(&buf)
        };
        trace!("Response from {} for {}:\n{}", server, domain, reply);
        Ok((self.map_response)(reply.as_str()))
    }
}

const KNOWN_SERVERS: &[(&str, &TldWhois)] = &[
    ("whois.nic.at", at::AT),
    ("whois.auda.org.au", au::AU),
    ("whois.dns.be", be::BE),
    ("whois.registro.br", br::BR),
    ("whois.cnnic.cn", cn::CN),
    ("whois.denic.de", de::DE),
    ("whois.punktum.dk", dk::DK),
    ("whois.educause.edu", edu::EDU),
    ("whois.fi", fi::FI),
    ("whois.nic.fr", fr::FR),
    (gtld::GRS_SERVER, gtld::GRS),
    //("whois.weare.ie", gtld::GTLD),
    ("whois.nic.it", it::IT),
    ("whois.jprs.jp", jp::JP),
    ("whois.kr", kr::KR),
    ("whois.domain-registry.nl", nl::NL),
    ("whois.norid.no", no::NO),
    ("whois.dns.pl", pl::PL),
    ("whois.dns.pt", pt::PT),
    ("whois.tcinet.ru", ru::RU),
    ("whois.iis.se", se::SE),
    ("whois.nic.uk", uk::UK),
];

pub struct Whois {
    cache: lru::LruCache<String, (std::time::Instant, Option<DomainInfo>)>,
}

impl Whois {
    pub fn new() -> Self {
        Self {
            cache: lru::LruCache::new(super::CACHE_MAX_ENTRIES),
        }
    }

    fn query_cache_or_server(
        &mut self,
        name: &str,
        tldw: &TldWhois,
        server: &str,
        timeout: &std::time::Duration,
    ) -> Result<Option<&DomainInfo>, std::io::Error> {
        let k = if server == gtld::GRS_SERVER {
            format!("\0{}\0", name)
        } else {
            name.to_string()
        };
        if let Some((t, v)) = self.cache.peek(&k) {
            let max_age = if v.is_some() {
                super::CACHE_MAX_AGE_HIT
            } else {
                super::CACHE_MAX_AGE_MISS
            };
            if t.elapsed() > max_age {
                debug!("Cache for {} is stale", name);
                self.cache.pop(&k);
            } else {
                debug!("Cache hit for {}", name);
            }
        }
        self.cache
            .try_get_or_insert(k, || {
                debug!("Cache miss for {}", name);
                tldw.query(name, server, timeout)
                    .or_else(|e| {
                        if e.kind() == std::io::ErrorKind::TimedOut
                            || e.kind() == std::io::ErrorKind::WouldBlock
                        {
                            Ok(None)
                        } else {
                            Err(e)
                        }
                    })
                    .map(|v| (std::time::Instant::now(), v))
            })
            .map(|(_, v)| v.as_ref())
    }

    fn get_whois_server(
        &mut self,
        name: &str,
        tldw: &TldWhois,
        server: &str,
        timeout: &std::time::Duration,
    ) -> Result<Option<&str>, Box<dyn std::error::Error>> {
        match self.query_cache_or_server(name, tldw, server, timeout) {
            Ok(Some(r)) if r.whois.is_some() => {
                let whois = r.whois.as_ref().unwrap();
                debug!("Whois server for {} from {} is {}", name, server, whois);
                Ok(Some(whois))
            }
            Ok(Some(_)) => {
                debug!("No Whois server available for {} from {}", name, server);
                Ok(None)
            }
            Ok(None) => {
                debug!("Invalid entry for {} from {}", name, server);
                Err("Invalid reply".into())
            }
            Err(e) => {
                debug!("Query for {} to {} failed: {}", name, server, e);
                Err(e.into())
            }
        }
    }

    pub fn lookup(
        &mut self,
        domain: &str,
        timeout: &std::time::Duration,
    ) -> Result<Option<&DomainInfo>, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        let tld = domain.rsplit_once('.').map(|(_, v)| v).unwrap_or(domain);
        debug!("TLD for {} is {}", domain, tld);
        debug!("Querying IANA for the WHOIS server...");
        let remaining = timeout.saturating_sub(start.elapsed());
        let server = match self.get_whois_server(tld, iana::IANA, iana::IANA_SERVER, &remaining) {
            Ok(Some(server)) => {
                debug!(
                    "IANA reported that the WHOIS server for {} is {}",
                    tld, server
                );
                server.to_string()
            }
            Ok(None) => {
                debug!("IANA reported that no WHOIS server exists for {}", tld);
                return Err("No WHOIS server for this TLD".into());
            }
            Err(e) => {
                debug!("IANA query for {} failed: {}", tld, e);
                return Err(e);
            }
        };
        let server = if server == gtld::GRS_SERVER {
            debug!(
                "The WHOIS server for {} is GRS, trying to get a more specific server...",
                tld
            );
            let remaining = timeout.saturating_sub(start.elapsed());
            match self.get_whois_server(domain, gtld::GRS, gtld::GRS_SERVER, &remaining) {
                Ok(Some(server)) if server != gtld::GRS_SERVER => {
                    debug!(
                        "GRS reported that the WHOIS server for {} is {}",
                        domain, server
                    );
                    server.to_string()
                }
                Ok(_) => {
                    debug!(
                        "GRS did not provide a specific WHOIS server for {}, using generic",
                        domain
                    );
                    server
                }
                Err(e) => {
                    debug!("GRS request for WHOIS server for {} failed: {}", domain, e);
                    return Err(e);
                }
            }
        } else {
            debug!("The indicated WHOIS server appears to be specific");
            server
        };
        let tldw = KNOWN_SERVERS
            .iter()
            .find_map(|(s, t)| if *s == server { Some(*t) } else { None })
            .inspect(|_| {
                debug!("WHOIS server {} for {} is known", server, domain);
            })
            .unwrap_or_else(|| {
                debug!(
                    "WHOIS server {} for {} is unknown, assuming gTLD",
                    server, domain
                );
                gtld::GTLD
            });
        let remaining = timeout.saturating_sub(start.elapsed());
        self.query_cache_or_server(domain, tldw, &server, &remaining)
            .map_err(|e| e.into())
    }
}
