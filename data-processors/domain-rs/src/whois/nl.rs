use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const NL: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Creation Date:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Updated Date:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!("Domain nameservers:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Registrar:.*$\n"));
    let mut res = DomainInfo::default();
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    res.status.as_ref()?;
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.nss = NS.captures(resp).map(|cap| {
        cap[1]
            .lines()
            .filter_map(|line| {
                let l = line.trim();
                if l.is_empty() {
                    None
                } else {
                    Some(l.to_string())
                }
            })
            .collect()
    });
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn found() {
        const RESP: &str = "\
Domain name: 1337.nl\r
Status:      active\r
\r
Registrar:\r
   Internet Service Europe B.V.\r
   Rucphensebaan 30 A\r
   4706PJ ROOSENDAAL\r
   Netherlands\r
\r
DNSSEC:      no\r
\r
Domain nameservers:\r
   ns1.administratiemenu.nl\r
   ns2.administratiemenu.nl\r
\r
Creation Date: 2008-06-27\r
\r
Updated Date: 2019-01-26\r
\r
Record maintained by: SIDN BV\r
";
        let whois = (NL.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 27);
        assert_eq!(created.month() as i32, 6);
        assert_eq!(created.year(), 2008);
        let updated = whois.updated.expect("created date");
        assert_eq!(updated.day(), 26);
        assert_eq!(updated.month() as i32, 1);
        assert_eq!(updated.year(), 2019);
        assert_eq!(
            whois.registrar.as_deref(),
            Some("Internet Service Europe B.V.")
        );
        assert_eq!(whois.status.as_deref(), Some("active"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1.administratiemenu.nl");
        assert_eq!(nss[1], "ns2.administratiemenu.nl");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "??????????????.nl is free\r\n";
        let whois = (NL.map_response)(RESP);
        assert!(whois.is_none());
    }
}
