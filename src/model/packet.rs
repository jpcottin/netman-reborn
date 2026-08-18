//! Décodage d'une trame brute en `PacketMeta` via etherparse.
//!
//! Exécuté dans le thread de capture : aucune allocation évitable, aucune IO,
//! aucune résolution (invariant 4). Une trame → une struct compacte.

use std::fmt;
use std::net::IpAddr;

use etherparse::{LinkSlice, NetSlice, SlicedPacket, TransportSlice};

/// Adresse MAC affichable (`aa:bb:cc:dd:ee:ff`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Mac(pub [u8; 6]);

impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.0;
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            m[0], m[1], m[2], m[3], m[4], m[5]
        )
    }
}

/// Étiquette de protocole, compacte et hashable (pas d'allocation).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Proto {
    /// Protocole identifié (EtherType connu, L4 connu ou applicatif déduit du port).
    Named(&'static str),
    /// EtherType inconnu (affiché en hexa).
    Ether(u16),
    /// Numéro de protocole IP inconnu.
    Ip(u8),
}

impl fmt::Display for Proto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Proto::Named(s) => f.write_str(s),
            Proto::Ether(v) => write!(f, "ethertype-0x{v:04x}"),
            Proto::Ip(v) => write!(f, "ipproto-{v}"),
        }
    }
}

/// Métadonnées L2 (vue Etherman). Absentes d'un paquet lu sur un tun
/// (Android) : il n'y a pas d'Ethernet sur un tunnel de niveau 3.
#[derive(Clone, Copy, Debug)]
pub struct L2Meta {
    pub src_mac: Mac,
    pub dst_mac: Mac,
    /// Protocole vu de la couche 2 (EtherType, éventuellement affiné).
    pub proto: Proto,
}

/// Ports transport (TCP/UDP uniquement) : nécessaires à l'attribution d'un
/// flux à une application (Android, `getConnectionOwnerUid`). Les autres
/// protocoles n'ont pas de ports — et ne sont pas attribuables.
#[derive(Clone, Copy, Debug)]
pub struct TransportMeta {
    /// Numéro de protocole IP (6 = TCP, 17 = UDP).
    pub ip_number: u8,
    pub src_port: u16,
    pub dst_port: u16,
}

/// Métadonnées L3 (vue Interman).
#[derive(Clone, Copy, Debug)]
pub struct L3Meta {
    pub src: IpAddr,
    pub dst: IpAddr,
    pub proto: Proto,
    /// Présent pour TCP/UDP uniquement.
    pub transport: Option<TransportMeta>,
}

/// Métadonnées d'une trame, extraites une seule fois puis projetées sur les
/// deux tables (invariant 1 : une capture, deux agrégations).
#[derive(Clone, Copy, Debug)]
pub struct PacketMeta {
    /// Longueur réelle sur le fil (`PacketHeader::len`).
    pub wire_len: u64,
    /// Présent quand la source est une trame Ethernet (capture pcap) ;
    /// `None` pour un paquet IP nu (tun Android) — la vue L2 reste vide.
    pub l2: Option<L2Meta>,
    /// Présent pour IPv4/IPv6 uniquement (ARP reste une affaire L2).
    pub l3: Option<L3Meta>,
    /// Paire IP↔MAC apprise passivement (ARP) — pour la résolution best-effort.
    pub arp_pair: Option<(IpAddr, Mac)>,
}

/// Décode une trame Ethernet. `None` si la trame est inexploitable
/// (pas Ethernet II, tronquée au point d'être illisible…).
pub fn parse_frame(data: &[u8], wire_len: u32) -> Option<PacketMeta> {
    let sliced = SlicedPacket::from_ethernet(data).ok()?;

    let eth = match &sliced.link {
        Some(LinkSlice::Ethernet2(eth)) => eth,
        _ => return None,
    };
    let src_mac = Mac(eth.source());
    let dst_mac = Mac(eth.destination());

    let (l3, arp_pair, refined) = net_meta(&sliced);
    let l2_proto = refined.unwrap_or(Proto::Ether(eth.ether_type().0));

    Some(PacketMeta {
        wire_len: u64::from(wire_len),
        l2: Some(L2Meta {
            src_mac,
            dst_mac,
            proto: l2_proto,
        }),
        l3,
        arp_pair,
    })
}

/// Décode un paquet IP nu (sans couche liaison) : lecture d'un tun Android,
/// dont chaque `read()` rend exactement un datagramme IPv4 ou IPv6 (pas de
/// préfixe packet-info sous Linux/Android). `None` si inexploitable.
pub fn parse_ip_packet(data: &[u8], wire_len: u32) -> Option<PacketMeta> {
    let sliced = SlicedPacket::from_ip(data).ok()?;
    let (l3, _, _) = net_meta(&sliced);
    // Sans L2 ni L3, le paquet n'alimente aucune vue : inexploitable.
    Some(PacketMeta {
        wire_len: u64::from(wire_len),
        l2: None,
        l3: Some(l3?),
        arp_pair: None,
    })
}

/// Partie commune aux deux points d'entrée : métadonnées réseau (IPv4, IPv6,
/// ARP). Renvoie aussi le protocole L2 « affiné » quand la couche réseau est
/// identifiée (trames Ethernet uniquement).
fn net_meta(sliced: &SlicedPacket<'_>) -> (Option<L3Meta>, Option<(IpAddr, Mac)>, Option<Proto>) {
    match &sliced.net {
        Some(NetSlice::Ipv4(ipv4)) => {
            let header = ipv4.header();
            let (proto, transport) = l4_meta(sliced, ipv4.payload_ip_number().0);
            (
                Some(L3Meta {
                    src: IpAddr::V4(header.source_addr()),
                    dst: IpAddr::V4(header.destination_addr()),
                    proto,
                    transport,
                }),
                None,
                Some(Proto::Named("IPv4")),
            )
        }
        Some(NetSlice::Ipv6(ipv6)) => {
            let header = ipv6.header();
            // Protocole réel après les en-têtes d'extension (pas next_header()).
            let (proto, transport) = l4_meta(sliced, ipv6.payload().ip_number.0);
            (
                Some(L3Meta {
                    src: IpAddr::V6(header.source_addr()),
                    dst: IpAddr::V6(header.destination_addr()),
                    proto,
                    transport,
                }),
                None,
                Some(Proto::Named("IPv6")),
            )
        }
        Some(NetSlice::Arp(arp)) => {
            // Paire IP↔MAC apprise du sender (best-effort, IPv4 seulement).
            let mut arp_pair = None;
            if let (Ok(spa), Ok(sha)) = (
                <[u8; 4]>::try_from(arp.sender_protocol_addr()),
                <[u8; 6]>::try_from(arp.sender_hw_addr()),
            ) {
                let ip = IpAddr::V4(spa.into());
                if !ip.is_unspecified() {
                    arp_pair = Some((ip, Mac(sha)));
                }
            }
            (None, arp_pair, Some(Proto::Named("ARP")))
        }
        None => (None, None, None),
    }
}

/// Protocole L4 (affiné en protocole applicatif via les ports connus) et
/// ports transport quand il y en a.
fn l4_meta(sliced: &SlicedPacket<'_>, ip_number: u8) -> (Proto, Option<TransportMeta>) {
    match &sliced.transport {
        Some(TransportSlice::Tcp(tcp)) => (
            app_proto(true, tcp.source_port(), tcp.destination_port()),
            Some(TransportMeta {
                ip_number: 6,
                src_port: tcp.source_port(),
                dst_port: tcp.destination_port(),
            }),
        ),
        Some(TransportSlice::Udp(udp)) => (
            app_proto(false, udp.source_port(), udp.destination_port()),
            Some(TransportMeta {
                ip_number: 17,
                src_port: udp.source_port(),
                dst_port: udp.destination_port(),
            }),
        ),
        Some(TransportSlice::Icmpv4(_)) => (Proto::Named("ICMP"), None),
        Some(TransportSlice::Icmpv6(_)) => (Proto::Named("ICMPv6"), None),
        None => (
            match ip_number {
                2 => Proto::Named("IGMP"),
                6 => Proto::Named("TCP"),
                17 => Proto::Named("UDP"),
                50 => Proto::Named("ESP"),
                58 => Proto::Named("ICMPv6"),
                132 => Proto::Named("SCTP"),
                other => Proto::Ip(other),
            },
            None,
        ),
    }
}

/// Déduit le protocole applicatif des ports (le port « connu » gagne, priorité
/// au port destination). Repli : TCP ou UDP.
fn app_proto(tcp: bool, sport: u16, dport: u16) -> Proto {
    well_known_port(tcp, dport)
        .or_else(|| well_known_port(tcp, sport))
        .unwrap_or(Proto::Named(if tcp { "TCP" } else { "UDP" }))
}

fn well_known_port(tcp: bool, port: u16) -> Option<Proto> {
    let name = match (tcp, port) {
        (_, 53) => "DNS",
        (true, 80) => "HTTP",
        (true, 443) => "HTTPS",
        (false, 443) => "QUIC",
        (true, 22) => "SSH",
        (true, 21) => "FTP",
        (true, 23) => "Telnet",
        (true, 25) | (true, 465) | (true, 587) => "SMTP",
        (true, 110) | (true, 995) => "POP3",
        (true, 143) | (true, 993) => "IMAP",
        (false, 67) | (false, 68) => "DHCP",
        (false, 546) | (false, 547) => "DHCPv6",
        (false, 123) => "NTP",
        (false, 5353) => "mDNS",
        (false, 5355) => "LLMNR",
        (false, 1900) => "SSDP",
        (false, 137) | (false, 138) => "NetBIOS",
        (true, 139) | (true, 445) => "SMB",
        (true, 3389) => "RDP",
        (false, 514) => "Syslog",
        (false, 161) | (false, 162) => "SNMP",
        (false, 500) | (false, 4500) => "IPsec-IKE",
        (false, 51820) => "WireGuard",
        (true, 8080) | (true, 8000) => "HTTP",
        (true, 8443) => "HTTPS",
        _ => return None,
    };
    Some(Proto::Named(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use etherparse::PacketBuilder;

    const MAC_A: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x0a];
    const MAC_B: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x0b];

    #[test]
    fn parse_ipv4_tcp_https() {
        let builder = PacketBuilder::ethernet2(MAC_A, MAC_B)
            .ipv4([192, 168, 0, 21], [93, 184, 216, 34], 64)
            .tcp(51234, 443, 1000, 64240);
        let mut frame = Vec::new();
        builder.write(&mut frame, &[1, 2, 3]).unwrap();

        let meta = parse_frame(&frame, frame.len() as u32).unwrap();
        let l2 = meta.l2.unwrap();
        assert_eq!(l2.src_mac, Mac(MAC_A));
        assert_eq!(l2.dst_mac, Mac(MAC_B));
        assert_eq!(l2.proto, Proto::Named("IPv4"));
        let l3 = meta.l3.unwrap();
        assert_eq!(l3.src, "192.168.0.21".parse::<IpAddr>().unwrap());
        assert_eq!(l3.dst, "93.184.216.34".parse::<IpAddr>().unwrap());
        assert_eq!(l3.proto, Proto::Named("HTTPS"));
        let transport = l3.transport.unwrap();
        assert_eq!(
            (transport.ip_number, transport.src_port, transport.dst_port),
            (6, 51234, 443)
        );
        assert!(meta.arp_pair.is_none());
    }

    #[test]
    fn parse_ipv6_udp_mdns() {
        let src = "fe80::1".parse::<std::net::Ipv6Addr>().unwrap();
        let dst = "ff02::fb".parse::<std::net::Ipv6Addr>().unwrap();
        let builder = PacketBuilder::ethernet2(MAC_A, MAC_B)
            .ipv6(src.octets(), dst.octets(), 255)
            .udp(5353, 5353);
        let mut frame = Vec::new();
        builder.write(&mut frame, &[0u8; 10]).unwrap();

        let meta = parse_frame(&frame, frame.len() as u32).unwrap();
        assert_eq!(meta.l2.unwrap().proto, Proto::Named("IPv6"));
        let l3 = meta.l3.unwrap();
        assert_eq!(l3.src, IpAddr::V6(src));
        assert_eq!(l3.proto, Proto::Named("mDNS"));
        assert_eq!(l3.transport.unwrap().ip_number, 17);
    }

    #[test]
    fn parse_arp_request() {
        // Trame ARP forgée à la main (etherparse la décode, on ne la construit pas).
        let mut frame = Vec::new();
        frame.extend_from_slice(&[0xff; 6]); // dst broadcast
        frame.extend_from_slice(&MAC_A); // src
        frame.extend_from_slice(&[0x08, 0x06]); // EtherType ARP
        frame.extend_from_slice(&[0x00, 0x01]); // htype ethernet
        frame.extend_from_slice(&[0x08, 0x00]); // ptype ipv4
        frame.extend_from_slice(&[6, 4]); // hlen, plen
        frame.extend_from_slice(&[0x00, 0x01]); // oper request
        frame.extend_from_slice(&MAC_A); // sender hw
        frame.extend_from_slice(&[192, 168, 0, 21]); // sender ip
        frame.extend_from_slice(&[0u8; 6]); // target hw
        frame.extend_from_slice(&[192, 168, 0, 1]); // target ip

        let meta = parse_frame(&frame, frame.len() as u32).unwrap();
        assert_eq!(meta.l2.unwrap().proto, Proto::Named("ARP"));
        assert!(meta.l3.is_none(), "ARP must not feed the L3 table");
        let (ip, mac) = meta.arp_pair.unwrap();
        assert_eq!(ip, "192.168.0.21".parse::<IpAddr>().unwrap());
        assert_eq!(mac, Mac(MAC_A));
    }

    #[test]
    fn parse_vlan_tagged_ipv4() {
        let builder = PacketBuilder::ethernet2(MAC_A, MAC_B)
            .single_vlan(42.try_into().unwrap())
            .ipv4([10, 0, 0, 1], [10, 0, 0, 2], 64)
            .udp(12345, 53);
        let mut frame = Vec::new();
        builder.write(&mut frame, &[0u8; 4]).unwrap();

        let meta = parse_frame(&frame, frame.len() as u32).unwrap();
        // La trame taguée reste décodée jusqu'à L4.
        assert_eq!(meta.l2.unwrap().proto, Proto::Named("IPv4"));
        let l3 = meta.l3.unwrap();
        assert_eq!(l3.proto, Proto::Named("DNS"));
    }

    #[test]
    fn parse_garbage_returns_none() {
        assert!(parse_frame(&[0u8; 5], 5).is_none());
    }

    #[test]
    fn parse_raw_ipv4_tcp_https() {
        // Paquet IP nu, sans Ethernet : ce que lit un tun Android.
        let builder = PacketBuilder::ipv4([192, 168, 0, 21], [93, 184, 216, 34], 64)
            .tcp(51234, 443, 1000, 64240);
        let mut packet = Vec::new();
        builder.write(&mut packet, &[1, 2, 3]).unwrap();

        let meta = parse_ip_packet(&packet, packet.len() as u32).unwrap();
        assert!(meta.l2.is_none(), "pas de couche 2 sur un tun");
        assert!(meta.arp_pair.is_none());
        let l3 = meta.l3.unwrap();
        assert_eq!(l3.src, "192.168.0.21".parse::<IpAddr>().unwrap());
        assert_eq!(l3.dst, "93.184.216.34".parse::<IpAddr>().unwrap());
        assert_eq!(l3.proto, Proto::Named("HTTPS"));
        let transport = l3.transport.unwrap();
        assert_eq!(
            (transport.ip_number, transport.src_port, transport.dst_port),
            (6, 51234, 443)
        );
    }

    #[test]
    fn parse_raw_ipv6_udp_dns() {
        let src = "fd00::1".parse::<std::net::Ipv6Addr>().unwrap();
        let dst = "2001:4860:4860::8888"
            .parse::<std::net::Ipv6Addr>()
            .unwrap();
        let builder = PacketBuilder::ipv6(src.octets(), dst.octets(), 64).udp(50123, 53);
        let mut packet = Vec::new();
        builder.write(&mut packet, &[0u8; 30]).unwrap();

        let meta = parse_ip_packet(&packet, packet.len() as u32).unwrap();
        assert!(meta.l2.is_none());
        let l3 = meta.l3.unwrap();
        assert_eq!(l3.src, IpAddr::V6(src));
        assert_eq!(l3.proto, Proto::Named("DNS"));
        assert_eq!(l3.transport.unwrap().ip_number, 17);
    }

    #[test]
    fn parse_raw_garbage_returns_none() {
        assert!(parse_ip_packet(&[0u8; 3], 3).is_none());
        // Nibble de version invalide (0) : ni IPv4 ni IPv6.
        assert!(parse_ip_packet(&[0u8; 40], 40).is_none());
    }

    #[test]
    fn proto_display() {
        assert_eq!(Proto::Named("DNS").to_string(), "DNS");
        assert_eq!(Proto::Ether(0x88cc).to_string(), "ethertype-0x88cc");
        assert_eq!(Proto::Ip(89).to_string(), "ipproto-89");
        assert_eq!(Mac(MAC_A).to_string(), "02:00:00:00:00:0a");
    }
}
