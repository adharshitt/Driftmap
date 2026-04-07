#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::TC_ACT_OK,
    macros::{classifier, map},
    maps::{HashMap, RingBuf},
    programs::TcContext,
};
use driftmap_probe_common::NetworkPacketEvent;
use network_types::{
    eth::{EtherType, EthHdr},
    ip::{IpProto, Ipv4Hdr},
    tcp::TcpHdr,
};

#[map]
static PACKET_EVENT_RING_BUFFER: RingBuf = RingBuf::with_byte_size(4 * 1024 * 1024, 0);

#[map]
static FILTERED_PORT_REGISTRY: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

#[map]
static DROPPED_PACKETS: HashMap<u32, u64> = HashMap::with_max_entries(1, 0);

#[classifier]
pub fn intercept_traffic_control_hook(ctx: TcContext) -> i32 {
    match evaluate_network_packet(ctx) {
        Ok(ret) => ret,
        Err(_) => TC_ACT_OK,
    }
}

fn evaluate_network_packet(ctx: TcContext) -> Result<i32, ()> {
    let eth_hdr: *const EthHdr = ctx.load(0).map_err(|_| ())?;
    if unsafe { (*eth_hdr).ether_type } != EtherType::Ipv4 {
        return Ok(TC_ACT_OK);
    }

    let ip_hdr: *const Ipv4Hdr = ctx.load(EthHdr::LEN).map_err(|_| ())?;
    if unsafe { (*ip_hdr).proto } != IpProto::Tcp {
        return Ok(TC_ACT_OK);
    }

    let src_ip = unsafe { (*ip_hdr).src_addr.to_be_bytes() };
    let dst_ip = unsafe { (*ip_hdr).dst_addr.to_be_bytes() };

    let tcp_offset = EthHdr::LEN + Ipv4Hdr::LEN;
    let tcp_hdr: *const TcpHdr = ctx.load(tcp_offset).map_err(|_| ())?;

    let src_port = u16::from_be(unsafe { (*tcp_hdr).source });
    let dst_port = u16::from_be(unsafe { (*tcp_hdr).dest });
    let seq = u32::from_be(unsafe { (*tcp_hdr).seq });
    let ack = u32::from_be(unsafe { (*tcp_hdr).ack_seq });

    let src_watched = unsafe { FILTERED_PORT_REGISTRY.get(&(src_port as u32)).is_some() };
    let dst_watched = unsafe { FILTERED_PORT_REGISTRY.get(&(dst_port as u32)).is_some() };

    if !src_watched && !dst_watched {
        return Ok(TC_ACT_OK);
    }

    let tcp_flags = unsafe {
        let flags_ptr = (tcp_hdr as *const u8).add(12) as *const u16;
        u16::from_be(*flags_ptr) & 0x0FFF
    };

    let payload_offset = tcp_offset + (unsafe { (*tcp_hdr).doff() as usize } * 4);
    let payload_len = (ctx.len() as usize).saturating_sub(payload_offset).min(9000);

    // FIN = 0x001, RST = 0x004
    let is_term = (tcp_flags & 0x001) != 0 || (tcp_flags & 0x004) != 0;

    if payload_len == 0 && !is_term {
        return Ok(TC_ACT_OK);
    }

    if let Some(mut event) = PACKET_EVENT_RING_BUFFER.reserve::<NetworkPacketEvent>(0) {
        let ev = event.as_mut_ptr();
        unsafe {
            (*ev).src_ip = src_ip;
            (*ev).dst_ip = dst_ip;
            (*ev).src_port = src_port;
            (*ev).dst_port = dst_port;
            (*ev).seq = seq;
            (*ev).ack = ack;
            (*ev).tcp_flags = tcp_flags;
            (*ev).payload_len = payload_len as u16;
            if payload_len > 0 {
                if ctx.load_bytes(payload_offset, &mut (&mut (*ev).payload)[..payload_len]).is_err() {
                    event.discard(0);
                    return Ok(TC_ACT_OK);
                }
            }
        }
        event.submit(0);
    } else {
        // Increment dropped packets counter
        let key = 0u32;
        if let Some(count) = DROPPED_PACKETS.get_ptr_mut(&key) {
            unsafe { *count += 1 };
        } else {
            let _ = DROPPED_PACKETS.insert(&key, &1, 0);
        }
    }

    Ok(TC_ACT_OK)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
