#![allow(dead_code, unused)]
pub mod dispatcher;
pub mod utils;
use std::{
    borrow::BorrowMut,
    fmt::{Debug, Error},
    panic::Location,
    process,
    sync::{Arc, Mutex, RwLock},
    thread::{self, JoinHandle, ThreadId},
    time::Duration,
};

// use dispatcher::DispatcherReceiver;

use stdext::function_name;

use crate::{
    dispatcher::{
        dispatcher::Dispatcher,
        receiver::{self, Receiver},
    },
    utils::buffer::{MediaData, MediaType},
    utils::macros,
};
struct BufferController {
    pub dispatcher: Arc<Mutex<Dispatcher>>,
    dummy_data: Vec<Arc<Mutex<MediaData>>>,
    dummy_count: u32,
    gop_size: u32,
    write_thread: Option<JoinHandle<()>>,
    running: Arc<Mutex<bool>>,
    video_count: u32,
}

impl BufferController {
    fn new(dummy_count: u32) -> Self {
        BufferController {
            dispatcher: Dispatcher::new(400, 50),
            dummy_data: Vec::new(),
            dummy_count,
            gop_size: 30,
            write_thread: None,
            running: Arc::new(Mutex::new(false)),
            video_count: 0,
        }
    }

    fn generate_av_data(&mut self) {
        for i in 0..self.dummy_count {
            let mut media_data = MediaData::default();
            media_data.buff.lock().unwrap().set_capacity(16);
            media_data
                .buff
                .lock()
                .unwrap()
                .borrow_mut()
                .replace(i.to_be_bytes().to_vec());

            media_data.pts = i as u64;
            if i % 3 == 0 {
                media_data.media_type = MediaType::VIDEO;
                self.video_count += 1;
                if self.video_count >= self.gop_size && self.video_count % self.gop_size == 0 {
                    media_data.key_frame = true
                } else {
                    media_data.key_frame = false
                }
            } else {
                media_data.media_type = MediaType::AUDIO;
            }

            self.dummy_data.push(Arc::new(Mutex::new(media_data)));
        }
        self.dummy_data[0].lock().unwrap().key_frame = true;
        self.dispatcher.lock().unwrap().start_dispatch();
    }

    fn start_write(&mut self) {
        info!("start write");
        let running = self.running.clone();
        let dummy = self.dummy_data.clone();
        let dispatcher = self.dispatcher.clone();
        *running.lock().unwrap() = true;
        self.write_thread = Some(thread::spawn(move || {
            while *running.lock().unwrap() {
                for data in dummy.iter() {
                    dispatcher.lock().unwrap().input_data(data.clone());
                    thread::sleep(Duration::from_millis(25));
                }
                *running.lock().unwrap() = false;
            }
        }));
    }

    fn stop_write(&mut self) {
        debug!("stop write begin");
        self.dispatcher.lock().unwrap().stop_dispatch();
        *self.running.lock().unwrap() = false;
        self.write_thread
            .take()
            .unwrap()
            .join()
            .expect("can not join the write thread");
        debug!("stop write end");
    }
}

struct BufferReceiver {
    receiver: Arc<Receiver>,
    read_thread: Option<JoinHandle<()>>,
    reading: Arc<Mutex<bool>>,
    av_count: Arc<Mutex<u32>>,
    audio_count: Arc<Mutex<u32>>,
    video_count: Arc<Mutex<u32>>,
    read_count: Arc<Mutex<u32>>,
}

impl BufferReceiver {
    fn new(dispatcher: Arc<Mutex<Dispatcher>>) -> BufferReceiver {
        let receiver = BufferReceiver {
            receiver: Arc::new(Receiver::new()),
            read_thread: None,
            reading: Arc::new(Mutex::new(false)),
            av_count: Arc::new(Mutex::new(0)),
            audio_count: Arc::new(Mutex::new(0)),
            video_count: Arc::new(Mutex::new(0)),
            read_count: Arc::new(Mutex::new(0)),
        };

        dispatcher
            .lock()
            .unwrap()
            .attach_receiver(receiver.receiver.clone());

        receiver.receiver.set_dispatcher(dispatcher.clone());
        receiver.receiver.set_key_mode(true);
        receiver
    }

    fn start_read(&mut self, media_type: MediaType) {
        debug!("start read");
        let reading = self.reading.clone();
        *reading.lock().unwrap() = true;
        let receiver = self.receiver.clone();
        let av_count = self.av_count.clone();
        let audio_count = self.audio_count.clone();
        let video_count = self.video_count.clone();
        let read_count = self.read_count.clone();

        self.read_thread = Some(thread::spawn(move || {
            while *reading.lock().unwrap() {
                info!("request read in");
                let (ret, out_data) = receiver.request_read(media_type);
                info!("request read out");
                *read_count.lock().unwrap() += 1;
                if !ret {
                    warn!("request read error");
                    continue;
                }

                {
                    let binding = out_data.unwrap();
                    let data = binding.lock().unwrap();
                    match data.media_type {
                        MediaType::AUDIO => *audio_count.lock().unwrap() += 1,
                        MediaType::VIDEO => *video_count.lock().unwrap() += 1,
                        MediaType::AV => todo!(),
                    }
                    if (media_type == MediaType::AV) {
                        *av_count.lock().unwrap() += 1;
                    }
                    fatal!(
                        "read_type: {:?}, out_type: {:?}, pts: {:?}",
                        media_type,
                        data.media_type,
                        data.pts
                    );
                }
            }

            match media_type {
                MediaType::AUDIO => fatal!(
                    "read_type: {:?}, read done, read count: {}, total count: {}",
                    media_type,
                    *audio_count.lock().unwrap(),
                    *read_count.lock().unwrap()
                ),
                MediaType::VIDEO => fatal!(
                    "read_type: {:?}, read done, read count: {}, total count: {}",
                    media_type,
                    *video_count.lock().unwrap(),
                    *read_count.lock().unwrap()
                ),
                MediaType::AV => fatal!(
                    "read_type: {:?}, read done, read count: {}, total count: {}",
                    media_type,
                    *av_count.lock().unwrap(),
                    *read_count.lock().unwrap()
                ),
            }
        }));
    }

    fn stop_read(&mut self) {
        debug!("stop read begin");
        *self.reading.lock().unwrap() = false;
        self.receiver.notify_read_stop();
        info!("trace");
        self.read_thread
            .take()
            .unwrap()
            .join()
            .expect("can not join the write thread");

        debug!("stop read end");
    }
}

fn main() {
    let mut controller = BufferController::new(100);
    controller.generate_av_data();
    let mut receiver = BufferReceiver::new(controller.dispatcher.clone());
    // thread::sleep(Duration::from_millis(500));
    controller.start_write();

    receiver.start_read(MediaType::VIDEO);
    while *controller.running.lock().unwrap() == true {
        warn!("wait 1s");
        thread::sleep(Duration::from_secs(1));
    }

    controller.stop_write();
    receiver.stop_read();
}
