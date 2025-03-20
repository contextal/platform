use super::macros::*;
use crate::DomainInfo;

pub const AT: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] =
        time::macros::format_description!("[year][month padding:zero][day padding:zero]");
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("domain:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_digits!("changed:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("registrar:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!("nserver:"));
    static REGISTRANT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("registrant:"));
    static TECH_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("tech-c:"));
    static HANDLES: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!(r"(?:personname|organization):"));

    static NIC_HDL: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("nic-hdl:"));
    static NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("personname:"));
    static ORG: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("organization:"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    let registrant = REGISTRANT
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    let tech_c = TECH_C.captures(resp).map(|cap| cap[1].trim().to_string());
    for cap in HANDLES.captures_iter(resp) {
        let handle = if let Some(hdl) = NIC_HDL
            .captures(&cap[0])
            .map(|cap| cap[1].trim().to_string())
        {
            hdl
        } else {
            continue;
        };
        let name = NAME.captures(&cap[0]).map(|cap| cap[1].trim().to_string());
        let org = ORG.captures(&cap[0]).map(|cap| cap[1].trim().to_string());
        if registrant.as_deref() == Some(&handle) {
            res.registrant_name = name;
            res.registrant_org = org;
        } else if tech_c.as_deref() == Some(&handle) {
            res.tech_name = name;
            res.tech_org = org;
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
% Copyright (c)2025 by NIC.AT (1)                                       \r
%\r
% Restricted rights.\r
%\r
% Except  for  agreed Internet  operational  purposes, no  part  of this\r
% information  may  be reproduced,  stored  in  a  retrieval  system, or\r
% transmitted, in  any  form  or by  any means,  electronic, mechanical,\r
% recording, or otherwise, without prior  permission of NIC.AT on behalf\r
% of itself and/or the copyright  holders.  Any use of this  material to\r
% target advertising  or similar activities is explicitly  forbidden and\r
% can be prosecuted.\r
%\r
% It is furthermore strictly forbidden to use the Whois-Database in such\r
% a  way  that  jeopardizes or  could jeopardize  the  stability  of the\r
% technical  systems of  NIC.AT  under any circumstances. In particular,\r
% this includes  any misuse  of the  Whois-Database and  any  use of the\r
% Whois-Database which disturbs its operation.\r
%\r
% Should the  user violate  these points,  NIC.AT reserves  the right to\r
% deactivate  the  Whois-Database   entirely  or  partly  for  the user.\r
% Moreover,  the  user  shall be  held liable  for  any  and all  damage\r
% arising from a violation of these points.\r
\r
domain:         google.at\r
registrar:      MarkMonitor Inc. ( https://nic.at/registrar/434 )\r
registrant:     GL11783559-NICAT\r
tech-c:         GI7803025-NICAT\r
tech-c:         GL11783561-NICAT\r
nserver:        ns1.google.com\r
nserver:        ns2.google.com\r
nserver:        ns3.google.com\r
nserver:        ns4.google.com\r
changed:        20241113 19:36:02\r
source:         AT-DOM\r
\r
personname:     Domain Administrator\r
organization:   Google LLC\r
street address: 1600 Amphitheatre Parkway\r
postal code:    94043\r
city:           Mountain View\r
country:        United States of America (the)\r
phone:          +16502530000\r
fax-no:         +16502530001\r
e-mail:         dns-admin@google.com\r
nic-hdl:        GL11783559-NICAT\r
changed:        20180302 18:52:05\r
source:         AT-DOM\r
\r
personname:     DNS Admin\r
organization:   Google Inc.\r
street address: 1600 Amphitheatre Parkway\r
postal code:    94043\r
city:           Mountain View\r
country:        United States of America (the)\r
phone:          +16502530000\r
fax-no:         +16502530001\r
e-mail:         dns-admin@google.com\r
nic-hdl:        GI7803025-NICAT\r
changed:        20110111 00:08:30\r
source:         AT-DOM\r
\r
personname:     Domain Administrator\r
organization:   Google LLC\r
street address: 1600 Amphitheatre Parkway\r
postal code:    94043\r
city:           Mountain View\r
country:        United States of America (the)\r
phone:          +16502530000\r
fax-no:         +16502530001\r
e-mail:         dns-admin@google.com\r
nic-hdl:        GL11783561-NICAT\r
changed:        20180302 18:52:06\r
source:         AT-DOM\r
\r
";
        let whois = (AT.map_response)(RESP).expect("mapping");
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 13);
        assert_eq!(updated.month() as i32, 11);
        assert_eq!(updated.year(), 2024);
        assert_eq!(
            whois.registrar.as_deref(),
            Some("MarkMonitor Inc. ( https://nic.at/registrar/434 )")
        );
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("Domain Administrator")
        );
        assert_eq!(whois.registrant_org.as_deref(), Some("Google LLC"));
        assert_eq!(whois.tech_name.as_deref(), Some("DNS Admin"));
        assert_eq!(whois.tech_org.as_deref(), Some("Google Inc."));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1.google.com");
        assert_eq!(nss[3], "ns4.google.com");
        assert_eq!(nss.len(), 4);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
% Copyright (c)2025 by NIC.AT (1)                                       \r
%\r
% Restricted rights.\r
%\r
% Except  for  agreed Internet  operational  purposes, no  part  of this\r
% information  may  be reproduced,  stored  in  a  retrieval  system, or\r
% transmitted, in  any  form  or by  any means,  electronic, mechanical,\r
% recording, or otherwise, without prior  permission of NIC.AT on behalf\r
% of itself and/or the copyright  holders.  Any use of this  material to\r
% target advertising  or similar activities is explicitly  forbidden and\r
% can be prosecuted.\r
%\r
% It is furthermore strictly forbidden to use the Whois-Database in such\r
% a  way  that  jeopardizes or  could jeopardize  the  stability  of the\r
% technical  systems of  NIC.AT  under any circumstances. In particular,\r
% this includes  any misuse  of the  Whois-Database and  any  use of the\r
% Whois-Database which disturbs its operation.\r
%\r
% Should the  user violate  these points,  NIC.AT reserves  the right to\r
% deactivate  the  Whois-Database   entirely  or  partly  for  the user.\r
% Moreover,  the  user  shall be  held liable  for  any  and all  damage\r
% arising from a violation of these points.\r
\r
% nothing found\r
\r
";
        let whois = (AT.map_response)(RESP);
        assert!(whois.is_none());
    }
}
