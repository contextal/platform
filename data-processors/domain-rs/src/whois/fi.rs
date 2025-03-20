use super::macros::*;
use crate::DomainInfo;

pub const FI: &super::TldWhois = &super::TldWhois {
    get_query_string: None,
    map_response,
};

fn map_response(resp: &str) -> Option<DomainInfo> {
    const DATEPARSEFMT: &[time::format_description::BorrowedFormatItem<'_>] =
        time::macros::format_description!("[day padding:none].[month padding:none].[year]");
    static DOMAIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"domain\.*?:"));
    static CREATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!(r"created\.*?:"));
    static UPDATED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!(r"modified\.*?:"));
    static EXPIRY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!(r"expires\.*?:"));
    static STATUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"status\.*?:"));
    static NS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_ws!(r"nserver\.*?:"));
    static REGISTRAR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"registrar\.*?:"));
    static REGISTRANT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| capture_til_eol!(r"name\.*?:"));

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
    res.registrar = REGISTRAR
        .captures(resp)
        .map(|cap| cap[1].trim().to_string());
    res.registrant_name = REGISTRANT
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
        const RESP: &str = "
\r
domain.............: penet.fi\r
status.............: Registered\r
created............: 1.1.1991 00:00:00\r
expires............: 31.8.2025 00:00:00\r
available..........: 30.9.2025 00:00:00\r
modified...........: 25.4.2023 14:05:05\r
RegistryLock.......: no\r
\r
Nameservers\r
\r
nserver............: ns-1066.awsdns-05.org [OK]\r
nserver............: ns-cloud-a3.googledomains.com [OK]\r
nserver............: ns-841.awsdns-41.net [OK]\r
nserver............: ns-1750.awsdns-26.co.uk [OK]\r
nserver............: ns-cloud-a4.googledomains.com [OK]\r
nserver............: ns-cloud-a1.googledomains.com [OK]\r
nserver............: ns-cloud-a2.googledomains.com [OK]\r
nserver............: ns-398.awsdns-49.com [OK]\r
\r
DNSSEC\r
\r
dnssec.............: no\r
\r
Holder\r
\r
name...............: Penetic Oy\r
register number....: 0830656-5\r
address............: Lisdoddelaan 57\r
postal.............: 1087KB\r
city...............: Amsterdam\r
country............: Netherlands\r
phone..............: \r
holder email.......: \r
\r
Registrar\r
\r
registrar..........: BaseN Oy\r
www................: www.basen.net\r
\r
>>> Last update of WHOIS database: 5.3.2025 19:00:10 (EET) <<<\r
\r
\r
Copyright (c) Finnish Transport and Communications Agency Traficom\r
\r
";
        let whois = (FI.map_response)(RESP).expect("mapping");
        let created = whois.created.expect("created date");
        assert_eq!(created.day(), 1);
        assert_eq!(created.month() as i32, 1);
        assert_eq!(created.year(), 1991);
        let updated = whois.updated.expect("updated date");
        assert_eq!(updated.day(), 25);
        assert_eq!(updated.month() as i32, 4);
        assert_eq!(updated.year(), 2023);
        let expiry = whois.expiry.expect("expiry date");
        assert_eq!(expiry.day(), 31);
        assert_eq!(expiry.month() as i32, 8);
        assert_eq!(expiry.year(), 2025);
        assert_eq!(whois.status.as_deref(), Some("Registered"));
        let nss = whois.nss.expect("name servers");
        assert_eq!(nss[0], "ns-1066.awsdns-05.org");
        assert_eq!(nss[6], "ns-cloud-a2.googledomains.com");
        assert_eq!(nss.len(), 8);
    }

    #[test]
    fn not_found() {
        const RESP: &str = "\
# Copyright (c) 1997- The Swedish Internet Foundation.\r
# All rights reserved.\r
# The information obtained through searches, or otherwise, is protected\r
# by the Swedish Copyright Act (1960:729) and international conventions.\r
# It is also subject to database protection according to the Swedish\r
# Copyright Act.\r
# Any use of this material to target advertising or\r
# similar activities is forbidden and will be prosecuted.\r
# If any of the information below is transferred to a third\r
# party, it must be done in its entirety. This server must\r
# not be used as a backend for a search engine.\r
# Result of search for registered domain names under\r
# the .se top level domain.\r
# This whois printout is printed with UTF-8 encoding.\r
#\r
domain \"??????.se\" not found\r
";
        let whois = (FI.map_response)(RESP);
        assert!(whois.is_none());
    }
}
