pub use crate::chains::Chain as Capability;
pub fn registry() -> Vec<Capability> {
    crate::chains::registry()
}
