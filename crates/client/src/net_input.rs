use serde::{Deserialize, Serialize};
use sim::Input;

/// Wire-format input. GGRS requires `Copy + Clone + PartialEq + Default +
/// Serialize + DeserializeOwned`. Our `sim::Input` is a `bitflags!` newtype
/// over `u8` and doesn't derive serde, so we wrap it for the network boundary
/// and keep the `sim` crate dependency-free.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(transparent)]
pub struct NetInput(pub u8);

impl From<Input> for NetInput {
    fn from(i: Input) -> Self {
        Self(i.bits())
    }
}

impl From<NetInput> for Input {
    fn from(n: NetInput) -> Self {
        Input::from_bits_truncate(n.0)
    }
}
