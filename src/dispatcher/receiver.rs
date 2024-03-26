use crate::utils::{buffer::MediaType, Identity};
use std::{
    borrow::BorrowMut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
};

use super::dispatcher::{self, Dispatcher};
pub struct Receiver {
    id: u32,
    data_ready: AtomicBool,
    block_audio: AtomicBool,
    block_video: AtomicBool,

    first_audio: AtomicBool,
    first_video: AtomicBool,
    first_mix: AtomicBool,

    mutex: Mutex<()>,
    notify_audio: Condvar,
    notify_video: Condvar,
    notify_data: Condvar,

    mix_read: AtomicBool,
    key_only: AtomicBool,
    dispatcher: Option<Weak<Dispatcher>>,
}

impl Identity for Receiver {
    fn get_id(&self) -> u32 {
        self.id
    }
}

impl Receiver {
    pub fn new() -> Arc<Self> {
        Arc::new(Receiver {
            id: 0,
            data_ready: AtomicBool::new(false),
            block_audio: AtomicBool::new(true),
            block_video: AtomicBool::new(true),
            first_audio: AtomicBool::new(true),
            first_video: AtomicBool::new(true),
            first_mix: AtomicBool::new(true),
            mutex: Mutex::new(()),
            notify_audio: Condvar::new(),
            notify_video: Condvar::new(),
            notify_data: Condvar::new(),
            mix_read: AtomicBool::new(false),
            key_only: AtomicBool::new(false),
            dispatcher: None,
        })
    }

    pub fn set_dispatcher(self: &Arc<Self>, new_dispatcher: &Arc<Dispatcher>) {
        let mut dispatcher = &self.dispatcher;
        dispatcher = &Some(Arc::downgrade(new_dispatcher));
    }

    pub fn request_read(self: Arc<Self>, media_type: MediaType) {}

    pub fn notify_read_start(&self) {
        self.first_audio.store(true, Ordering::Relaxed);
        self.first_video.store(true, Ordering::Relaxed);
        self.first_mix.store(true, Ordering::Relaxed);
    }

    pub fn notify_read_stop(&self) {
        self.block_audio.store(true, Ordering::Relaxed);
        self.block_video.store(true, Ordering::Relaxed);
        self.notify_audio.notify_all();
        self.notify_video.notify_all();
        self.notify_data.notify_all();
    }

    pub fn on_media_data(&self) {
        let lk = self.mutex.lock().unwrap();
        if self.data_ready.load(Ordering::Relaxed) == true {
            return;
        }

        self.data_ready.store(true, Ordering::Relaxed);
        self.notify_data.notify_one();
        *lk;
    }

    pub fn on_audio_data(&self) {
        let lk = self.mutex.lock().unwrap();
        if self.block_audio.load(Ordering::Relaxed) == true {
            return;
        }

        self.block_audio.store(true, Ordering::Relaxed);
        self.notify_audio.notify_one();
        *lk;
    }

    pub fn on_video_data(&self) {
        let lk = self.mutex.lock().unwrap();
        if self.block_video.load(Ordering::Relaxed) == true {
            return;
        }

        self.block_video.store(true, Ordering::Relaxed);
        self.notify_video.notify_one();
        *lk;
    }
}
