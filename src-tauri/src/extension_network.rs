//! Shared network boundary for declarative adapters and downloads.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && (b == 168 || (b == 0 && c == 0) || (b == 0 && c == 2)))
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_ipv4(ip),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_ipv4(mapped);
            }
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
    }
}

pub(crate) fn safe_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.as_str().len() < 2048
        && host.contains('.')
        && host.parse::<IpAddr>().is_err()
        && ![".local", ".localhost", ".internal", ".test", ".invalid"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

/// Build a request client pinned to public DNS answers, with no redirects or proxy.
pub(crate) async fn secure_https_client(
    url: &url::Url,
    timeout: Duration,
) -> Result<reqwest::Client, String> {
    if !safe_url(url.as_str()) {
        return Err("URL de origem insegura.".into());
    }
    let hostname = url.host_str().ok_or("Domínio ausente.")?.to_string();
    let resolved: Vec<SocketAddr> = tokio::net::lookup_host((hostname.as_str(), 443))
        .await
        .map_err(|error| format!("DNS: {error}"))?
        .take(24)
        .collect();
    if resolved.is_empty() || resolved.iter().any(|addr| !public_ip(addr.ip())) {
        return Err("DNS apontou para IP interno ou inválido; conexão bloqueada.".into());
    }
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&hostname, &resolved)
        .timeout(timeout)
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prevents_private_networks_and_unsafe_urls() {
        for ip in [
            "127.0.0.1",
            "10.0.0.3",
            "192.168.1.1",
            "169.254.169.254",
            "172.19.0.1",
            "100.64.0.1",
            "::1",
            "fc00::1",
        ] {
            assert!(
                !public_ip(ip.parse().unwrap()),
                "IP should be blocked: {ip}"
            );
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        assert!(!safe_url("http://example.org/"));
        assert!(!safe_url("https://127.0.0.1/"));
        assert!(!safe_url("https://example.org:8443/"));
    }
}
