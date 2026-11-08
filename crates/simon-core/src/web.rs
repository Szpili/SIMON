//! Broad-scope tool bounds: strict web and code validation.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// Parsed URL fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlFields {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    #[serde(rename = "method", alias = "metoda")]
    pub method: String,
    pub userinfo: bool,
}

pub type PolaUrl = UrlFields;

/// Policy for `web_fetch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebPolicy {
    #[serde(rename = "allowed_hosts", alias = "dozwolone_hosty")]
    pub allowed_hosts: Vec<String>,
    #[serde(rename = "max_bytes", alias = "max_bajtow")]
    pub max_bytes: u64,
}

pub type PolitykaWeb = WebPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebError {
    Scheme,
    Port,
    Userinfo,
    Method,
    Host,
    PrivateHost,
    DnsMismatch,
    ImageNotAllowed,
}

pub type BladWeb = WebError;

pub use WebError::{
    DnsMismatch as DnsNiezgodny, Host as HostOdrzucony, ImageNotAllowed as ObrazNiedozwolony,
    Method as Metoda, PrivateHost as HostPrywatny,
};

pub fn is_private_ip(s: &str) -> bool {
    let clean = s.trim_start_matches('[').trim_end_matches(']');
    match clean.parse::<IpAddr>() {
        Ok(ip) => match ip.to_canonical() {
            IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
                    || v4.is_multicast()
                    || v4.is_documentation()
                    || (v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 64)
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || ((v6.segments()[0] & 0xfe00) == 0xfc00)
                    || ((v6.segments()[0] & 0xffc0) == 0xfe80)
            }
        },
        Err(_) => false,
    }
}

pub use is_private_ip as prywatny_ip;

/// Whether host matches allowlist (exact or subdomain match).
pub fn host_allowed(host: &str, list: &[String]) -> bool {
    let h = host.to_ascii_lowercase();
    list.iter().any(|pattern| {
        let p = pattern.to_ascii_lowercase();
        let without_dot = p.trim_start_matches('.');
        h == without_dot || h.ends_with(&format!(".{without_dot}"))
    })
}

pub use host_allowed as host_dozwolony;

/// Validates `web_fetch` proposal against policy.
pub fn allowed_web(p: &UrlFields, pol: &WebPolicy) -> Result<(), WebError> {
    if p.scheme != "https" {
        return Err(WebError::Scheme);
    }
    if p.port != 443 {
        return Err(WebError::Port);
    }
    if p.userinfo {
        return Err(WebError::Userinfo);
    }
    if !p.method.eq_ignore_ascii_case("GET") {
        return Err(WebError::Method);
    }
    if is_private_ip(&p.host) {
        return Err(WebError::PrivateHost);
    }
    if !host_allowed(&p.host, &pol.allowed_hosts) {
        return Err(WebError::Host);
    }
    Ok(())
}

pub use allowed_web as dozwolony_web;

/// Anti-DNS rebind check: resolved IP must not be private, and must match declared.
pub fn allowed_dns(declared_ip: &str, resolved_ip: &str) -> Result<(), WebError> {
    if is_private_ip(resolved_ip) {
        return Err(WebError::PrivateHost);
    }
    if declared_ip != resolved_ip {
        return Err(WebError::DnsMismatch);
    }
    Ok(())
}

pub use allowed_dns as dozwolony_dns;

/// Validates container image name for code execution against allowlist.
pub fn allowed_code(image: &str, allowed_images: &[String]) -> Result<(), WebError> {
    if allowed_images.iter().any(|i| i == image) {
        Ok(())
    } else {
        Err(WebError::ImageNotAllowed)
    }
}

pub use allowed_code as dozwolony_code;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_policy() -> WebPolicy {
        WebPolicy {
            allowed_hosts: vec![
                "api.example.com".into(),
                "en.wikipedia.org".into(),
                ".trusted.org".into(),
            ],
            max_bytes: 1024 * 1024,
        }
    }

    #[test]
    fn web_allowed_only_on_narrow_policy() {
        let pol = sample_policy();
        let valid = UrlFields {
            scheme: "https".into(),
            host: "api.example.com".into(),
            port: 443,
            method: "GET".into(),
            userinfo: false,
        };
        assert!(allowed_web(&valid, &pol).is_ok());
    }

    #[test]
    fn web_rejects_private_ips_and_unknown_hosts() {
        let pol = sample_policy();
        let private = UrlFields {
            scheme: "https".into(),
            host: "10.0.0.5".into(),
            port: 443,
            method: "GET".into(),
            userinfo: false,
        };
        assert_eq!(allowed_web(&private, &pol), Err(WebError::PrivateHost));

        let unknown = UrlFields {
            scheme: "https".into(),
            host: "evil.com".into(),
            port: 443,
            method: "GET".into(),
            userinfo: false,
        };
        assert_eq!(allowed_web(&unknown, &pol), Err(WebError::Host));
    }

    #[test]
    fn dns_rebind_and_images() {
        assert!(allowed_dns("93.184.216.34", "93.184.216.34").is_ok());
        assert_eq!(allowed_dns("93.184.216.34", "10.0.0.5"), Err(WebError::PrivateHost));
        assert_eq!(allowed_dns("10.0.0.5", "10.0.0.5"), Err(WebError::PrivateHost));

        let images = vec!["sha256:python3.11-slim-locked".to_string()];
        assert!(allowed_code("sha256:python3.11-slim-locked", &images).is_ok());
        assert_eq!(allowed_code("sha256:other", &images), Err(WebError::ImageNotAllowed));
    }
}
