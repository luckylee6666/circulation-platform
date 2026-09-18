use std::net::{IpAddr, Ipv4Addr, UdpSocket};

use serde::Serialize;

/// 供控制台展示的一个可访问地址。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAddress {
    pub url: String,
    pub ip: String,
    pub interface: String,
    pub is_primary: bool,
}

/// 枚举本机可用于局域网访问的 IPv4 地址，主地址排在最前。
pub fn lan_addresses(port: u16) -> Vec<NetworkAddress> {
    let candidates = enumerate_ipv4();
    let primary = primary_local_ip();
    rank_addresses(candidates, primary, port)
}

/// 通过一次不产生流量的 UDP connect 让系统选出默认路由对应的本机地址。
/// 内网无法访问外网时，用常见的网关地址作为兜底探测目标。
pub fn primary_local_ip() -> Option<Ipv4Addr> {
    // 默认路由选出来的地址最可能是对的。但如果它落在组网工具的 CGNAT 网段上，
    // 办公网里的同事是访问不到的，这时退而选择真正的私有网段地址。
    let probed = probe_default_route();
    if let Some(ip) = probed
        && !is_cgnat(ip)
    {
        return Some(ip);
    }

    let private = enumerate_ipv4()
        .into_iter()
        .map(|(_, ip)| ip)
        .find(Ipv4Addr::is_private);

    private.or(probed)
}

fn probe_default_route() -> Option<Ipv4Addr> {
    const PROBES: [&str; 4] = ["8.8.8.8:80", "114.114.114.114:80", "192.168.1.1:80", "10.0.0.1:80"];

    for probe in PROBES {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        if socket.connect(probe).is_err() {
            continue;
        }
        if let Ok(local) = socket.local_addr()
            && let IpAddr::V4(ip) = local.ip()
            && is_usable(ip)
        {
            return Some(ip);
        }
    }
    None
}

fn enumerate_ipv4() -> Vec<(String, Ipv4Addr)> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };

    interfaces
        .into_iter()
        .filter_map(|interface| match interface.addr.ip() {
            IpAddr::V4(ip) if is_usable(ip) => Some((interface.name, ip)),
            _ => None,
        })
        .collect()
}

/// 排除局域网内不可互访的地址。
fn is_usable(ip: Ipv4Addr) -> bool {
    !ip.is_loopback()
        && !ip.is_unspecified()
        && !ip.is_broadcast()
        && !ip.is_link_local()
        && !ip.is_multicast()
        && !ip.is_documentation()
        && !is_proxy_fake_ip(ip)
}

/// 198.18.0.0/15 是 RFC 2544 的基准测试网段，常被代理软件占用来做 fake-IP。
///
/// 它长得像普通地址，但只有本机的代理能「到达」——浏览器走代理请求它会拿到 502，
/// 同事那边更是完全不通。列在访问地址里只会让人复制错。
fn is_proxy_fake_ip(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 198 && (octets[1] == 18 || octets[1] == 19)
}

/// 100.64.0.0/10 是运营商级 NAT 网段，Tailscale 之类的组网工具在用。
///
/// 地址本身是通的，但办公网里的同事通常访问不到，所以排在最后。
fn is_cgnat(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

/// 主地址优先，其次内网地址，再次公网地址，组网工具地址最后；
/// 同档次按接口名排序，保证每次输出稳定。
fn rank_addresses(
    mut candidates: Vec<(String, Ipv4Addr)>,
    primary: Option<Ipv4Addr>,
    port: u16,
) -> Vec<NetworkAddress> {
    candidates.sort_by(|a, b| {
        let rank = |ip: &Ipv4Addr| -> u8 {
            if Some(*ip) == primary {
                0
            } else if ip.is_private() {
                1
            } else if is_cgnat(*ip) {
                3
            } else {
                2
            }
        };
        rank(&a.1).cmp(&rank(&b.1)).then_with(|| a.0.cmp(&b.0))
    });

    candidates
        .into_iter()
        .map(|(interface, ip)| NetworkAddress {
            url: format!("http://{ip}:{port}"),
            ip: ip.to_string(),
            interface,
            is_primary: Some(ip) == primary,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(value: &str) -> Ipv4Addr {
        value.parse().unwrap()
    }

    #[test]
    fn filters_out_loopback_and_link_local() {
        assert!(!is_usable(ip("127.0.0.1")));
        assert!(!is_usable(ip("0.0.0.0")));
        assert!(!is_usable(ip("169.254.10.20")));
        assert!(!is_usable(ip("224.0.0.1")));
        assert!(is_usable(ip("192.168.1.88")));
        assert!(is_usable(ip("10.0.0.5")));
        assert!(is_usable(ip("172.16.3.4")));
    }

    #[test]
    fn proxy_fake_ip_range_is_excluded() {
        // 198.18.0.0/15 被代理软件当 fake-IP 用，只有本机代理能"到达"，
        // 出现在访问地址里会让人复制到一个打不开的地址
        assert!(!is_usable(ip("198.18.0.1")));
        assert!(!is_usable(ip("198.19.255.254")));
        assert!(is_proxy_fake_ip(ip("198.18.0.1")));
        assert!(is_proxy_fake_ip(ip("198.19.0.1")));
        assert!(!is_proxy_fake_ip(ip("198.20.0.1")));
        assert!(!is_proxy_fake_ip(ip("192.168.1.1")));
    }

    #[test]
    fn cgnat_is_recognised_and_ranked_last() {
        assert!(is_cgnat(ip("100.64.0.1")));
        assert!(is_cgnat(ip("100.88.64.76")));
        assert!(is_cgnat(ip("100.127.255.254")));
        assert!(!is_cgnat(ip("100.128.0.1")));
        assert!(!is_cgnat(ip("100.63.255.254")));
        assert!(!is_cgnat(ip("192.168.1.1")));

        // 组网工具地址排在公网地址之后，避免被当成首选复制出去
        let candidates = vec![
            ("utun0".to_string(), ip("100.88.64.76")),
            ("en0".to_string(), ip("172.20.23.2")),
            ("utun6".to_string(), ip("198.18.0.1")),
        ];
        let ranked = rank_addresses(candidates, Some(ip("172.20.23.2")), 8080);

        assert_eq!(ranked[0].ip, "172.20.23.2");
        assert!(ranked[0].is_primary);
        assert_eq!(ranked.last().unwrap().ip, "100.88.64.76");
    }

    #[test]
    fn primary_address_comes_first_and_is_flagged() {
        let candidates = vec![
            ("en1".to_string(), ip("10.8.0.3")),
            ("en0".to_string(), ip("192.168.1.88")),
            ("en2".to_string(), ip("203.0.113.9")),
        ];

        let ranked = rank_addresses(candidates, Some(ip("192.168.1.88")), 8080);

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].ip, "192.168.1.88");
        assert_eq!(ranked[0].url, "http://192.168.1.88:8080");
        assert!(ranked[0].is_primary);

        // 私有地址排在公网地址前面
        assert_eq!(ranked[1].ip, "10.8.0.3");
        assert_eq!(ranked[2].ip, "203.0.113.9");
        assert!(!ranked[1].is_primary);
    }

    #[test]
    fn without_primary_the_best_private_address_leads() {
        let candidates = vec![
            ("en2".to_string(), ip("203.0.113.9")),
            ("en0".to_string(), ip("192.168.1.88")),
        ];

        let ranked = rank_addresses(candidates, None, 9000);
        assert_eq!(ranked[0].ip, "192.168.1.88");
        assert_eq!(ranked[0].url, "http://192.168.1.88:9000");
        assert!(!ranked[0].is_primary);
    }

    #[test]
    fn ordering_is_stable_for_same_rank() {
        let candidates = vec![
            ("eth1".to_string(), ip("192.168.5.5")),
            ("eth0".to_string(), ip("192.168.5.6")),
        ];
        let ranked = rank_addresses(candidates, None, 8080);
        assert_eq!(ranked[0].interface, "eth0");
        assert_eq!(ranked[1].interface, "eth1");
    }
}
