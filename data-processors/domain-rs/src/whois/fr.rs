use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const FR: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("domain:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("created:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Expiry Date:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("registrar:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("nserver:"));
    static HOLDER_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("holder-c:"));
    static ADMIN_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("admin-c:"));
    static TECH_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("tech-c:"));
    static HANDLES: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!(r"nic-hdl:.*?([^\s].*)"));
    static HDL_TYPE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("type:"));
    static HDL_CONTACT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("contact:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    let holder_c = HOLDER_C.captures(resp).map(|cap| cap[1].trim().to_string());
    let admin_c = ADMIN_C.captures(resp).map(|cap| cap[1].trim().to_string());
    let tech_c = TECH_C.captures(resp).map(|cap| cap[1].trim().to_string());
    for cap in HANDLES.captures_iter(resp) {
        let handle = cap[1].trim();
        let is_person = match HDL_TYPE.captures(&cap[0]) {
            Some(t) if t[1].trim() == "ORGANIZATION" => false,
            Some(t) if t[1].trim() == "PERSON" => true,
            _ => continue,
        };
        let contact = HDL_CONTACT
            .captures(&cap[0])
            .map(|s| s[1].trim().to_string());
        if holder_c.as_deref() == Some(handle) {
            if is_person {
                res.registrant_name = contact;
            } else {
                res.registrant_org = contact;
            }
        } else if admin_c.as_deref() == Some(handle) {
            if is_person {
                res.admin_name = contact;
            } else {
                res.admin_org = contact;
            }
        } else if tech_c.as_deref() == Some(handle) {
            if is_person {
                res.tech_name = contact;
            } else {
                res.tech_org = contact;
            }
        }
    }
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
%%\r
%% This is the AFNIC Whois server.\r
%%\r
%% complete date format: YYYY-MM-DDThh:mm:ssZ\r
%%\r
%% Rights restricted by copyright.\r
%% See https://www.afnic.fr/en/domain-names-and-support/everything-there-is-to-know-about-domain-names/find-a-domain-name-or-a-holder-using-whois/\r
%%\r
%%\r
\r
domain:                        gouv.fr\r
status:                        ACTIVE\r
eppstatus:                     inactive\r
hold:                          NO\r
holder-c:                      NA102-FRNIC\r
admin-c:                       NF100-FRNIC\r
tech-c:                        VL-FRNIC\r
registrar:                     Registry AutoRenew\r
Expiry Date:                   2098-12-31T23:00:00Z\r
created:                       1995-01-01T00:00:00Z\r
source:                        FRNIC\r
\r
nserver:                       ns1.nic.fr\r
nserver:                       ns2.nic.fr\r
nserver:                       ns3.nic.fr\r
source:                        FRNIC\r
\r
registrar:                     Registry AutoRenew\r
address:                       AFNIC\r
address:                       immeuble le Stephenson\r
address:                       1, rue Stephenson\r
address:                       78180 Montigny-Le-Bretonneux\r
country:                       FR\r
phone:                         +33.139308300\r
fax-no:                        +33.139308301\r
e-mail:                        testing@afnic.fr\r
anonymous:                     No\r
registered:                    2014-07-29T15:35:33Z\r
source:                        FRNIC\r
\r
nic-hdl:                       NF100-FRNIC\r
type:                          ORGANIZATION\r
contact:                       AFNIC (admin)\r
address:                       7, avenue du 8 mai 1945\r
address:                       78280 Guyancourt\r
country:                       FR\r
phone:                         +33.139308300\r
fax-no:                        +33.139308301\r
e-mail:                        juridique@afnic.fr\r
registrar:                     Registry Operations\r
anonymous:                     NO\r
obsoleted:                     NO\r
eppstatus:                     associated\r
eppstatus:                     active\r
eligstatus:                    ok\r
eligdate:                      2014-12-11T00:00:00Z\r
reachstatus:                   ok\r
reachmedia:                    email\r
source:                        FRNIC\r
\r
nic-hdl:                       NA102-FRNIC\r
type:                          ORGANIZATION\r
contact:                       AFNIC (holder)\r
address:                       7, avenue du 8 mai 1945\r
address:                       78280 Guyancourt\r
country:                       FR\r
phone:                         +33.139308300\r
fax-no:                        +33.139308301\r
e-mail:                        juridique@afnic.fr\r
registrar:                     Registry AutoRenew\r
anonymous:                     NO\r
obsoleted:                     NO\r
eppstatus:                     associated\r
eppstatus:                     active\r
eligstatus:                    ok\r
eligdate:                      2014-12-12T00:00:00Z\r
reachstatus:                   ok\r
reachmedia:                    email\r
source:                        FRNIC\r
\r
nic-hdl:                       VL-FRNIC\r
type:                          PERSON\r
contact:                       Vincent Levigneron\r
address:                       AFNIC\r
address:                       7, avenue du 8 mai 1945\r
address:                       78280 Guyancourt\r
country:                       FR\r
phone:                         +33.139308300\r
fax-no:                        +33.139308301\r
e-mail:                        vincent.levigneron@afnic.fr\r
registrar:                     Registry Operations\r
changed:                       2024-03-11T13:31:44.500207Z\r
anonymous:                     NO\r
obsoleted:                     NO\r
eppstatus:                     associated\r
eppstatus:                     active\r
eligstatus:                    not identified\r
reachstatus:                   not identified\r
source:                        FRNIC\r
\r
>>> Last update of WHOIS database: 2025-03-04T19:59:45.041518Z <<<\r
";
        let whois = (FR.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 1);
        assert_eq!(created.month() as i32, 1);
        assert_eq!(created.year(), 1995);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 31);
        assert_eq!(expiry.month() as i32, 12);
        assert_eq!(expiry.year(), 2098);
        assert_eq!(whois.registrar.as_deref(), Some("Registry AutoRenew"));
        assert_eq!(whois.status.as_deref(), Some("ACTIVE"));
        assert!(whois.registrant_name.is_none());
        assert_eq!(whois.registrant_org.as_deref(), Some("AFNIC (holder)"));
        assert!(whois.admin_name.is_none());
        assert_eq!(whois.admin_org.as_deref(), Some("AFNIC (admin)"));
        assert_eq!(whois.tech_name.as_deref(), Some("Vincent Levigneron"));
        assert!(whois.tech_org.is_none());
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1.nic.fr");
        assert_eq!(nss[1], "ns2.nic.fr");
        assert_eq!(nss[2], "ns3.nic.fr");
        assert_eq!(nss.len(), 3);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
%%\r
%% This is the AFNIC Whois server.\r
%%\r
%% complete date format: YYYY-MM-DDThh:mm:ssZ\r
%%\r
%% Rights restricted by copyright.\r
%% See https://www.afnic.fr/en/domain-names-and-support/everything-there-is-to-know-about-domain-names/find-a-domain-name-or-a-holder-using-whois/\r
%%\r
%%\r
\r
%% NOT FOUND\r
\r
>>> Last update of WHOIS database: 2025-03-04T21:18:27.576643Z <<<\r
";
        let whois = (FR.map_response)(RESP);
        assert!(whois.is_none());
    }
}
