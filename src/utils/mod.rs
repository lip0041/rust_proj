pub mod buffer;
pub mod macros;
pub mod timeout_timer;

pub trait Identity {
    fn get_id(&self) -> u32;
}
