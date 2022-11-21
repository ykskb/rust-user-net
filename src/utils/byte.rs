const TARGET_BIG_ENDIAN: bool = cfg!(target_endian = "big");

fn byte_swap_u16(v: u16) -> u16 {
    (v & 0x00ff) << 8 | (v & 0xff00) >> 8
}

fn byte_swap_u32(v: u32) -> u32 {
    (v & 0x000000ff) << 24 | (v & 0x0000ff00) << 8 | (v & 0x00ff0000) >> 8 | (v & 0xff000000) >> 24
}

/// Converts big endian u16 to little endian if a target is a little endian machine.
pub fn be_to_le_u16(v: u16) -> u16 {
    if TARGET_BIG_ENDIAN {
        return v;
    }
    byte_swap_u16(v)
}

/// Converts little endian u16 to big endian if a target is a little endian machine.
pub fn le_to_be_u16(v: u16) -> u16 {
    if TARGET_BIG_ENDIAN {
        return v;
    }
    byte_swap_u16(v)
}

/// Converts big endian u32 to little endian if a target is a little endian machine.
pub fn be_to_le_u32(v: u32) -> u32 {
    if TARGET_BIG_ENDIAN {
        return v;
    }
    byte_swap_u32(v)
}

/// Converts little endian u32 to big endian if a target is a little endian machine.
pub fn le_to_be_u32(v: u32) -> u32 {
    if TARGET_BIG_ENDIAN {
        return v;
    }
    byte_swap_u32(v)
}
