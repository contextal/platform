use super::macros::*;
use crate::DomainInfo;

pub const KR: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] = time::macros::format_description!(
        "[year]. [month repr:numerical padding:zero]. [day padding:zero]."
    );
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Domain Name\s*?:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Registered Date\s*?:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Last Updated Date\s*?:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Expiration Date\s*?:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!(r"\w+ Name Server"));
    static HOSTNAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"\s*?Host Name\s*?:"));
    static REGISTRANT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Registrant\s*?:"));

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
    res.registrant_name = REGISTRANT
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    let nss: Vec<String> = NS
        .captures_iter(resp)
        .filter_map(|cap| {
            HOSTNAME
                .captures(&cap[1])
                .map(|cap| cap[1].trim().to_string())
        })
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
query : yu.ac.kr\r
\r
\r
# KOREAN(UTF8)\r
\r
도메인이름                  : yu.ac.kr\r
등록인                      : 영남대학교\r
등록인 주소                 : 경북 경산시 대동 영남대학교 정보전산원\r
등록인 우편번호             : 712749\r
책임자                      : 김병수\r
책임자 전자우편             : bskim@yeungnam.ac.kr\r
책임자 전화번호             : 053-810-3663\r
등록일                      : 1999. 07. 15.\r
최근 정보 변경일            : 2003. 10. 15.\r
사용 종료일                 : 2032. 07. 15.\r
정보공개여부                : Y\r
등록대행자                  : (주)아이네임즈(http://www.inames.co.kr)\r
DNSSEC                      : 미서명\r
\r
1차 네임서버 정보\r
   호스트이름               : ns.yu.ac.kr\r
   IP 주소                  : 165.229.11.5\r
\r
2차 네임서버 정보\r
   호스트이름               : ns3.yu.ac.kr\r
   IP 주소                  : 165.229.11.8\r
\r
네임서버 이름이 .kr이 아닌 경우는 IP주소가 보이지 않습니다.\r
\r
\r
# ENGLISH\r
\r
Domain Name                 : yu.ac.kr\r
Registrant                  : YEUNGNAM UNIVERSITY\r
Registrant Address          : YEUNGNAM UNIVERSITY, DAEDONG, KYUNGSAN, KYUNGPOOK, \r
Registrant Zip Code         : 712749\r
Administrative Contact(AC)  : KIM BYUNG SOO\r
AC E-Mail                   : bskim@yeungnam.ac.kr\r
AC Phone Number             : 053-810-3663\r
Registered Date             : 1999. 07. 15.\r
Last Updated Date           : 2003. 10. 15.\r
Expiration Date             : 2032. 07. 15.\r
Publishes                   : Y\r
Authorized Agency           : Inames Co., Ltd.(http://www.inames.co.kr)\r
DNSSEC                      : unsigned\r
\r
Primary Name Server\r
   Host Name                : ns.yu.ac.kr\r
   IP Address               : 165.229.11.5\r
\r
Secondary Name Server\r
   Host Name                : ns3.yu.ac.kr\r
   IP Address               : 165.229.11.8\r
\r
\r
- KISA/KRNIC WHOIS Service -\r
\r
";
        let whois = (KR.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 15);
        assert_eq!(created.month() as i32, 7);
        assert_eq!(created.year(), 1999);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 15);
        assert_eq!(updated.month() as i32, 10);
        assert_eq!(updated.year(), 2003);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 15);
        assert_eq!(expiry.month() as i32, 7);
        assert_eq!(expiry.year(), 2032);
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("YEUNGNAM UNIVERSITY")
        );
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns.yu.ac.kr");
        assert_eq!(nss[1], "ns3.yu.ac.kr");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
query : ????.kr\r
\r
\r
# KOREAN(UTF8)\r
\r
상기 도메인이름은 등록되어 있지 않습니다.\r
상기 도메인이름의 사용을 원하실 경우 도메인이름 등록대행자를 통해 \r
등록 신청하시기 바랍니다.\r
\r
\r
\r
# ENGLISH\r
\r
The requested domain was not found in the Registry or Registrar’s WHOIS Server.\r
\r
\r
\r
- KISA/KRNIC WHOIS Service -\r
\r
";
        let whois = (KR.map_response)(RESP);
        assert!(whois.is_none());
    }
}
