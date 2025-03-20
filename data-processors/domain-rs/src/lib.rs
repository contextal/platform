#![allow(clippy::field_reassign_with_default)]
#[cfg(feature = "rdap")]
mod rdap;
mod whois;

use public_suffix::{DEFAULT_PROVIDER, EffectiveTLDProvider};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

const CACHE_MAX_ENTRIES: std::num::NonZeroUsize = std::num::NonZeroUsize::new(8192).unwrap();
const CACHE_MAX_AGE_HIT: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);
const CACHE_MAX_AGE_MISS: std::time::Duration = std::time::Duration::from_secs(10 * 60);

#[derive(Debug, Default)]
pub struct DomainInfo {
    pub whois: Option<String>,
    pub registrar: Option<String>,
    pub created: Option<time::Date>,
    pub updated: Option<time::Date>,
    pub expiry: Option<time::Date>,
    pub status: Option<String>,
    pub nss: Option<Vec<String>>,
    pub registrant_name: Option<String>,
    pub registrant_org: Option<String>,
    pub admin_name: Option<String>,
    pub admin_org: Option<String>,
    pub tech_name: Option<String>,
    pub tech_org: Option<String>,
}

pub struct DomainQuery {
    whois: whois::Whois,
    #[cfg(feature = "rdap")]
    rdap: rdap::Rdap,
}

impl Default for DomainQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainQuery {
    pub fn new() -> Self {
        Self {
            whois: whois::Whois::new(),
            #[cfg(feature = "rdap")]
            rdap: rdap::Rdap::new().expect("FATAL: failed to start rdap client"),
        }
    }

    pub async fn query<S: AsRef<str>>(
        &mut self,
        fqdn: S,
        timeout: &std::time::Duration,
    ) -> Result<Option<&DomainInfo>, Box<dyn std::error::Error>> {
        #[cfg(feature = "rdap")]
        let start = std::time::Instant::now();
        let fqdn = fqdn.as_ref();
        let ascii_domain = idna::domain_to_ascii_cow(fqdn.as_bytes(), idna::AsciiDenyList::URL)?;
        let domain = match DEFAULT_PROVIDER.effective_tld_plus_one(ascii_domain.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                warn!("Cannot map FQDN {} to domain name: {:?}", fqdn, e);
                return Err("Cannot map FQDN to domain name".into());
            }
        };
        debug!("Domain name for {} is {}", fqdn, domain);
        let whois_res = self.whois.lookup(domain, timeout);
        debug!("WHOIS result for {}: {:#?}", domain, whois_res);
        #[cfg(feature = "rdap")]
        {
            if whois_res.is_err() || whois_res.as_ref().map(|v| v.is_none()).unwrap_or(false) {
                let remaining = timeout.saturating_sub(start.elapsed());
                let rdap_res = tokio::time::timeout(remaining, self.rdap.lookup(domain))
                    .await
                    .inspect_err(|_| warn!("RDAP query for {} timed out", domain))?;
                debug!("RDAP result for {}: {:#?}", domain, rdap_res);
                return rdap_res;
            }
        }
        whois_res
    }
}
