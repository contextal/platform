use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const CN: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain Name:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Registration Time:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Expiration Time:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain Status:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Sponsoring Registrar:"));
    static REGISTRANT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registrant:"));
    static ADMIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Admin Name:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Name Server:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.status = STATUS
        .captures_iter(resp)
        .map(|cap| cap[1].trim().to_string())
        .reduce(|acc, s| acc + "," + s.as_str());
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.registrant_name = REGISTRANT
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.admin_name = ADMIN.captures(resp).map(|cap| cap[1].trim().to_string());
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
Domain Name: china.org.cn\r
ROID: 20021209s10051s00002118-cn\r
Domain Status: clientDeleteProhibited\r
Domain Status: clientUpdateProhibited\r
Domain Status: clientTransferProhibited\r
Registrant: 中国互联网新闻中心\r
Registrant Contact Email: songzh@china.org.cn\r
Sponsoring Registrar: 北京国科云计算技术有限公司（原北京中科三方网络技术有限公司）\r
Name Server: cl1.sfndns.cn\r
Name Server: cl2.sfndns.cn\r
Name Server: cl1.sfndns.com\r
Name Server: cl2.sfndns.com\r
Name Server: ns1.china.org.cn\r
Name Server: ns1.china-online.com.cn\r
Registration Time: 1997-04-22 00:00:00\r
Expiration Time: 2029-07-01 00:00:00\r
DNSSEC: unsigned\r
";
        let whois = (CN.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 22);
        assert_eq!(created.month() as i32, 4);
        assert_eq!(created.year(), 1997);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 1);
        assert_eq!(expiry.month() as i32, 7);
        assert_eq!(expiry.year(), 2029);
        assert_eq!(
            whois.status.as_deref(),
            Some("clientDeleteProhibited,clientUpdateProhibited,clientTransferProhibited")
        );
        assert_eq!(
            whois.registrar.as_deref(),
            Some("北京国科云计算技术有限公司（原北京中科三方网络技术有限公司）")
        );
        assert_eq!(whois.registrant_name.as_deref(), Some("中国互联网新闻中心"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "cl1.sfndns.cn");
        assert_eq!(nss[5], "ns1.china-online.com.cn");
        assert_eq!(nss.len(), 6);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "No matching record.\r\n";
        let whois = (CN.map_response)(RESP);
        assert!(whois.is_none());
    }
}
