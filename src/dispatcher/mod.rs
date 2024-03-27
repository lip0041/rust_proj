use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use crate::utils::{
    buffer::{MediaData, MediaType},
    Identity,
};

use self::dispatcher::Dispatcher;

pub mod dispatcher;
pub mod receiver;

// pub trait DispatcherReceiver: Identity + Send + Sync {
//     fn set_dispatcher(&self, new_dispatcher: Arc<Mutex<Dispatcher>>);
//     fn request_read(&self, media_type: MediaType) -> Arc<Mutex<MediaData>>;
//     fn notify_read_start(&self);
//     fn notify_read_stop(&self);
//     fn on_media_data(&self);
//     fn on_audio_data(&self);
//     fn on_video_data(&self);
// }
