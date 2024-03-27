use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use crate::utils::{
    buffer::{MediaData, MediaType},
    macros, Identity,
};

use self::dispatcher::Dispatcher;

pub mod dispatcher;
pub mod receiver;
