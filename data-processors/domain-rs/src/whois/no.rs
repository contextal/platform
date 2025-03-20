use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const NO: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Domain Name\.*:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Created:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Last updated:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Registrar Handle\.*:"));
    static TECH: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Tech-c Handle\.*:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.tech_name = TECH.captures(resp).map(|cap| cap[1].trim().to_string());
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn found() {
        const RESP: &str = "\
% By looking up information in the domain registration directory\r
% service, you confirm that you accept the terms and conditions of the\r
% service:\r
% https://www.norid.no/en/domeneoppslag/vilkar/\r
%\r
% Norid AS holds the copyright to the lookup service, content,\r
% layout and the underlying collections of information used in the\r
% service (cf. the Act on Intellectual Property of May 2, 1961, No.\r
% 2). Any commercial use of information from the service, including\r
% targeted marketing, is prohibited. Using information from the domain\r
% registration directory service in violation of the terms and\r
% conditions may result in legal prosecution.\r
%\r
% The whois service at port 43 is intended to contribute to resolving\r
% technical problems where individual domains threaten the\r
% functionality, security and stability of other domains or the\r
% internet as an infrastructure. It does not give any information\r
% about who the holder of a domain is. To find information about a\r
% domain holder, please visit our website:\r
% https://www.norid.no/en/domeneoppslag/\r
\r
Domain Information\r
\r
NORID Handle...............: EBA78D-NORID\r
Domain Name................: ebay.no\r
Registrar Handle...........: REG466-NORID\r
Tech-c Handle..............: EI14R-NORID\r
Name Server Handle.........: DNSP1350H-NORID\r
Name Server Handle.........: DNSP1351H-NORID\r
Name Server Handle.........: DNSP1352H-NORID\r
Name Server Handle.........: DNSP1353H-NORID\r
Name Server Handle.........: NSEB27H-NORID\r
Name Server Handle.........: NSEB28H-NORID\r
Name Server Handle.........: NSEB29H-NORID\r
Name Server Handle.........: NSEB30H-NORID\r
\r
Additional information:\r
Created:         2000-02-22\r
Last updated:    2025-01-23\r
";
        let whois = (NO.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 22);
        assert_eq!(created.month() as i32, 2);
        assert_eq!(created.year(), 2000);
        let updated = whois.updated.expect("created date");
        assert_eq!(updated.day(), 23);
        assert_eq!(updated.month() as i32, 1);
        assert_eq!(updated.year(), 2025);
        assert_eq!(whois.registrar.as_deref(), Some("REG466-NORID"));
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
% By looking up information in the domain registration directory\r
[...]\r
\r
% No match\r
";
        let whois = (NO.map_response)(RESP);
        assert!(whois.is_none());
    }
}
