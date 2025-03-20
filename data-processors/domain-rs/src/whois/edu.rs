use super::macros::*;
use crate::DomainInfo;

pub const EDU: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] = time::macros::format_description!(
        "[day padding:zero]-[month repr:short case_sensitive:false]-[year]"
    );
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain Name:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain record activated:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain expires:"));
    static MODIFIED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!("Domain record last updated:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_section!("Name Servers:"));
    static REGISTRANT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Registrant:.*$\n"));
    static ADMIN_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Administrative Contact:.*$\n"));
    static TECH_C: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"Technical Contact:.*$\n"));

    DOMAIN.captures(resp)?;
    let mut res = DomainInfo::default();
    res.created = CREATED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.expiry = EXPIRY
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.updated = MODIFIED
        .captures(resp)
        .and_then(|cap| time::Date::parse(cap[1].trim(), &DATEPARSEFMT).ok());
    res.registrant_name = REGISTRANT
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.admin_name = ADMIN_C.captures(resp).map(|cap| cap[1].trim().to_string());
    res.tech_name = TECH_C.captures(resp).map(|cap| cap[1].trim().to_string());
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
This Registry database contains ONLY .EDU domains.\r
The data in the EDUCAUSE Whois database is provided\r
by EDUCAUSE for information purposes in order to\r
assist in the process of obtaining information about\r
or related to .edu domain registration records.\r
\r
The EDUCAUSE Whois database is authoritative for the\r
.EDU domain.\r
\r
A Web interface for the .EDU EDUCAUSE Whois Server is\r
available at: http://whois.educause.edu\r
\r
By submitting a Whois query, you agree that this information\r
will not be used to allow, enable, or otherwise support\r
the transmission of unsolicited commercial advertising or\r
solicitations via e-mail.  The use of electronic processes to\r
harvest information from this server is generally prohibited\r
except as reasonably necessary to register or modify .edu\r
domain names.\r
\r
-------------------------------------------------------------\r
\r
Domain Name: UC.EDU\r
\r
Registrant:\r
	University of Cincinnati\r
	UC Information Tecnologies (UCit)\r
	51 Goodman Drive, Suite 400\r
	Cincinnati, OH 45221-0658\r
	USA\r
\r
Administrative Contact:\r
	Domain Admin\r
	University of Cincinnati\r
	UC Information Tecnologies (UCit)\r
	51 Goodman Drive, Suite 400\r
	Cincinnati, OH 45221-0658\r
	USA\r
	+1.5135569898\r
	barb.renner@uc.edu\r
\r
Technical Contact:\r
	Brian Ruehl\r
	University of Cincinnati\r
	UC Information Technologies\r
	3255 Eden Ave, Suite G58\r
	Cincinnati, OH 45267-0819\r
	USA\r
	+1.5135561921\r
	brian.ruehl@uc.edu\r
\r
Name Servers:\r
	UCDNSA.UC.EDU\r
	UCDNSB.UC.EDU\r
\r
Domain record activated:    16-Nov-1987\r
Domain record last updated: 03-Jun-2024\r
Domain expires:             31-Jul-2027\r
\r
";
        let whois = (EDU.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 16);
        assert_eq!(created.month() as i32, 11);
        assert_eq!(created.year(), 1987);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 3);
        assert_eq!(updated.month() as i32, 6);
        assert_eq!(updated.year(), 2024);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 31);
        assert_eq!(expiry.month() as i32, 7);
        assert_eq!(expiry.year(), 2027);
        assert_eq!(
            whois.registrant_name.as_deref(),
            Some("University of Cincinnati")
        );
        assert!(whois.registrant_org.is_none());
        assert_eq!(whois.admin_name.as_deref(), Some("Domain Admin"));
        assert!(whois.admin_org.is_none());
        assert_eq!(whois.tech_name.as_deref(), Some("Brian Ruehl"));
        assert!(whois.tech_org.is_none());
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "UCDNSA.UC.EDU");
        assert_eq!(nss[1], "UCDNSB.UC.EDU");
        assert_eq!(nss.len(), 2);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
NO MATCH: ?????.edu\r
\r
";
        let whois = (EDU.map_response)(RESP);
        assert!(whois.is_none());
    }
}
