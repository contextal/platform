use super::macros::*;
use crate::DomainInfo;

pub const JP: &super::TldWhois = &super::TldWhois {
    get_query_string: Some(get_query_string),
    map_response,
};

fn get_query_string(ascii_domain: &str) -> String {
    format!("{}/e\r\n", ascii_domain)
}

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] = time::macros::format_description!(
        "[year]/[month repr:numerical padding:zero]/[day padding:zero]"
    );
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\[Domain Name\]"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\[Created on\]"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!(r"\[Last Updated\]"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\[Expires on\]"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\[Status\]"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\[Name Server\]"));
    static REGISTRANT_NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\[Registrant\]"));

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
    res.registrant_name = REGISTRANT_NAME
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
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
[ JPRS database provides information on network administration. Its use is    ]\r
[ restricted to network administration purposes. For further information,     ]\r
[ use 'whois -h whois.jprs.jp help'. To suppress Japanese output, add'/e'     ]\r
[ at the end of command, e.g. 'whois -h whois.jprs.jp xxx/e'.                 ]\r
Domain Information:\r
[Domain Name]                   HPMMUSEUM.JP\r
\r
[Registrant]                    Hiroshima Peace Memorial Museum\r
\r
[Name Server]                   ns1.gslb13.sakura.ne.jp\r
[Name Server]                   ns2.gslb13.sakura.ne.jp\r
[Signing Key]                   \r
\r
[Created on]                    2024/05/30\r
[Expires on]                    2025/05/31\r
[Status]                        Active\r
[Last Updated]                  2024/12/12 07:17:19 (JST)\r
\r
Contact Information:\r
[Name]                          SAKURA internet Inc.\r
[Email]                         nic-staff@sakura.ad.jp\r
[Web Page]                       \r
[Postal code]                   530-0001\r
[Postal Address]                Osaka\r
                                Osaka\r
                                11F,1-12-12,Umeda,Kita-ku\r
[Phone]                         06-6476-8790\r
[Fax]                           \r
\r
";
        let whois = (JP.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 30);
        assert_eq!(created.month() as i32, 5);
        assert_eq!(created.year(), 2024);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 12);
        assert_eq!(updated.month() as i32, 12);
        assert_eq!(updated.year(), 2024);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 31);
        assert_eq!(expiry.month() as i32, 5);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.status.as_deref(), Some("Active"));
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("Hiroshima Peace Memorial Museum")
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1.gslb13.sakura.ne.jp");
        assert_eq!(nss[1], "ns2.gslb13.sakura.ne.jp");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
[ JPRS database provides information on network administration. Its use is    ]\r
[ restricted to network administration purposes. For further information,     ]\r
[ use 'whois -h whois.jprs.jp help'. To suppress Japanese output, add'/e'     ]\r
[ at the end of command, e.g. 'whois -h whois.jprs.jp xxx/e'.                 ]\r
No match!!\r
\r
With JPRS WHOIS, you can query the following domain name information\r
sponsored by JPRS.\r
    - All of registered JP domain name\r
    - gTLD domain name of which sponsoring registrar is JPRS\r
Detail: https://jprs.jp/about/dom-search/jprs-whois/ (only in Japanese)\r
\r
For IP address information, please refer to the following WHOIS servers:\r
    - JPNIC WHOIS (whois.nic.ad.jp)\r
    - APNIC WHOIS (whois.apnic.net)\r
    - ARIN WHOIS (whois.arin.net)\r
    - RIPE WHOIS (whois.ripe.net)\r
    - LACNIC WHOIS (whois.lacnic.net)\r
    - AfriNIC WHOIS (whois.afrinic.net)\r
\r
";
        let whois = (JP.map_response)(RESP);
        assert!(whois.is_none());
    }
}
