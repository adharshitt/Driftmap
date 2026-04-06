#![no_std]

#[repr(C)]
#[derive(Copy, Clone)]
pub struct NetworkPacketEvent {
    pub src_ip:      [u8; 4],
    pub dst_ip:      [u8; 4],
    pub src_port:    u16,
    pub dst_port:    u16,
    pub payload_len: u16,
    pub payload:     [u8; 1500],
}

#[application_config(feature = "userspace")]
unsafe impl aya::Pod for NetworkPacketEvent {}
