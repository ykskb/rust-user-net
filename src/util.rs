type Link<T> = Option<Box<Node<T>>>;
struct Node<T> {
    elem: T,
    next: Link<T>,
}
pub struct List<T> {
    head: Link<T>,
}

pub struct Iter<'a, T> {
    next: Option<&'a Node<T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next.map(|node| {
            self.next = node.next.as_deref();
            &node.elem
        })
    }
}

pub struct IterMut<'a, T> {
    next: Option<&'a mut Node<T>>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next.take().map(|node| {
            self.next = node.next.as_deref_mut();
            &mut node.elem
        })
    }
}

impl<T> List<T> {
    pub fn new() -> Self {
        List { head: None }
    }

    pub fn push(&mut self, elem: T) {
        let new_node = Box::new(Node { elem, next: None });
        let mut head = self.head.as_mut();
        if head.is_none() {
            self.head = Some(new_node);
            return;
        }
        while head.is_some() {
            let node = head.unwrap();
            if node.next.is_none() {
                node.next = Some(new_node);
                break;
            }
            head = node.next.as_mut();
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            next: self.head.as_deref(),
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            next: self.head.as_deref_mut(),
        }
    }
}

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

/// Converts a struct to u8 slice.
pub unsafe fn to_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

/// Converts u8 slice to a struct.
pub unsafe fn bytes_to_struct<T: Sized>(b: &[u8]) -> T {
    let s: T = std::ptr::read(b.as_ptr() as *const _);
    s
}

pub fn cksum16<T: Sized>(hdr: &T, hlen: usize, init: u32) -> u16 {
    let data = unsafe { to_u8_slice(hdr) };
    let mut i = 0;
    let mut hlen = hlen;
    let mut sum = init;

    // Add by 16 bit blocks
    while hlen > 1 {
        sum += ((data[i] as u16) << 8 | (data[i + 1] as u16)) as u32;
        hlen -= 2;
        i += 2;
    }
    if hlen > 0 {
        sum += data[i] as u32
    }
    // Add overflowed value
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16) // return NOT value
}

#[cfg(test)]
mod test {
    use super::List;

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
