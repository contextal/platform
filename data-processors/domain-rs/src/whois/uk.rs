use super::macros::*;
use crate::DomainInfo;

pub const UK: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] = time::macros::format_description!(
        "[day padding:zero]-[month repr:short case_sensitive:false]-[year]"
    );
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\s+?Domain name:.*$\n"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\s+?Registered on:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\s+?Expiry date:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\s+?Last updated:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\s+?Registrar:.*$\n"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"^\s+?Registration status:.*$\n"));
    static NSS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!(r"\s+?Name servers:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED.captures(resp).and_then(|cap| {
        let created = cap[1].trim();
        if created.starts_with("before ") {
            time::Date::parse("01-Aug-1996", &DATEPARSEFMT).ok()
        } else {
            time::Date::parse(created, &DATEPARSEFMT).ok()
        }
    });
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.nss = NSS.captures(resp).map(|cap| {
        cap[1]
            .lines()
            .filter_map(|line| line.trim().split_whitespace().next().map(|l| l.to_string()))
            .collect()
    });
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn found() {
        const RESP: &str = "
\r
    Domain name:\r
        facebook.co.uk\r
\r
    Data validation:\r
        The registrar is responsible for having checked these contact details\r
\r
    Registrar:\r
        Gandi [Tag = GANDI]\r
        URL: https://www.gandi.net\r
\r
    Relevant dates:\r
        Registered on: 30-Dec-2004\r
        Expiry date:  30-Dec-2025\r
        Last updated:  26-Nov-2024\r
\r
    Registration status:\r
        Registered until expiry date.\r
\r
    Name servers:\r
        dns101.register.com        1.2.3.4   1111:2222::33\r
        dns102.register.com\r
\r
    WHOIS lookup made at 22:42:41 04-Mar-2025\r
\r
-- \r
This WHOIS information is provided for free by Nominet UK the central registry\r
for .uk domain names. This information and the .uk WHOIS are:\r
\r
    Copyright Nominet UK 1996 - 2025.\r
";
        let whois = (UK.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 30);
        assert_eq!(created.month() as i32, 12);
        assert_eq!(created.year(), 2004);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 30);
        assert_eq!(expiry.month() as i32, 12);
        assert_eq!(expiry.year(), 2025);
        let updated = whois.updated.expect("update date");
        assert_eq!(updated.day(), 26);
        assert_eq!(updated.month() as i32, 11);
        assert_eq!(updated.year(), 2024);
        assert_eq!(whois.registrar.as_deref(), Some("Gandi [Tag = GANDI]"));
        assert_eq!(
            whois.status.as_deref(),
            Some("Registered until expiry date.")
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "dns101.register.com");
        assert_eq!(nss[1], "dns102.register.com");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "
    No match for \"????.co.uk\".\r
\r
    This domain name has not been registered.\r
\r
    WHOIS lookup made at 00:09:36 05-Mar-2025\r
\r
-- \r
This WHOIS information is provided for free by Nominet UK the central registry\r
for .uk domain names. This information and the .uk WHOIS are:\r
[...]
";
        let whois = (UK.map_response)(RESP);
        assert!(whois.is_none());
    }
}
