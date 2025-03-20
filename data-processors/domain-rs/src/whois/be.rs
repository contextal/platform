use super::macros::*;
use crate::DomainInfo;

pub const BE: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] = time::macros::format_description!(
        "[weekday repr:short case_sensitive:false] [month repr:short case_sensitive:false] [day padding:none] [year]"
    );

    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registered:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!("Nameservers:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Registrar:.*$\n\s+?Name:"));
    static TECH: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        capture_til_eol!(r"Registrar Technical Contacts:.*$\n\s+?Organisation:")
    });

    let mut res = DomainInfo::default();
    res.status = STATUS
        .captures(resp)
        .map(|cap| cap[1].trim().trim().to_string());
    if res.status.as_deref() == Some("AVAILABLE") {
        return None;
    }
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.tech_org = TECH.captures(resp).map(|cap| cap[1].trim().to_string());
    res.nss = NS.captures(resp).map(|cap| {
        cap[1]
            .lines()
            .filter_map(|line| {
                let l = line.trim();
                if l.is_empty() {
                    None
                } else {
                    Some(l.trim().to_string())
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
% .be Whois Server 6.1\r
%\r
[...]\r
%\r
\r
Domain:	wikipedia.be\r
Status:	NOT AVAILABLE\r
Registered:	Fri Oct 25 2002\r
\r
Registrant:\r
	Not shown, please visit www.dnsbelgium.be for webbased whois.\r
\r
Registrar Technical Contacts:\r
	Organisation:	One.com A/S\r
	Language:	en\r
	Phone:	+45.46907100\r
\r
\r
Registrar:\r
	Name:	One.com A/S\r
	Website:	https://www.one.com\r
\r
Nameservers:\r
	ns01.one.com\r
	ns02.one.com\r
\r
Keys:\r
	keyTag:28466 flags:KSK protocol:3 algorithm:ECDSAP256SHA256 pubKey:RFDscOGNg1A6W7Us6Diarkd/2hallg4VZKgCxvTvN2C4qlbjaOkawWDcv7jTfO+aIpOCB0mDajU14FMwSclYAg==\r
\r
Flags:\r
\r
Please visit www.dnsbelgium.be for more info.\r
";
        let whois = (BE.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 25);
        assert_eq!(created.month() as i32, 10);
        assert_eq!(created.year(), 2002);
        assert_eq!(whois.registrar.as_deref(), Some("One.com A/S"));
        assert_eq!(whois.tech_org.as_deref(), Some("One.com A/S"));
        assert_eq!(whois.status.as_deref(), Some("NOT AVAILABLE"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns01.one.com");
        assert_eq!(nss[1], "ns02.one.com");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
[...]
% protect the privacy of its registrants or the integrity of the database.\r
%\r
\r
Domain:	?????.be\r
Status:	AVAILABLE\r
";
        let whois = (BE.map_response)(RESP);
        assert!(whois.is_none());
    }
}
