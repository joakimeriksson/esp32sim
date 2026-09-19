//! Packet construction shared by the local subnet and its host NAT transport.

pub(crate) const GATEWAY_MAC: [u8; 6] = [0x02, 0x53, 0x49, 0x4d, 0x00, 0x02];

pub(crate) fn checksum(data: &[u8], init: u32) -> u16 {
    let mut sum = init;
    let mut words = data.chunks_exact(2);
    for word in &mut words { sum += u16::from_be_bytes([word[0], word[1]]) as u32; }
    if let [last] = words.remainder() { sum += (*last as u32) << 8; }
    while sum >> 16 != 0 { sum = (sum & 0xffff) + (sum >> 16); }
    !(sum as u16)
}

pub(crate) fn transport_checksum(src: &[u8; 4], dst: &[u8; 4], proto: u8, data: &[u8]) -> u16 {
    let mut sum = proto as u32 + data.len() as u32;
    for address in [src, dst] {
        sum += u16::from_be_bytes([address[0], address[1]]) as u32;
        sum += u16::from_be_bytes([address[2], address[3]]) as u32;
    }
    checksum(data, sum)
}

pub(crate) fn ethernet(dst: &[u8; 6], src: &[u8; 6], ethertype: u16, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(14 + payload.len());
    frame.extend_from_slice(dst);
    frame.extend_from_slice(src);
    frame.extend_from_slice(&ethertype.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Build an IPv4 packet without options.
pub(crate) fn ip_packet(proto: u8, src: &[u8; 4], dst: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let total = 20 + payload.len();
    let mut packet = Vec::with_capacity(total);
    packet.extend_from_slice(&[0x45, 0]);
    packet.extend_from_slice(&(total as u16).to_be_bytes());
    packet.extend_from_slice(&[0, 0, 0x40, 0, 64, proto, 0, 0]);
    packet.extend_from_slice(src);
    packet.extend_from_slice(dst);
    let check = checksum(&packet, 0);
    packet[10..12].copy_from_slice(&check.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

pub(crate) fn udp_packet(src: &[u8; 4], dst: &[u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let len = 8 + payload.len();
    let mut packet = Vec::with_capacity(len);
    packet.extend_from_slice(&sport.to_be_bytes());
    packet.extend_from_slice(&dport.to_be_bytes());
    packet.extend_from_slice(&(len as u16).to_be_bytes());
    packet.extend_from_slice(&[0, 0]);
    packet.extend_from_slice(payload);
    let check = transport_checksum(src, dst, 17, &packet);
    packet[6..8].copy_from_slice(&if check == 0 { 0xffff } else { check }.to_be_bytes());
    packet
}
