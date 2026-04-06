#![no_std]
#![no_main]

use aya_ebpf::{
    macros::{classifier},
    programs::TcContext,
    bindings::TC_ACT_OK,
};
use driftmap_probe_common::PacketEvent;

#[classifier]
pub fn driftmap_tc(_ctx: TcContext) -> i32 {
    TC_ACT_OK
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
