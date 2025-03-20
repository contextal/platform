use super::macros::*;
use crate::DomainInfo;

pub const IANA_SERVER: &str = "whois.iana.org";

pub const IANA: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("domain:"));
    static WHOIS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("whois:"));
    // There is little point in parsing things further as we
    // really only care about whois for recursion

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.whois = WHOIS.captures(resp).map(|cap| cap[1].trim().to_string());
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn found() {
        const RESP: &str = "\
% IANA WHOIS server\r
% for more information on IANA, visit http://www.iana.org\r
% This query returned 1 object\r
\r
domain:       COM\r
\r
organisation: VeriSign Global Registry Services\r
address:      12061 Bluemont Way\r
address:      Reston VA 20190\r
address:      United States of America (the)\r
\r
contact:      administrative\r
name:         Registry Customer Service (ADMIN)\r
organisation: VeriSign Global Registry Services (ADMIN)\r
address:      12061 Bluemont Way\r
address:      Reston VA 20190\r
address:      United States of America (the)\r
phone:        +1 703 925-6999\r
fax-no:       +1 703 948 3978\r
e-mail:       info@verisign-grs.com\r
\r
contact:      technical\r
name:         Registry Customer Service (TECH)\r
organisation: VeriSign Global Registry Services (TECH)\r
address:      12061 Bluemont Way\r
address:      Reston VA 20190\r
address:      United States of America (the)\r
phone:        +1 703 925-6999\r
fax-no:       +1 703 948 3978\r
e-mail:       info@verisign-grs.com\r
\r
nserver:      A.GTLD-SERVERS.NET 192.5.6.30 2001:503:a83e:0:0:0:2:30\r
nserver:      B.GTLD-SERVERS.NET 192.33.14.30 2001:503:231d:0:0:0:2:30\r
nserver:      C.GTLD-SERVERS.NET 192.26.92.30 2001:503:83eb:0:0:0:0:30\r
nserver:      D.GTLD-SERVERS.NET 192.31.80.30 2001:500:856e:0:0:0:0:30\r
nserver:      E.GTLD-SERVERS.NET 192.12.94.30 2001:502:1ca1:0:0:0:0:30\r
nserver:      F.GTLD-SERVERS.NET 192.35.51.30 2001:503:d414:0:0:0:0:30\r
nserver:      G.GTLD-SERVERS.NET 192.42.93.30 2001:503:eea3:0:0:0:0:30\r
nserver:      H.GTLD-SERVERS.NET 192.54.112.30 2001:502:8cc:0:0:0:0:30\r
nserver:      I.GTLD-SERVERS.NET 192.43.172.30 2001:503:39c1:0:0:0:0:30\r
nserver:      J.GTLD-SERVERS.NET 192.48.79.30 2001:502:7094:0:0:0:0:30\r
nserver:      K.GTLD-SERVERS.NET 192.52.178.30 2001:503:d2d:0:0:0:0:30\r
nserver:      L.GTLD-SERVERS.NET 192.41.162.30 2001:500:d937:0:0:0:0:30\r
nserver:      M.GTLD-SERVERS.NET 192.55.83.30 2001:501:b1f9:0:0:0:0:30\r
ds-rdata:     19718 13 2 8acbb0cd28f41250a80a491389424d341522d946b0da0c0291f2d3d771d7805a\r
\r
whois:        whois.verisign-grs.com\r
\r
status:       ACTIVE\r
remarks:      Registration information: http://www.verisigninc.com\r
\r
created:      1985-01-01\r
changed:      2023-12-07\r
source:       IANA\r
";
        let whois = (IANA.map_response)(RESP).expect("mapping");
        assert_eq!(whois.whois.as_deref(), Some("whois.verisign-grs.com"));
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
% IANA WHOIS server\r
% for more information on IANA, visit http://www.iana.org\r
% This query returned 0 objects.\r
%\r
% You queried for aaaaaaaa but this server does not have\r
% any data for aaaaaaaa.\r
%\r
% If you need further information please check the web site\r
% or use -h for help\r
";
        let whois = (IANA.map_response)(RESP);
        assert!(whois.is_none());
    }
}
