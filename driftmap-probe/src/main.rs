#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::TC_ACT_OK,
    macros::{classifier, map},
    maps::{HashMap, RingBuf},
    programs::TcContext,
};
use aya_log_ebpf::info;
use core::mem;
use driftmap_probe_common::PacketEvent;
use network_types::{
    eth::{EtherType, EthHdr},
    ip::{IpProto, Ipv4Hdr},
    tcp::TcpHdr,
};

#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(4 * 1024 * 1024, 0);

#[map]
static WATCHED_PORTS: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

#[classifier]
pub fn driftmap_tc(ctx: TcContext) -> i32 {
    match try_driftmap_tc(ctx) {
        Ok(ret) => ret,
        Err(_) => TC_ACT_OK,
    }
}

fn try_driftmap_tc(ctx: TcContext) -> Result<i32, ()> {
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

    if WATCHED_PORTS.get(&(src_port as u32)).is_none()
        && WATCHED_PORTS.get(&(dst_port as u32)).is_none() {
        return Ok(TC_ACT_OK);
    }

    let payload_offset = tcp_offset + (unsafe { (*tcp_hdr).doff() as usize } * 4);
    let payload_len = (ctx.len() as usize).saturating_sub(payload_offset).min(1500);

    if payload_len == 0 {
        return Ok(TC_ACT_OK);
    }

    if let Some(mut event) = EVENTS.reserve::<PacketEvent>(0) {
        let ev = event.as_mut_ptr();
        unsafe {
            (*ev).src_ip = src_ip;
            (*ev).dst_ip = dst_ip;
            (*ev).src_port = src_port;
            (*ev).dst_port = dst_port;
            (*ev).payload_len = payload_len as u16;
            ctx.load_bytes(payload_offset, &mut (*ev).payload[..payload_len]).map_err(|_| {
                event.discard(0);
            })?;
        }
        event.submit(0);
    }

    Ok(TC_ACT_OK)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
