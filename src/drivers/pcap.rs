use crate::devices::{ethernet::ETH_FRAME_MAX, NetDevice};

pub fn read_data(device: &NetDevice) -> (usize, [u8; ETH_FRAME_MAX]) {
    (0, [0; ETH_FRAME_MAX])
}
