use crate::utils::{
    buffer::{MediaData, MediaType},
    Identity,
};
use std::{
    borrow::BorrowMut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
    thread, time,
};

use super::dispatcher::{self, Dispatcher};
// use super::DispatcherReceiver;

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

    pub fn set_dispatcher(&self, new_dispatcher: Arc<Mutex<Dispatcher>>) {
        println!("{:?}, set dispatcher", thread::current().id());
        let mut dispatcher = self.dispatcher.lock().unwrap();

        let binding = Arc::downgrade(&new_dispatcher);
        *dispatcher = binding;
    }

    pub fn request_read(&self, media_type: MediaType) -> Arc<Mutex<MediaData>> {
        println!("{:?}, request read", thread::current().id());
        let dispatcher = self.dispatcher.lock().unwrap().upgrade().clone().unwrap();
        // fix here
        self.block_video.store(false, Ordering::Release);

        if self.first_mix.load(Ordering::Relaxed) == true && media_type == MediaType::AV {
            dispatcher.lock().unwrap().notify_read_ready();
            self.mix_read.store(true, Ordering::Relaxed);
            self.first_mix.store(false, Ordering::Relaxed);
        } else if self.first_audio.load(Ordering::Relaxed) && media_type == MediaType::AUDIO {
            dispatcher.lock().unwrap().notify_read_ready();
            self.first_audio.store(false, Ordering::Relaxed);
        } else if self.first_video.load(Ordering::Relaxed) && media_type == MediaType::VIDEO {
            dispatcher.lock().unwrap().notify_read_ready();
            self.first_video.store(false, Ordering::Relaxed);
        }
        println!("{:?}, request read 1", thread::current().id());
        let lock = self.mutex.lock().unwrap();
        println!("{:?}, request read 2", thread::current().id());
        match media_type {
            MediaType::AUDIO => self.notify_audio.wait(lock),
            MediaType::VIDEO => self.notify_video.wait(lock),
            MediaType::AV => self.notify_data.wait(lock),
        };
        println!("{:?}, request read 3", thread::current().id());
        let x = dispatcher.lock().unwrap().read_buffer_data(media_type);
        match media_type {
            MediaType::AUDIO => self.block_audio.store(false, Ordering::Release),
            MediaType::VIDEO => self.block_video.store(false, Ordering::Release),
            MediaType::AV => self.data_ready.store(false, Ordering::Release),
        };

        x.clone()
    }

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
        println!("{:?}, on video", thread::current().id());
        let lk = self.mutex.lock().unwrap();
        println!("{:?}, on video 1", thread::current().id());
        if self.block_video.load(Ordering::Relaxed) == true {
            println!("{:?}, on video 2", thread::current().id());
            return;
        }
        println!("{:?}, on video3", thread::current().id());

        self.block_video.store(true, Ordering::Relaxed);
        self.notify_video.notify_one();
        *lk;
        println!("{:?}, on video 4", thread::current().id());
    }
}
