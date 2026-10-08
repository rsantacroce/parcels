//! The tick pipeline. Each stage reads the previous stage's output; the order is
//! fixed in `GameState::step`.

pub mod apply;
pub mod demand;
pub mod economy;
pub mod growth;
pub mod land_value;
pub mod utilities;
