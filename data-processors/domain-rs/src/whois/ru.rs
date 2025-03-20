use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const RU: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("domain:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("created:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("paid-till:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("registrar:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("state:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("nserver:"));
    static REGISTRANT_NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("person:"));
    static REGISTRANT_ORG: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("org:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.registrant_name = REGISTRANT_NAME
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.registrant_org = REGISTRANT_ORG
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
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
% TCI Whois Service. Terms of use:\r
% https://tcinet.ru/documents/whois_ru_rf.pdf (in Russian)\r
% https://tcinet.ru/documents/whois_su.pdf (in Russian)\r
\r
domain:        VK.RU\r
nserver:       ns1.vk.ru. 87.240.131.131\r
nserver:       ns2.vk.ru. 95.213.21.21\r
nserver:       ns3.vk.ru. 93.186.238.238\r
nserver:       ns4.vk.ru. 87.240.136.136\r
state:         REGISTERED, DELEGATED, VERIFIED\r
org:           LLC \"V Kontakte\"\r
taxpayer-id:   7842349892\r
registrar:     RU-CENTER-RU\r
admin-contact: https://www.nic.ru/whois\r
created:       1999-06-18T13:39:09Z\r
paid-till:     2025-06-30T21:00:00Z\r
free-date:     2025-08-01\r
source:        TCI\r
\r
Last updated on 2025-03-06T11:33:01Z\r
\r
";
        let whois = (RU.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 18);
        assert_eq!(created.month() as i32, 6);
        assert_eq!(created.year(), 1999);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 30);
        assert_eq!(expiry.month() as i32, 6);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.registrar.as_deref(), Some("RU-CENTER-RU"));
        assert_eq!(
            whois.status.as_deref(),
            Some("REGISTERED, DELEGATED, VERIFIED")
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1.vk.ru.");
        assert_eq!(nss[3], "ns4.vk.ru.");
        assert_eq!(nss.len(), 4);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
% TCI Whois Service. Terms of use:\r
% https://tcinet.ru/documents/whois_ru_rf.pdf (in Russian)\r
% https://tcinet.ru/documents/whois_su.pdf (in Russian)\r
\r
No entries found for the selected source(s).\r
\r
Last updated on 2025-03-06T11:43:01Z\r
\r
";
        let whois = (RU.map_response)(RESP);
        assert!(whois.is_none());
    }
}
