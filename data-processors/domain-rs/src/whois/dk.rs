use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const DK: &super::TldWhois = &super::TldWhois {
    get_query_string: Some(get_query_string),
    map_response,
};

fn get_query_string(ascii_domain: &str) -> String {
    format!("--show-handles --charset=utf-8 {}\r\n", ascii_domain)
}

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registered:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Expires:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registrar:"));
    static REGISTRANT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!("Registrant"));
    static REGISTRANT_NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Name:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Hostname:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    if let Some(sect) = REGISTRANT
        .captures(resp)
        .map(|cap| cap[1].trim().to_string())
    {
        res.registrant_name = REGISTRANT_NAME
            .captures(&sect)
            .map(|cap| cap[1].trim().to_string());
    }
    let nss: Vec<String> = NS
        .captures_iter(resp)
        .map(|cap| cap[1].trim().to_string())
        .collect();
    res.nss = if nss.is_empty() { None } else { Some(nss) };
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn found() {
        const RESP: &str = "\
# Hello x.x.x.x. Your session has been logged.\r
#\r
# Copyright (c) 2002 - 2025 by Punktum dk A/S\r
#\r
# Version: 5.4.0\r
#\r
# The data in the DK Whois database is provided by Punktum dk A/S\r
# for information purposes only, and to assist persons in obtaining\r
# information about or related to a domain name registration record.\r
# We do not guarantee its accuracy. We will reserve the right to remove\r
# access for entities abusing the data, without notice.\r
#\r
# Any use of this material to target advertising or similar activities\r
# are explicitly forbidden and will be prosecuted. Punktum dk A/S\r
# requests to be notified of any such activities or suspicions thereof.\r
\r
Domain:               google.dk\r
DNS:                  google.dk\r
Registered:           1999-01-10\r
Expires:              2025-03-31\r
Registrar:            MarkMonitor Inc.\r
Registration period:  1 year\r
VID:                  no\r
DNSSEC:               Unsigned delegation\r
Status:               Active\r
\r
Registrant\r
Handle:               ***N/A***\r
Name:                 Google LLC\r
Attention:            Domain Administrator\r
Address:              1600 Amphitheatre Parkway\r
Postalcode:           94043\r
City:                 Mountain View\r
Country:              US\r
Phone:                +1 650-253-0000\r
\r
Nameservers\r
Hostname:             ns1.google.com\r
Hostname:             ns2.google.com\r
Hostname:             ns3.google.com\r
Hostname:             ns4.google.com\r
";
        let whois = (DK.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 10);
        assert_eq!(created.month() as i32, 1);
        assert_eq!(created.year(), 1999);
        let expiry = whois.expiry.expect("created date");
        assert_eq!(expiry.day(), 31);
        assert_eq!(expiry.month() as i32, 3);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.registrar.as_deref(), Some("MarkMonitor Inc."));
        assert_eq!(whois.registrant_name.as_deref(), Some("Google LLC"));
        assert_eq!(whois.status.as_deref(), Some("Active"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1.google.com");
        assert_eq!(nss[1], "ns2.google.com");
        assert_eq!(nss.len(), 4);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
# Hello x.x.x.x. Your session has been logged.\r
#\r
# Copyright (c) 2002 - 2025 by Punktum dk A/S\r
#\r
# Version: 5.4.0\r
#\r
# The data in the DK Whois database is provided by Punktum dk A/S\r
# for information purposes only, and to assist persons in obtaining\r
# information about or related to a domain name registration record.\r
# We do not guarantee its accuracy. We will reserve the right to remove\r
# access for entities abusing the data, without notice.\r
#\r
# Any use of this material to target advertising or similar activities\r
# are explicitly forbidden and will be prosecuted. Punktum dk A/S\r
# requests to be notified of any such activities or suspicions thereof.\r
\r
No entries found for the selected source.\r
";
        let whois = (DK.map_response)(RESP);
        assert!(whois.is_none());
    }
}
