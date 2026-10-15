//! Broad-scope tool bounds (krytyk muse + hakerzy): **nie ma „ogólnego" fetch/run**.
//! LLM proponuje URL/skrypt, a **Rust waliduje** wobec wąskiej polityki. To jest
//! uzupełnienie I2: sufitu nie da się utrzymać, jeśli `web_fetch(url: String)` i
//! `code_exec(code: String)` są szerokie.
//!
//! Uwaga: ten moduł waliduje POLA już sparsowane. Twardy parser URL (obfuskacje,
//! IDN, backslashe) musi być po stronie wołającego — tu testujemy politykę.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// Pola URL po parsowaniu (wołający musi użyć twardego parsera).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolaUrl {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub metoda: String,
    /// Czy URL zawiera userinfo (`user:pass@host`) — zawsze odrzucamy.
    pub userinfo: bool,
}

/// Polityka dla `web_fetch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolitykaWeb {
    /// Dokładne hosty lub sufiksy domenowe (`.example.com`).
    pub dozwolone_hosty: Vec<String>,
    pub max_bajtow: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BladWeb {
    Scheme,
    Port,
    Userinfo,
    Metoda,
    Host,
    HostPrywatny,
    DnsNiezgodny,
    ObrazNiedozwolony,
}

fn prywatny_ip(s: &str) -> bool {
    let czysty = s.trim_start_matches('[').trim_end_matches(']');
    match czysty.parse::<IpAddr>() {
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
        Err(_) => false, // nazwa domenowa, nie literał IP
    }
}

/// Czy host pasuje do allowlisty (dokładnie albo jako subdomena).
pub fn host_dozwolony(host: &str, lista: &[String]) -> bool {
    let h = host.to_ascii_lowercase();
    lista.iter().any(|wzorzec| {
        let w = wzorzec.to_ascii_lowercase();
        let bez_kropki = w.trim_start_matches('.');
        h == bez_kropki || h.ends_with(&format!(".{bez_kropki}"))
    })
}

/// Waliduje propozycję `web_fetch` wobec polityki.
pub fn dozwolony_web(p: &PolaUrl, pol: &PolitykaWeb) -> Result<(), BladWeb> {
    if p.scheme != "https" {
        return Err(BladWeb::Scheme);
    }
    if p.port != 443 {
        return Err(BladWeb::Port);
    }
    if p.userinfo {
        return Err(BladWeb::Userinfo);
    }
    if !p.metoda.eq_ignore_ascii_case("GET") {
        return Err(BladWeb::Metoda);
    }
    if prywatny_ip(&p.host) {
        return Err(BladWeb::HostPrywatny);
    }
    if !host_dozwolony(&p.host, &pol.dozwolone_hosty) {
        return Err(BladWeb::Host);
    }
    Ok(())
}

/// Kontrola DNS-rebind: IP z rozwiązania musi równać się IP, z którym się łączymy,
/// i nie może być prywatne.
pub fn dozwolony_dns(resolved_ip: &str, connect_ip: &str) -> Result<(), BladWeb> {
    if resolved_ip != connect_ip {
        return Err(BladWeb::DnsNiezgodny);
    }
    if prywatny_ip(connect_ip) {
        return Err(BladWeb::HostPrywatny);
    }
    Ok(())
}

/// Waliduje propozycję uruchomienia kodu wobec polityki obrazów.
pub fn dozwolony_code(obraz_digest: &str, dozwolone_obrazy: &[String]) -> Result<(), BladWeb> {
    if obraz_digest.is_empty() || !dozwolone_obrazy.iter().any(|o| o == obraz_digest) {
        return Err(BladWeb::ObrazNiedozwolony);
    }
    Ok(())
}

#[cfg(test)]
mod testy {
    use super::*;

    fn pol() -> PolitykaWeb {
        PolitykaWeb {
            dozwolone_hosty: vec!["api.example.com".into(), "en.wikipedia.org".into()],
            max_bajtow: 1 << 20,
        }
    }

    fn url(scheme: &str, host: &str, port: u16, method: &str) -> PolaUrl {
        PolaUrl {
            scheme: scheme.into(),
            host: host.into(),
            port,
            metoda: method.into(),
            userinfo: false,
        }
    }

    #[test]
    fn web_dozwolony_tylko_waska_polityka() {
        assert!(dozwolony_web(&url("https", "api.example.com", 443, "GET"), &pol()).is_ok());
        assert!(dozwolony_web(&url("https", "en.wikipedia.org", 443, "get"), &pol()).is_ok());
        // subdomena dozwolonego hosta
        assert!(dozwolony_web(&url("https", "sub.api.example.com", 443, "GET"), &pol()).is_ok());
    }

    #[test]
    fn web_odrzuca_scheme_port_metode_userinfo() {
        assert_eq!(dozwolony_web(&url("http", "api.example.com", 443, "GET"), &pol()), Err(BladWeb::Scheme));
        assert_eq!(dozwolony_web(&url("https", "api.example.com", 8080, "GET"), &pol()), Err(BladWeb::Port));
        assert_eq!(dozwolony_web(&url("https", "api.example.com", 443, "POST"), &pol()), Err(BladWeb::Metoda));
        let mut u = url("https", "api.example.com", 443, "GET");
        u.userinfo = true;
        assert_eq!(dozwolony_web(&u, &pol()), Err(BladWeb::Userinfo));
    }

    #[test]
    fn web_odrzuca_prywatne_ip_i_hosty_spoza_listy() {
        assert_eq!(dozwolony_web(&url("https", "10.0.0.5", 443, "GET"), &pol()), Err(BladWeb::HostPrywatny));
        assert_eq!(dozwolony_web(&url("https", "169.254.169.254", 443, "GET"), &pol()), Err(BladWeb::HostPrywatny));
        assert_eq!(dozwolony_web(&url("https", "127.0.0.1", 443, "GET"), &pol()), Err(BladWeb::HostPrywatny));
        assert_eq!(dozwolony_web(&url("https", "::ffff:127.0.0.1", 443, "GET"), &pol()), Err(BladWeb::HostPrywatny));
        assert_eq!(dozwolony_web(&url("https", "fc00::1", 443, "GET"), &pol()), Err(BladWeb::HostPrywatny));
        assert_eq!(dozwolony_web(&url("https", "fe80::1", 443, "GET"), &pol()), Err(BladWeb::HostPrywatny));
        assert_eq!(dozwolony_web(&url("https", "[::1]", 443, "GET"), &pol()), Err(BladWeb::HostPrywatny));
        assert_eq!(dozwolony_web(&url("https", "evil.example.net", 443, "GET"), &pol()), Err(BladWeb::Host));
        // próba obfuskacji: "api.example.com.evil.net" NIE jest subdomeną api.example.com
        assert_eq!(dozwolony_web(&url("https", "api.example.com.evil.net", 443, "GET"), &pol()), Err(BladWeb::Host));
    }

    #[test]
    fn host_dozwolony_ze_wzorcem_z_kropka() {
        let p = vec![".example.com".to_string()];
        assert!(host_dozwolony("example.com", &p));
        assert!(host_dozwolony("api.example.com", &p));
        assert!(host_dozwolony("sub.api.example.com", &p));
        assert!(!host_dozwolony("other.org", &p));
        assert!(!host_dozwolony("notexample.com", &p));
    }

    #[test]
    fn dns_rebind_i_obrazy() {
        assert!(dozwolony_dns("93.184.216.34", "93.184.216.34").is_ok());
        assert_eq!(dozwolony_dns("93.184.216.34", "10.0.0.5"), Err(BladWeb::DnsNiezgodny));
        assert_eq!(dozwolony_dns("10.0.0.5", "10.0.0.5"), Err(BladWeb::HostPrywatny));

        let obrazy = vec!["sha256:python3.11-slim-locked".to_string()];
        assert!(dozwolony_code("sha256:python3.11-slim-locked", &obrazy).is_ok());
        assert_eq!(dozwolony_code("sha256:other", &obrazy), Err(BladWeb::ObrazNiedozwolony));
        assert_eq!(dozwolony_code("", &obrazy), Err(BladWeb::ObrazNiedozwolony));
    }
}
