use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

impl PlayerId {
    /// Author of system commands (joins, leaves, controller changes). Only the
    /// host/server may issue these; the network layer rejects them from clients.
    pub const SYSTEM: PlayerId = PlayerId(u8::MAX);

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ParcelId(pub u8);

impl ParcelId {
    pub const NONE: ParcelId = ParcelId(u8::MAX);

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }
}
