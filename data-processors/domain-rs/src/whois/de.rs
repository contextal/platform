use super::macros::*;
use crate::DomainInfo;
use time::format_description::well_known;

pub const DE: &super::TldWhois = &super::TldWhois {
    get_query_string: Some(get_query_string),
    map_response,
};

fn get_query_string(ascii_domain: &str) -> String {
    format!("-T dn,ace {}\r\n", ascii_domain)
}

fn map_response(resp: &str) -> Option<DomainInfo> {
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Changed:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Status:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Nserver:"));

    let mut res = DomainInfo::default();
    res.status = STATUS.captures(resp).map(|cap| cap[1].trim().to_string());
    if res.status.as_deref() == Some("free") {
        return None;
    }
    res.updated = UPDATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &well_known::Iso8601::DEFAULT).ok());
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
% Restricted rights.\r
% \r
% Terms and Conditions of Use\r
% \r
% The above data may only be used within the scope of technical or\r
% administrative necessities of Internet operation or to remedy legal\r
% problems.\r
% The use for other purposes, in particular for advertising, is not permitted.\r
% \r
% The DENIC whois service on port 43 doesn't disclose any information concerning\r
% the domain holder, general request and abuse contact.\r
% This information can be obtained through use of our web-based whois service\r
% available at the DENIC website:\r
% http://www.denic.de/en/domains/whois-service/web-whois.html\r
% \r
% \r
\r
Domain: hetzner.de\r
Nserver: ns1.your-server.de\r
Nserver: ns3.second-ns.de\r
Nserver: ns.second-ns.com\r
Status: connect\r
Changed: 2015-08-04T07:58:13+02:00\r
";
        let whois = (DE.map_response)(RESP).expect("mapping");
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 4);
        assert_eq!(updated.month() as i32, 8);
        assert_eq!(updated.year(), 2015);
        assert_eq!(whois.status.as_deref(), Some("connect"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns1.your-server.de");
        assert_eq!(nss[2], "ns.second-ns.com");
        assert_eq!(nss.len(), 3);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
Domain: ???????.de\r
Status: free\r
";
        let whois = (DE.map_response)(RESP);
        assert!(whois.is_none());
    }
}
