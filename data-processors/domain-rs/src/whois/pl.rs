use super::macros::*;
use crate::DomainInfo;

pub const PL: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] =
        time::macros::format_description!("[year].[month padding:zero].[day padding:zero]");
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("DOMAIN NAME:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("created:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("last modified:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("renewal date:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"REGISTRAR:.*$\n"));
    static NS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?m)^nameservers:((?s).*?)^[^\s]").unwrap()
    });

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
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
                    l.split_whitespace().next().map(|l| l.to_string())
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
DOMAIN NAME:                    torun.pl\r
registrant type:                organization\r
nameservers:                    bilbo.nask.org.pl. [195.187.245.51]\r
                                blackbox.uci.uni.torun.pl. [158.75.1.5]\r
                                flis.man.torun.pl. [158.75.33.142]\r
                                koala.uci.uni.torun.pl. [158.75.1.4]\r
created:                        1995.01.01 12:00:00\r
last modified:                  2024.12.13 19:28:10\r
renewal date:                   2027.12.31 13:00:00\r
\r
option:                         no\r
\r
dnssec:                         Unsigned\r
\r
REGISTRAR:\r
cyber_Folks S.A.\r
ul. Wierzbięcice 1B\r
61-569 Poznań\r
Polska/Poland\r
Tel: +48.122963663\r
https://cyberfolks.pl/\r
domeny@cyberfolks.pl\r
\r
WHOIS database responses:       https://dns.pl/en/whois\r
\r
WHOIS displays data with a delay not exceeding 15 minutes in relation to the .pl Registry system\r
\r
";
        let whois = (PL.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 1);
        assert_eq!(created.month() as i32, 1);
        assert_eq!(created.year(), 1995);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 13);
        assert_eq!(updated.month() as i32, 12);
        assert_eq!(updated.year(), 2024);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 31);
        assert_eq!(expiry.month() as i32, 12);
        assert_eq!(expiry.year(), 2027);
        assert_eq!(whois.registrar.as_deref(), Some("cyber_Folks S.A."));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "bilbo.nask.org.pl.");
        assert_eq!(nss[2], "flis.man.torun.pl.");
        assert_eq!(nss[3], "koala.uci.uni.torun.pl.");
        assert_eq!(nss.len(), 4);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
No information available about domain name ?????.pl in the Registry NASK database.\r
\r
WHOIS database responses:       https://dns.pl/en/whois\r
\r
WHOIS displays data with a delay not exceeding 15 minutes in relation to the .pl Registry system\r
\r
";
        let whois = (PL.map_response)(RESP);
        assert!(whois.is_none());
    }
}
