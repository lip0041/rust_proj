#![allow(dead_code, unused)]
pub mod dispatcher;
pub mod utils;
use std::{
    borrow::BorrowMut,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    dispatcher::{dispatcher::Dispatcher, receiver::Receiver},
    utils::buffer::{MediaData, MediaType},
};
struct BufferController {
    dispatcher: Arc<Mutex<Dispatcher>>,
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

        self.dispatcher.lock().unwrap().start_dispatch();
    }

    fn start_write(&mut self) {
        println!("start write");
        let running = self.running.clone();
        let dummy = self.dummy_data.clone();
        let dispatcher = self.dispatcher.clone();
        *running.lock().unwrap() = true;
        self.write_thread = Some(thread::spawn(move || {
            while *running.lock().unwrap() {
                for data in &dummy {
                    dispatcher.lock().unwrap().input_data(data.clone());
                    thread::sleep(Duration::from_millis(25));
                }
                *running.lock().unwrap() = false;
            }
        }));
    }

    fn stop_write(&mut self) {
        println!("stop write begin");
        self.dispatcher.lock().unwrap().stop_dispatch();
        *self.running.lock().unwrap() = false;
        self.write_thread
            .take()
            .unwrap()
            .join()
            .expect("can not join the write thread");
        println!("stop write end");
    }
}

fn main() {
    let mut controller = BufferController::new(1000);
    println!("start");
    controller.generate_av_data();
    controller.start_write();

    while *controller.running.lock().unwrap() == true {
        println!("wait 1s");
        thread::sleep(Duration::from_secs(1));
    }

    controller.stop_write();
}
