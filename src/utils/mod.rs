pub mod byte;
pub mod list;

/// Converts a struct to u8 slice.
pub unsafe fn to_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

/// Converts u8 slice to a struct.
pub unsafe fn bytes_to_struct<T: Sized>(b: &[u8]) -> T {
    let s: T = std::ptr::read(b.as_ptr() as *const _);
    s
}

pub fn cksum16(data: &[u8], len: usize, init: u32) -> u16 {
    let mut i = 0;
    let mut len = len;
    let mut sum = init;

    // Add by 16 bit blocks
    while len > 1 {
        sum += ((data[i] as u16) << 8 | (data[i + 1] as u16)) as u32;
        len -= 2;
        i += 2;
    }
    if len > 0 {
        sum += ((data[i] as u16) << 8) as u32
    }
    // Add overflowed value
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16) // return NOT value
}

#[cfg(test)]
mod test {
    use super::list::List;

    #[test]
    fn test_list() {
        let mut list = List::new();
        list.push(1);
        list.push(2);
        list.push(3);
        let mut iteration = list.iter();
        assert_eq!(iteration.next(), Some(&1));
        assert_eq!(iteration.next(), Some(&2));
        assert_eq!(iteration.next(), Some(&3));
    }
}
