use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const SE: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("domain:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("created:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("modified:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("expires:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("state:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("nserver:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
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
# Copyright (c) 1997- The Swedish Internet Foundation.\r
# All rights reserved.\r
# The information obtained through searches, or otherwise, is protected\r
# by the Swedish Copyright Act (1960:729) and international conventions.\r
# It is also subject to database protection according to the Swedish\r
# Copyright Act.\r
# Any use of this material to target advertising or\r
# similar activities is forbidden and will be prosecuted.\r
# If any of the information below is transferred to a third\r
# party, it must be done in its entirety. This server must\r
# not be used as a backend for a search engine.\r
# Result of search for registered domain names under\r
# the .se top level domain.\r
# This whois printout is printed with UTF-8 encoding.\r
#\r
state:            active\r
domain:           wikipedia.se\r
holder:           janwik1211-00001\r
created:          2004-04-27\r
modified:         2024-04-02\r
expires:          2025-04-27\r
transferred:      2019-02-15\r
nserver:          ns2.loopia.se 185.71.156.20 2a02:250:fffe::20\r
nserver:          ns1.loopia.se 93.188.0.20 2a02:250:ffff::20\r
dnssec:           signed delegation\r
registry-lock:    unlocked\r
status:           ok\r
registrar:        Loopia AB\r
";
        let whois = (SE.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 27);
        assert_eq!(created.month() as i32, 4);
        assert_eq!(created.year(), 2004);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 2);
        assert_eq!(updated.month() as i32, 4);
        assert_eq!(updated.year(), 2024);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 27);
        assert_eq!(expiry.month() as i32, 4);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.status.as_deref(), Some("active"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns2.loopia.se");
        assert_eq!(nss[1], "ns1.loopia.se");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
# Copyright (c) 1997- The Swedish Internet Foundation.\r
# All rights reserved.\r
# The information obtained through searches, or otherwise, is protected\r
# by the Swedish Copyright Act (1960:729) and international conventions.\r
# It is also subject to database protection according to the Swedish\r
# Copyright Act.\r
# Any use of this material to target advertising or\r
# similar activities is forbidden and will be prosecuted.\r
# If any of the information below is transferred to a third\r
# party, it must be done in its entirety. This server must\r
# not be used as a backend for a search engine.\r
# Result of search for registered domain names under\r
# the .se top level domain.\r
# This whois printout is printed with UTF-8 encoding.\r
#\r
domain \"??????.se\" not found\r
";
        let whois = (SE.map_response)(RESP);
        assert!(whois.is_none());
    }
}
