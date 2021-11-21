// #![feature(try_from)]
// #![feature(core_intrinsics)]
// #![feature(extern_prelude)]
// #![feature(specialization)]

#[macro_use]
extern crate quick_error;

pub use bytes;
pub mod errors;

pub mod tars_type;

pub mod tars_trait;

pub mod tars_decoder;
pub mod tars_encoder;

pub mod tup_uni_attribute;

pub mod prelude {
    pub use crate::errors::*;
    pub use crate::tars_decoder::*;
    pub use crate::tars_encoder::*;
    pub use crate::tars_trait::*;
    pub use crate::tars_type::*;
    pub use crate::tup_uni_attribute::*;
}
