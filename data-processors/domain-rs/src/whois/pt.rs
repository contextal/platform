use super::macros::*;
use crate::DomainInfo;

pub const PT: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] = time::macros::format_description!(
        "[day padding:zero]/[month repr:numerical padding:zero]/[year]"
    );
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Creation Date:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Expiration Date:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain Status:"));
    static OWNER: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Owner Name:"));
    static ADMIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Admin Name:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Name Server:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    res.registrant_name = OWNER.captures(resp).map(|cap| cap[1].trim().to_string());
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
Domain: tap.pt\r
Domain Status: Registered\r
Creation Date: 14/01/1997 00:00:00\r
Expiration Date: 20/07/2025 23:59:42\r
Owner Name: Transportes Aereos Portugueses SA\r
Owner Address: Aeroporto de Lisboa Edificio 19\r
Owner Locality: Lisboa\r
Owner ZipCode: 1704-801\r
Owner Locality ZipCode: Lisboa\r
Owner Country Code: PT\r
Owner Email: dnsadmin@tap.pt\r
Admin Name: Eurodns S.A.\r
Admin Address: rue Léon Laval 24\r
Admin Locality: Leudelange\r
Admin ZipCode: 3372\r
Admin Locality ZipCode: Leudelange\r
Admin Country Code: LU\r
Admin Email: dnspt@admin.eurodns.com\r
Name Server: ns1-04.azure-dns.com | IPv4:  and IPv6: \r
Name Server: ns4-04.azure-dns.info | IPv4:  and IPv6: \r
Name Server: ns2-04.azure-dns.net | IPv4:  and IPv6: \r
Name Server: ns3-04.azure-dns.org | IPv4:  and IPv6: \r
";
        let whois = (PT.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 14);
        assert_eq!(created.month() as i32, 1);
        assert_eq!(created.year(), 1997);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 20);
        assert_eq!(expiry.month() as i32, 7);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.status.as_deref(), Some("Registered"));
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("Transportes Aereos Portugueses SA")
        );
        assert_eq!(whois.admin_name.as_deref(), Some("Eurodns S.A."));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1-04.azure-dns.com");
        assert_eq!(nss[3], "ns3-04.azure-dns.org");
        assert_eq!(nss.len(), 4);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "????.pt - No Match\r\n";
        let whois = (PT.map_response)(RESP);
        assert!(whois.is_none());
    }
}
