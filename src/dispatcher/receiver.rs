use crate::utils::{
    buffer::{MediaData, MediaType},
    Identity,
};
use crate::{debug, error, fatal, info, warn};
use std::{
    borrow::BorrowMut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, RwLock, Weak,
    },
    thread, time,
};

use super::dispatcher::{self, Dispatcher};
// use super::DispatcherReceiver;

pub struct Receiver {
    id: u32,
    read_index: Mutex<u32>,
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
    dispatcher: Mutex<Weak<Mutex<Dispatcher>>>,
}

impl Identity for Receiver {
    fn get_id(&self) -> u32 {
        self.id
    }
}

impl Receiver {
    pub fn new() -> Self {
        Receiver {
            id: 0,
            read_index: Mutex::new(0),
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
            dispatcher: Mutex::new(Weak::new()),
        }
    }

    pub fn is_mix_read(&self) -> bool {
        self.mix_read.load(Ordering::Relaxed)
    }

    pub fn set_read_index(&self, index: u32) {
        *self.read_index.lock().unwrap() = index;
    }

    pub fn get_read_index(&self) -> u32 {
        *self.read_index.lock().unwrap()
    }

    pub fn set_dispatcher(&self, new_dispatcher: Arc<Mutex<Dispatcher>>) {
        debug!("set dispatcher");
        let mut dispatcher = self.dispatcher.lock().unwrap();

        let binding = Arc::downgrade(&new_dispatcher);
        *dispatcher = binding;
    }

    pub fn request_read(&self, media_type: MediaType) -> (bool, Option<Arc<Mutex<MediaData>>>) {
        debug!("request_read");
        let dispatcher = self.dispatcher.lock().unwrap().upgrade();
        if dispatcher.is_none() {
            return (false, None);
        }
        debug!("request_read");
        let dispatcher = dispatcher.unwrap();

        debug!("request_read");
        if self.first_mix.load(Ordering::Relaxed) == true && media_type == MediaType::AV {
            dispatcher
                .lock()
                .unwrap()
                .notify_read_ready(self.get_id(), media_type);
            self.mix_read.store(true, Ordering::Relaxed);
            self.first_mix.store(false, Ordering::Relaxed);
        } else if self.first_audio.load(Ordering::Relaxed) && media_type == MediaType::AUDIO {
            dispatcher
                .lock()
                .unwrap()
                .notify_read_ready(self.get_id(), media_type);
            self.first_audio.store(false, Ordering::Relaxed);
        } else if self.first_video.load(Ordering::Relaxed) && media_type == MediaType::VIDEO {
            dispatcher
                .lock()
                .unwrap()
                .notify_read_ready(self.get_id(), media_type);
            self.first_video.store(false, Ordering::Relaxed);
        }

        debug!("request_read");
        {
            let lock = self.mutex.lock().unwrap();
            debug!("request_read");
            match media_type {
                MediaType::AUDIO => self.notify_audio.wait_while(lock, |_| {
                    return self.block_audio.load(Ordering::Relaxed)
                        || media_type != MediaType::AUDIO;
                }),
                MediaType::VIDEO => self.notify_video.wait(lock),
                MediaType::AV => self.notify_data.wait_while(lock, |_| {
                    return self.data_ready.load(Ordering::Relaxed) || media_type != MediaType::AV;
                }),
            };
        }
        debug!("request_read");

        dispatcher
            .lock()
            .unwrap()
            .clear_data_bit(self.get_read_index(), media_type);
        dispatcher
            .lock()
            .unwrap()
            .clear_read_bit(self.get_read_index(), media_type);

        debug!("request_read");
        let (ret, x) = dispatcher
            .lock()
            .unwrap()
            .read_buffer_data(self.get_id(), media_type);

        warn!("read buffer data out, ret: {:?}", ret);
        {
            let _lock = self.mutex.lock().unwrap();
            match media_type {
                MediaType::AUDIO => self.block_audio.store(false, Ordering::Release),
                MediaType::VIDEO => self.block_video.store(false, Ordering::Release),
                MediaType::AV => self.data_ready.store(false, Ordering::Release),
            };
        }

        debug!("request_read");
        dispatcher
            .lock()
            .unwrap()
            .notify_read_ready(self.get_id(), media_type);

        debug!("request_read");
        (ret, x.clone())
    }

    pub fn notify_read_start(&self) {
        self.first_audio.store(true, Ordering::Relaxed);
        self.first_video.store(true, Ordering::Relaxed);
        self.first_mix.store(true, Ordering::Relaxed);
    }

    pub fn notify_read_stop(&self) {
        self.block_audio.store(true, Ordering::Relaxed);
        self.block_video.store(true, Ordering::Relaxed);
        self.data_ready.store(true, Ordering::Relaxed);
        self.notify_audio.notify_all();
        self.notify_video.notify_all();
        self.notify_data.notify_all();
    }

    pub fn on_media_data(&self) {
        let _lk = self.mutex.lock().unwrap();
        if self.data_ready.load(Ordering::Relaxed) == true {
            warn!("block in media");
            return;
        }

        self.data_ready.store(true, Ordering::Relaxed);
        self.notify_data.notify_one();
    }

    pub fn on_audio_data(&self) {
        let _lk = self.mutex.lock().unwrap();
        if self.block_audio.load(Ordering::Relaxed) == true {
            warn!("block in audio");
            return;
        }

        self.block_audio.store(true, Ordering::Relaxed);
        self.notify_audio.notify_one();
    }

    pub fn on_video_data(&self) {
        debug!("trace");
        let _lk = self.mutex.lock().unwrap();
        if self.block_video.load(Ordering::Relaxed) == true {
            warn!("block in video");
            return;
        }

        debug!("trace");
        self.block_video.store(true, Ordering::Relaxed);
        self.notify_video.notify_one();
    }
}
