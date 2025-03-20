use crate::DomainInfo;
use icann_rdap_client::http::Client;
use icann_rdap_client::prelude::*;
use icann_rdap_common::response::{RdapResponse, entity::Entity};
use serde_json::Value;
use time::format_description::well_known;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

// NOTE TO SELF
// The code dealing with the cache may appear quirky and slightly inefficient, but
// that's the only way to make the borrow checker happy
// See
// https://github.com/rust-lang/rfcs/blob/master/text/2094-nll.md#problem-case-3-conditional-control-flow-across-functions

#[derive(Debug)]
struct VcardTextProp<'a> {
    name: &'a str,
    value: &'a str,
}

impl<'a> TryFrom<&'a Value> for VcardTextProp<'a> {
    type Error = &'static str;

    fn try_from(v: &'a Value) -> Result<Self, Self::Error> {
        let as_ar = if let Some(ar) = v.as_array() {
            ar.as_slice()
        } else {
            return Err("Invalid Property (expected array)");
        };
        if as_ar.len() != 4 {
            return Err("Invalid Property (invalid array len)");
        }
        let name = if let Some(v) = as_ar[0].as_str() {
            v
        } else {
            return Err("Invalid Property (invalid name type)");
        };
        if !as_ar[1].is_object() {
            return Err("Invalid Property (invalid params type)");
        }
        if as_ar[2].as_str() != Some("text") {
            return Err("Invalid Property (not text");
        };
        let value = if let Some(v) = as_ar[3].as_str() {
            v
        } else {
            return Err("Invalid Property (invalid value type)");
        };
        Ok(Self { name, value })
    }
}

#[derive(Debug)]
struct Vcard<'a> {
    full_name: &'a str,
    kind: Option<&'a str>,
    roles: Vec<&'a str>,
}

impl Vcard<'_> {
    fn has_role(&self, role: &str) -> bool {
        self.roles.as_slice().contains(&role)
    }
    fn is_person(&self) -> bool {
        self.kind == Some("individual")
    }
}

impl<'a> TryFrom<&'a Entity> for Vcard<'a> {
    type Error = &'static str;

    fn try_from(ent: &'a Entity) -> Result<Self, Self::Error> {
        let vcard_array = if let Some(ar) = &ent.vcard_array {
            ar.as_slice()
        } else {
            return Err("Not a vCard (no array)");
        };
        if vcard_array.len() != 2 {
            return Err("Not a vCard (bad array len)");
        }
        if let Some(s) = vcard_array[0].as_str() {
            if s != "vcard" {
                return Err("Not a vCard (bad entity type)");
            }
        } else {
            return Err("Not a vCard (bad entity type type)");
        }
        let props_ar = if let Some(p) = vcard_array[1].as_array() {
            p.as_slice()
        } else {
            return Err("Not a vCard (bad properties type)");
        };
        let props: Vec<VcardTextProp<'_>> = props_ar
            .iter()
            .filter_map(|p| VcardTextProp::try_from(p).ok())
            .collect();
        if !props
            .iter()
            .any(|p| p.name == "version" && p.value == "4.0")
        {
            return Err("Not a vCard (missing or invalid version)");
        }
        let full_name = if let Some(n) = props.iter().find(|p| p.name == "fn") {
            n.value
        } else {
            return Err("Not a vCard (missing fn)");
        };
        let kind = props.iter().find(|p| p.name == "kind").map(|p| p.value);
        let roles = if let Some(roles) = &ent.roles {
            roles.iter().map(|s| s.as_str()).collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            full_name,
            kind,
            roles,
        })
    }
}

pub struct Rdap {
    client: Client,
    store: MemoryBootstrapStore,
    cache: lru::LruCache<String, (std::time::Instant, DomainInfo)>,
}

impl Rdap {
    pub fn new() -> Result<Self, RdapClientError> {
        let config = ClientConfig::default();
        Ok(Self {
            client: create_client(&config)?,
            store: MemoryBootstrapStore::new(),
            cache: lru::LruCache::new(super::CACHE_MAX_ENTRIES),
        })
    }

    async fn query(&self, domain: &str) -> Result<DomainInfo, Box<dyn std::error::Error>> {
        let query: QueryType = domain.parse()?;
        let resp = rdap_bootstrapped_request(&query, &self.client, &self.store, |_| {}).await?;

        let dom = match &resp.rdap {
            RdapResponse::Domain(d) => d,
            _ => {
                debug!("Invalid RDAP reply type {}", resp.rdap_type);
                return Err("Invalid RDAP reply type".into());
            }
        };

        let mut di = DomainInfo::default();
        if let Some(events) = &dom.object_common.events {
            di.created = events
                .iter()
                .find(|e| e.event_action.as_deref() == Some("registration"))
                .and_then(|e| e.event_date.as_ref())
                .and_then(|dt| time::Date::parse(dt.trim(), &well_known::Iso8601::DEFAULT).ok());
            di.updated = events
                .iter()
                .find(|e| e.event_action.as_deref() == Some("last changed"))
                .and_then(|e| e.event_date.as_ref())
                .and_then(|dt| time::Date::parse(dt.trim(), &well_known::Iso8601::DEFAULT).ok());
            di.expiry = events
                .iter()
                .find(|e| e.event_action.as_deref() == Some("expiration"))
                .and_then(|e| e.event_date.as_ref())
                .and_then(|dt| time::Date::parse(dt.trim(), &well_known::Iso8601::DEFAULT).ok());
        }
        di.status = dom.object_common.status.as_ref().map(|s| {
            s.iter()
                .map(|s| s.0.as_str())
                .collect::<Vec<&str>>()
                .join(",")
        });
        di.whois = dom.object_common.port_43.as_ref().map(|s| s.to_string());

        if let Some(ents) = &dom.object_common.entities {
            for vcard in ents.iter().filter_map(|e| Vcard::try_from(e).ok()) {
                if vcard.has_role("registrar") {
                    di.registrar = Some(vcard.full_name.to_string());
                }
                if vcard.has_role("registrant") {
                    if vcard.is_person() {
                        di.registrant_name = Some(vcard.full_name.to_string());
                    } else {
                        di.registrant_org = Some(vcard.full_name.to_string());
                    }
                }
                if vcard.has_role("administrative") {
                    if vcard.is_person() {
                        di.admin_name = Some(vcard.full_name.to_string());
                    } else {
                        di.admin_org = Some(vcard.full_name.to_string());
                    }
                }
                if vcard.has_role("technical") {
                    if vcard.is_person() {
                        di.tech_name = Some(vcard.full_name.to_string());
                    } else {
                        di.tech_org = Some(vcard.full_name.to_string());
                    }
                }
            }
        }
        di.nss = dom.nameservers.as_ref().map(|nss| {
            nss.iter()
                .filter_map(|ns| ns.ldh_name.as_ref().or(ns.unicode_name.as_ref()))
                .map(|s| s.to_string())
                .collect()
        });
        Ok(di)
    }

    pub async fn lookup(
        &mut self,
        domain: &str,
    ) -> Result<Option<&DomainInfo>, Box<dyn std::error::Error>> {
        let cached = self.cache.pop(domain);
        let cached = if let Some(cached) = cached {
            if cached.0.elapsed() < super::CACHE_MAX_AGE_HIT {
                Some(cached)
            } else {
                None
            }
        } else {
            None
        };
        let cached_or_queried = if let Some(v) = cached {
            debug!("Cache hit for {}", domain);
            v
        } else {
            debug!("Cache miss for {}", domain);
            (std::time::Instant::now(), self.query(domain).await?)
        };
        self.cache.push(domain.to_string(), cached_or_queried);
        Ok(self.cache.get(domain).map(|(_, v)| v))
    }
}
