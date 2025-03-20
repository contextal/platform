use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const GRS_SERVER: &str = "whois.verisign-grs.com";

pub const GRS: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response: |resp: &str| -> Option<DomainInfo> {
        static DOMAIN: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Domain Name:"));
        static WHOIS: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Registrar WHOIS Server:"));
        static CREATED: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Creation Date:"));
        static UPDATED: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Updated Date:"));
        static EXPIRY: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Registry Expiry Date:"));
        static REGISTRAR: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Registrar:"));
        static STATUS: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_ws!(r"\s*?Domain Status:"));
        static NS: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Name Server:"));

        DOMAIN.captures(resp)?;
        let mut res = DomainInfo::default();
        res.whois = WHOIS.captures(resp).map(|cap| cap[1].trim().to_string());
        res.created = CREATED
            .captures(resp)
            .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
        res.updated = UPDATED
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
        let nss: Vec<String> = NS
            .captures_iter(resp)
            .map(|cap| cap[1].trim().to_string())
            .collect();
        res.nss = if nss.is_empty() { None } else { Some(nss) };
        Some(res)
    },
};

pub const GTLD: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response: |resp: &str| -> Option<DomainInfo> {
        static DOMAIN: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Domain Name:"));
        static WHOIS: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Registrar WHOIS Server:"));
        static CREATED: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Creation Date:"));
        static UPDATED: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Updated Date:"));
        static EXPIRY: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            capture_til_eol!(r"(?:Registry Expiry Date|Registrar Registration Expiration Date):")
        });
        static REGISTRAR: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Registrar:"));
        static STATUS: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_ws!("Domain Status:"));
        static NS: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Name Server:"));
        static REGISTRANT_NAME: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Registrant Name:"));
        static REGISTRANT_ORG: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Registrant Organization:"));
        static ADMIN_NAME: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Admin Name:"));
        static ADMIN_ORG: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Admin Organization:"));
        static TECH_NAME: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Tech Name:"));
        static TECH_ORG: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| capture_til_eol!("Tech Organization:"));

        DOMAIN.captures(resp)?;
        let mut res = DomainInfo::default();
        res.whois = WHOIS.captures(resp).map(|cap| cap[1].trim().to_string());
        res.created = CREATED
            .captures(resp)
            .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
        res.updated = UPDATED
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
        res.admin_name = ADMIN_NAME
            .captures(resp)
            .map(|cap| cap[1].trim().to_string());
        res.admin_org = ADMIN_ORG
            .captures(resp)
            .map(|cap| cap[1].trim().to_string());
        res.tech_name = TECH_NAME
            .captures(resp)
            .map(|cap| cap[1].trim().to_string());
        res.tech_org = TECH_ORG.captures(resp).map(|cap| cap[1].trim().to_string());
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
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grs_found() {
        const RESP: &str = "   \
   Domain Name: GOOGLE.COM\r
   Registry Domain ID: 2138514_DOMAIN_COM-VRSN\r
   Registrar WHOIS Server: whois.markmonitor.com\r
   Registrar URL: http://www.markmonitor.com\r
   Updated Date: 2019-09-09T15:39:04Z\r
   Creation Date: 1997-09-15T04:00:00Z\r
   Registry Expiry Date: 2028-09-14T04:00:00Z\r
   Registrar: MarkMonitor Inc.\r
   Registrar IANA ID: 292\r
   Registrar Abuse Contact Email: abusecomplaints@markmonitor.com\r
   Registrar Abuse Contact Phone: +1.2086851750\r
   Domain Status: clientDeleteProhibited https://icann.org/epp#clientDeleteProhibited\r
   Domain Status: clientTransferProhibited https://icann.org/epp#clientTransferProhibited\r
   Domain Status: clientUpdateProhibited https://icann.org/epp#clientUpdateProhibited\r
   Domain Status: serverDeleteProhibited https://icann.org/epp#serverDeleteProhibited\r
   Domain Status: serverTransferProhibited https://icann.org/epp#serverTransferProhibited\r
   Domain Status: serverUpdateProhibited https://icann.org/epp#serverUpdateProhibited\r
   Name Server: NS1.GOOGLE.COM\r
   Name Server: NS2.GOOGLE.COM\r
   Name Server: NS3.GOOGLE.COM\r
   Name Server: NS4.GOOGLE.COM\r
   DNSSEC: unsigned\r
   URL of the ICANN Whois Inaccuracy Complaint Form: https://www.icann.org/wicf/\r
>>> Last update of whois database: 2025-03-04T12:59:39Z <<<\r
\r
For more information on Whois status codes, please visit https://icann.org/epp\r
[...]
";
        let whois = (GRS.map_response)(RESP).expect("mapping");
        assert_eq!(whois.whois.as_deref(), Some("whois.markmonitor.com"));
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 15);
        assert_eq!(created.month() as i32, 9);
        assert_eq!(created.year(), 1997);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 9);
        assert_eq!(updated.month() as i32, 9);
        assert_eq!(updated.year(), 2019);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 14);
        assert_eq!(expiry.month() as i32, 9);
        assert_eq!(expiry.year(), 2028);
        assert_eq!(whois.registrar.as_deref(), Some("MarkMonitor Inc."));
        assert_eq!(
            whois.status.as_deref(),
            Some(
                "clientDeleteProhibited,clientTransferProhibited,clientUpdateProhibited,serverDeleteProhibited,serverTransferProhibited,serverUpdateProhibited"
            )
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "NS1.GOOGLE.COM");
        assert_eq!(nss[3], "NS4.GOOGLE.COM");
        assert_eq!(nss.len(), 4);
    }

    #[test]
    fn grs_not_found() {
        const RESP: &str = "\
No match for \"24334293249432.COM\".\r
>>> Last update of whois database: 2025-03-04T13:38:57Z <<<\r
\r
NOTICE: The expiration date displayed in this record is the date the\r
[...]
";
        let whois = (GRS.map_response)(RESP);
        assert!(whois.is_none());
    }

    #[test]
    fn gtld_found() {
        const RESP: &str = "\
Domain Name: contextal.com\r
Registry Domain ID: 2740123807_DOMAIN_COM-VRSN\r
Registrar WHOIS Server: whois.godaddy.com\r
Registrar URL: https://www.godaddy.com\r
Updated Date: 2023-09-25T05:15:54Z\r
Creation Date: 2022-11-22T11:24:36Z\r
Registrar Registration Expiration Date: 2025-11-22T11:24:36Z\r
Registrar: GoDaddy.com, LLC\r
Registrar IANA ID: 146\r
Registrar Abuse Contact Email: abuse@godaddy.com\r
Registrar Abuse Contact Phone: +1.4806242505\r
Domain Status: clientTransferProhibited https://icann.org/epp#clientTransferProhibited\r
Domain Status: clientUpdateProhibited https://icann.org/epp#clientUpdateProhibited\r
Domain Status: clientRenewProhibited https://icann.org/epp#clientRenewProhibited\r
Domain Status: clientDeleteProhibited https://icann.org/epp#clientDeleteProhibited\r
Registry Registrant ID: Not Available From Registry\r
Registrant Name: Registration Private\r
Registrant Organization: Domains By Proxy, LLC\r
Registrant Street: DomainsByProxy.com\r
Registrant Street: 100 S. Mill Ave, Suite 1600\r
Registrant City: Tempe\r
Registrant State/Province: Arizona\r
Registrant Postal Code: 85281\r
Registrant Country: US\r
Registrant Phone: +1.4806242599\r
Registrant Phone Ext:\r
Registrant Fax: \r
Registrant Fax Ext:\r
Registrant Email: Select Contact Domain Holder link at https://www.godaddy.com/whois/results.aspx?domain=contextal.com\r
Registry Tech ID: Not Available From Registry\r
Admin Name: GDPR Masked (admin name)\r
Admin Organization: GDPR Masked (admin org)\r
Admin Street: GDPR Masked\r
Admin City: GDPR Masked\r
Admin State/Province: GDPR Masked\r
Admin Postal Code: GDPR Masked\r
Admin Country: GDPR Masked\r
Admin Phone: GDPR Masked\r
Admin Phone Ext: \r
Admin Fax: GDPR Masked\r
Admin Fax Ext: \r
Admin Email: gdpr-masking@gdpr-masked.com\r
Tech Name: Registration Private (tech)\r
Tech Organization: Domains By Proxy, LLC (tech)\r
Tech Street: DomainsByProxy.com\r
Tech Street: 100 S. Mill Ave, Suite 1600\r
Tech City: Tempe\r
Tech State/Province: Arizona\r
Tech Postal Code: 85281\r
Tech Country: US\r
Tech Phone: +1.4806242599\r
Tech Phone Ext:\r
Tech Fax: \r
Tech Fax Ext:\r
Tech Email: Select Contact Domain Holder link at https://www.godaddy.com/whois/results.aspx?domain=contextal.com\r
Name Server: NS25.DOMAINCONTROL.COM\r
Name Server: NS26.DOMAINCONTROL.COM\r
DNSSEC: unsigned\r
URL of the ICANN WHOIS Data Problem Reporting System: http://wdprs.internic.net/\r
>>> Last update of WHOIS database: 2025-03-04T16:15:53Z <<<\r
For more information on Whois status codes, please visit https://icann.org/epp\r
[...]
";
        let whois = (GTLD.map_response)(RESP).expect("mapping");
        assert_eq!(whois.whois.as_deref(), Some("whois.godaddy.com"));
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 22);
        assert_eq!(created.month() as i32, 11);
        assert_eq!(created.year(), 2022);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 25);
        assert_eq!(updated.month() as i32, 9);
        assert_eq!(updated.year(), 2023);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 22);
        assert_eq!(expiry.month() as i32, 11);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.registrar.as_deref(), Some("GoDaddy.com, LLC"));
        assert_eq!(
            whois.status.as_deref(),
            Some(
                "clientTransferProhibited,clientUpdateProhibited,clientRenewProhibited,clientDeleteProhibited"
            )
        );
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("Registration Private")
        );
        assert_eq!(
            whois.registrant_org.as_deref(),
            Some("Domains By Proxy, LLC")
        );
        assert_eq!(
            whois.admin_name.as_deref(),
            Some("GDPR Masked (admin name)")
        );
        assert_eq!(whois.admin_org.as_deref(), Some("GDPR Masked (admin org)"));
        assert_eq!(
            whois.tech_name.as_deref(),
            Some("Registration Private (tech)")
        );
        assert_eq!(
            whois.tech_org.as_deref(),
            Some("Domains By Proxy, LLC (tech)")
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "NS25.DOMAINCONTROL.COM");
        assert_eq!(nss[1], "NS26.DOMAINCONTROL.COM");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn gtld_not_found() {
        const RESP: &str = "Whois Error: No Match for for \"24334293249432.COM\"\r\n";
        let whois = (GTLD.map_response)(RESP);
        assert!(whois.is_none());
    }
}
