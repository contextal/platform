use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const AU: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain Name:"));
    static WHOIS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registrar WHOIS Server:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Last Modified:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registrar Name:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Name Server:"));
    static REGISTRANT_NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registrant:"));
    static ADMIN_NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Registrant Contact Name:"));
    static TECH_NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Tech Contact Name:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.whois = WHOIS.captures(resp).map(|cap| cap[1].trim().to_string());
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.registrant_name = REGISTRANT_NAME
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.admin_name = ADMIN_NAME
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.tech_name = TECH_NAME
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.status = STATUS
        .captures_iter(resp)
        .map(|cap| cap[1].trim().to_string())
        .reduce(|acc, s| acc + "," + s.as_str());
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
Domain Name: news.com.au\r
Registry Domain ID: 6b5aaf82756343e5aa9842f050d38f81-AU\r
Registrar WHOIS Server: whois.auda.org.au\r
Registrar URL: https://www.cscdigitalbrand.services\r
Last Modified: 2024-07-06T05:05:35Z\r
Registrar Name: Corporation Service Company (Aust) Pty Ltd\r
Registrar Abuse Contact Email: domainabuse@cscglobal.com\r
Registrar Abuse Contact Phone: +1.8887802723\r
Reseller Name: \r
Status: clientDeleteProhibited https://identitydigital.au/get-au/whois-status-codes#clientDeleteProhibited\r
Status: serverDeleteProhibited https://identitydigital.au/get-au/whois-status-codes#serverDeleteProhibited\r
Status Reason: Registry Lock\r
Status: serverRenewProhibited https://identitydigital.au/get-au/whois-status-codes#serverRenewProhibited\r
Status Reason: Not Currently Eligible For Renewal\r
Status: serverTransferProhibited https://identitydigital.au/get-au/whois-status-codes#serverTransferProhibited\r
Status Reason: Registry Lock\r
Status: serverUpdateProhibited https://identitydigital.au/get-au/whois-status-codes#serverUpdateProhibited\r
Status Reason: Registry Lock\r
Registrant Contact ID: 6015c5ca7b144237a037ba9d357db8cc-AU\r
Registrant Contact Name: Domain Admin\r
Tech Contact ID: 9ecb3a55266f4718b5385d77ca604e2c-AU\r
Tech Contact Name: News Limited  Domain Manager\r
Name Server: asia1.akam.net\r
Name Server: ns1-24.akam.net\r
Name Server: ns1-50.akam.net\r
Name Server: usc1.akam.net\r
Name Server: usc4.akam.net\r
Name Server: usw1.akam.net\r
DNSSEC: unsigned\r
Registrant: News Life Media Pty Ltd\r
Registrant ID: ABN 57088923906\r
Eligibility Type: Company\r
>>> Last update of WHOIS database: 2025-03-06T13:40:50Z <<<\r
\r
Identity Digital Australia Pty Ltd, for itself and on behalf of .au Domain Administration Limited (auDA), makes the WHOIS registration data directory service (WHOIS Service) available solely for the purposes of:\r
\r
(a) querying the availability of a domain name licence;\r
\r
(b) identifying the holder of a domain name licence; and/or\r
\r
(c) contacting the holder of a domain name licence in relation to that domain name and its use.\r
\r
The WHOIS Service must not be used for any other purpose (even if that purpose is lawful), including:\r
\r
(a) aggregating, collecting or compiling information from the WHOIS database, whether for personal or commercial purposes;\r
\r
(b) enabling the sending of unsolicited electronic communications; and / or\r
\r
(c) enabling high volume, automated, electronic processes that send queries or data to the systems of Afilias, any registrar, any domain name licence holder, or auDA.\r
\r
The WHOIS Service is provided for information purposes only. By using the WHOIS Service, you agree to be bound by these terms and conditions. The WHOIS Service is operated in\r
accordance with the auDA WHOIS Policy (available at https://www.auda.org.au/policy/2014-07-whois-policy).\r
Domain Name: contextal.com\r
";
        let whois = (AU.map_response)(RESP).expect("mapping");
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 6);
        assert_eq!(updated.month() as i32, 7);
        assert_eq!(updated.year(), 2024);
        assert_eq!(
            whois.registrar.as_deref(),
            Some("Corporation Service Company (Aust) Pty Ltd")
        );
        assert_eq!(
            whois.status.as_deref(),
            Some(
                "clientDeleteProhibited,serverDeleteProhibited,serverRenewProhibited,serverTransferProhibited,serverUpdateProhibited"
            )
        );
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("News Life Media Pty Ltd")
        );
        assert_eq!(whois.admin_name.as_deref(), Some("Domain Admin"));
        assert_eq!(
            whois.tech_name.as_deref(),
            Some("News Limited  Domain Manager")
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "asia1.akam.net");
        assert_eq!(nss[5], "usw1.akam.net");
        assert_eq!(nss.len(), 6);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
Domain not found.\r
>>> Last update of WHOIS database: 2025-03-06T14:07:48Z <<<\r
\r
Identity Digital Australia Pty Ltd, for itself and on behalf of .au Domain Administration Limited (auDA),\r
[...]
";
        let whois = (AU.map_response)(RESP);
        assert!(whois.is_none());
    }
}
