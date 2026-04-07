#![no_std]

#[repr(C)]
#[derive(Copy, Clone)]
pub struct NetworkPacketEvent {
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub tcp_flags: u16,
    pub payload_len: u16,
    pub payload: [u8; 9000],
}

#[cfg(feature = "userspace")]
unsafe impl aya::Pod for NetworkPacketEvent {}
