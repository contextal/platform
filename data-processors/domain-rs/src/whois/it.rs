use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const IT: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};
fn map_response(resp: &str) -> Option<DomainInfo> {
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Created:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Expire Date:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("Last Update:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!("Nameservers"));
    static SECTIONS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        capture_section!("(Registrant|Registrar|Admin Contact|Technical Contacts)")
    });
    static NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"^\s+?Name:"));
    static ORG: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"^\s+?Organization:"));

    let mut res = DomainInfo::default();
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    if res.status.as_deref() == Some("AVAILABLE") {
        return None;
    }
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());

    for section in SECTIONS.captures_iter(resp) {
        let title = section[1].trim();
        match title {
            "Registrar" => {
                res.registrar = ORG
                    .captures(&section[2])
                    .map(|cap| cap[1].trim().to_string());
            }
            "Registrant" => {
                res.registrant_org = ORG
                    .captures(&section[2])
                    .map(|cap| cap[1].trim().to_string());
                res.registrant_name = NAME
                    .captures(&section[2])
                    .map(|cap| cap[1].trim().to_string());
            }
            "Admin Contact" => {
                res.admin_org = ORG
                    .captures(&section[2])
                    .map(|cap| cap[1].trim().to_string());
                res.admin_name = NAME
                    .captures(&section[2])
                    .map(|cap| cap[1].trim().to_string());
            }
            "Technical Contacts" => {
                res.tech_org = ORG
                    .captures(&section[2])
                    .map(|cap| cap[1].trim().to_string());
                res.tech_name = NAME
                    .captures(&section[2])
                    .map(|cap| cap[1].trim().to_string());
            }
            _ => {}
        }
    }

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
*********************************************************************\r
* Please note that the following result could be a subgroup of      *\r
* the data contained in the database.                               *\r
*                                                                   *\r
* Additional information can be visualized at:                      *\r
* http://web-whois.nic.it                                           *\r
*********************************************************************\r
\r
Domain:             wikipedia.it\r
Status:             ok\r
Signed:             no\r
Created:            2003-03-04 00:00:00\r
Last Update:        2024-07-10 00:48:57\r
Expire Date:        2025-06-24\r
\r
Registrant\r
  Organization:     Associazione Wikipedia Italia\r
  Address:          Via Flaming, 49\r
                    Roma\r
                    00191\r
                    RM\r
                    IT\r
  Created:          2007-03-01 10:41:48\r
  Last Update:      2010-08-20 12:50:36\r
\r
Admin Contact\r
  Name:             admin name\r
  Organization:     admin org\r
\r
Technical Contacts\r
  Name:             tech name\r
  Organization:     tech org\r
\r
Registrar\r
  Organization:     Yepa S.r.l.\r
  Name:             YEPA-REG\r
  DNSSEC:           no\r
\r
\r
Nameservers\r
  ns0.yepa.com\r
  ns1.yepa.com\r
";
        let whois = (IT.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 4);
        assert_eq!(created.month() as i32, 3);
        assert_eq!(created.year(), 2003);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 24);
        assert_eq!(expiry.month() as i32, 06);
        assert_eq!(expiry.year(), 2025);
        let updated = whois.updated.expect("update date");
        assert_eq!(updated.day(), 10);
        assert_eq!(updated.month() as i32, 7);
        assert_eq!(updated.year(), 2024);
        assert_eq!(whois.registrar.as_deref(), Some("Yepa S.r.l."));
        assert!(whois.registrant_name.is_none());
        assert_eq!(
            whois.registrant_org.as_deref(),
            Some("Associazione Wikipedia Italia")
        );
        assert_eq!(whois.admin_name.as_deref(), Some("admin name"));
        assert_eq!(whois.admin_org.as_deref(), Some("admin org"));
        assert_eq!(whois.tech_name.as_deref(), Some("tech name"));
        assert_eq!(whois.tech_org.as_deref(), Some("tech org"));
        assert_eq!(whois.status.as_deref(), Some("ok"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns0.yepa.com");
        assert_eq!(nss[1], "ns1.yepa.com");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
Domain:             ??????.it\r
Status:             AVAILABLE\r
";
        let whois = (IT.map_response)(RESP);
        assert!(whois.is_none());
    }
}
