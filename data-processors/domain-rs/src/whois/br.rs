use super::macros::*;
use crate::DomainInfo;

pub const BR: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] =
        time::macros::format_description!("[year][month padding:zero][day padding:zero]");
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("domain:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("created:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("changed:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("expires:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("nserver:"));
    static REGISTRANT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("owner:"));
    static ADMIN_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("owner-c:"));
    static TECH_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("tech-c:"));
    static HANDLES: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!(r"nic-hdl-br:.*?([^\s].*)"));
    static HDL_CONTACT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("person:"));

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
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    res.registrant_name = REGISTRANT
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    let admin_c = ADMIN_C.captures(resp).map(|cap| cap[1].trim().to_string());
    let tech_c = TECH_C.captures(resp).map(|cap| cap[1].trim().to_string());
    for cap in HANDLES.captures_iter(resp) {
        let handle = cap[1].trim();
        let contact = HDL_CONTACT
            .captures(&cap[0])
            .map(|s| s[1].trim().to_string());
        if admin_c.as_deref() == Some(handle) {
            res.admin_name = contact;
        } else if tech_c.as_deref() == Some(handle) {
            res.tech_name = contact;
        }
    }
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
% Copyright (c) Nic.br - Use of this data is governed by the Use and\r
% Privacy Policy at https://registro.br/upp . Distribution,\r
% commercialization, reproduction, and use for advertising or similar\r
% purposes are expressly prohibited.\r
% 2025-03-08T06:35:54-03:00 - 82.57.203.60\r
\r
domain:      www.gov.br\r
owner:       Ministerio do Planejamento, Orcamento e Gestao\r
owner-c:     CGSTM2\r
tech-c:      GSISE\r
nserver:     bsa1.serpro.gov.br\r
nsstat:      20250308 AA\r
nslastaa:    20250308\r
nserver:     bsa2.serpro.gov.br\r
nsstat:      20250308 AA\r
nslastaa:    20250308\r
nserver:     spo1.serpro.gov.br\r
nsstat:      20250308 AA\r
nslastaa:    20250308\r
nserver:     spo2.serpro.gov.br\r
nsstat:      20250308 AA\r
nslastaa:    20250308\r
dsrecord:    55504 RSA-SHA-256 3298CCC079A7054836F73539941E1BF68779F05CCEA21850C72FF87CE62EDE1A\r
dsstatus:    20250308 DSOK\r
dslastok:    20250308\r
saci:        yes\r
created:     20170829 #17394568\r
changed:     20211030\r
expires:     20250927\r
status:      published\r
\r
nic-hdl-br:  CGSTM2\r
person:      COORDENAÇÃO GERAL DE SERVIÇOS DE TI - MP\r
created:     20140122\r
changed:     20240207\r
\r
nic-hdl-br:  GSISE\r
person:      Gestão do Serviço Internet  SERPRO\r
created:     20190814\r
changed:     20200511\r
\r
% Security and mail abuse issues should also be addressed to cert.br,\r
% respectivelly to cert@cert.br and mail-abuse@cert.br\r
%\r
% whois.registro.br only accepts exact match queries for domains,\r
% registrants, contacts, tickets, providers, IPs, and ASNs.\r
";
        let whois = (BR.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 29);
        assert_eq!(created.month() as i32, 8);
        assert_eq!(created.year(), 2017);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 30);
        assert_eq!(updated.month() as i32, 10);
        assert_eq!(updated.year(), 2021);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 27);
        assert_eq!(expiry.month() as i32, 9);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.status.as_deref(), Some("published"));
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("Ministerio do Planejamento, Orcamento e Gestao")
        );
        assert_eq!(
            whois.admin_name.as_deref(),
            Some("COORDENAÇÃO GERAL DE SERVIÇOS DE TI - MP")
        );
        assert_eq!(
            whois.tech_name.as_deref(),
            Some("Gestão do Serviço Internet  SERPRO")
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "bsa1.serpro.gov.br");
        assert_eq!(nss[3], "spo2.serpro.gov.br");
        assert_eq!(nss.len(), 4);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
% Copyright (c) Nic.br - Use of this data is governed by the Use and\r
% Privacy Policy at https://registro.br/upp . Distribution,\r
% commercialization, reproduction, and use for advertising or similar\r
% purposes are expressly prohibited.\r
% 2025-03-08T07:38:40-03:00 - 82.57.203.60\r
\r
% No match for ??????????????.br\r
\r
% Security and mail abuse issues should also be addressed to cert.br,\r
% respectivelly to cert@cert.br and mail-abuse@cert.br\r
%\r
% whois.registro.br only accepts exact match queries for domains,\r
% registrants, contacts, tickets, providers, IPs, and ASNs.\r
";
        let whois = (BR.map_response)(RESP);
        assert!(whois.is_none());
    }
}
