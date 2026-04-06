#![no_std]

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PacketEvent {
    pub src_port:    u16,
    pub dst_port:    u16,
    pub payload_len: u16,
    pub payload:     [u8; 1500],
}

#[cfg(feature = "userspace")]
unsafe impl aya::Pod for PacketEvent {}
