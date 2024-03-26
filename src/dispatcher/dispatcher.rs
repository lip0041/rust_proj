use super::receiver::{self, Receiver};
use crate::utils::{
    buffer::{MediaData, MediaType},
    Identity,
};
use std::{
    collections::{HashMap, LinkedList, VecDeque},
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering},
        Arc, Condvar, Mutex, RwLock, Weak,
    },
    thread::{self, JoinHandle, Thread},
};
const INVALID_INDEX: u32 = std::u32::MAX;
struct DataNotifier {
    pub audio_index: u32,
    pub video_index: u32,

    block: bool,
    read_index: u32,
    receiver: Option<Weak<Receiver>>,
    dispatcher: Option<Weak<Dispatcher>>,
}

impl DataNotifier {
    fn new() -> Arc<Self> {
        Arc::new(DataNotifier {
            audio_index: INVALID_INDEX,
            video_index: INVALID_INDEX,

            block: false,
            read_index: INVALID_INDEX,
            receiver: None,
            dispatcher: None,
        })
    }

    pub fn data_avaliable(&self) -> bool {
        true
    }

    pub fn set_listener(self: &Arc<Self>, new_dispatcher: &Arc<Dispatcher>) {
        let mut dispatcher = &self.dispatcher;
        dispatcher = &Some(Arc::downgrade(new_dispatcher));
    }

    pub fn set_receiver(self: &Arc<Self>, new_receiver: &Arc<Receiver>) {
        let mut receiver = &self.receiver;
        receiver = &Some(Arc::downgrade(new_receiver));
    }
}

#[derive(Default)]
struct DataSample {
    reserve_falg: AtomicU16,
    media_data: Arc<Mutex<MediaData>>,
    seq: u64,
}

impl DataSample {
    fn new(data: Arc<Mutex<MediaData>>) -> Self {
        DataSample {
            reserve_falg: AtomicU16::new(0),
            media_data: data,
            seq: 0,
        }
    }
}

#[derive(Default)]
struct DispatcherInner {
    running: bool,
    notify_mutex: Arc<Mutex<()>>,
    buffer_mutex: Arc<RwLock<()>>,
    notifiers: HashMap<u32, Arc<DataNotifier>>,
    data_condvar: Arc<Condvar>,

    continue_notify: AtomicBool,
    recv_ref: AtomicU32,
    data_ref: AtomicU32,
    circular_buffer: VecDeque<DataSample>,
}

#[derive(Default)]
pub struct Dispatcher {
    id: u32,
    inner: Arc<Mutex<DispatcherInner>>,
    writing: bool,
    audio_activate: bool,
    video_activate: bool,
    evaluating: bool,
    key_only: bool,
    waiting_key_frame: bool,
    read_flag: u16,
    base_count: u32,
    video_frames: u32,
    audio_frames: u32,
    max_capacity: u32,
    base_capacity: u32,
    double_capacity: u32,
    capacity_increment: u32,

    notify_thread: Option<JoinHandle<()>>,
    key_index: LinkedList<u32>,

    gop: AtomicU32,
    data_mode: MediaType,

    last_audio_index: u32,
    last_video_index: u32,
}

impl Identity for Dispatcher {
    fn get_id(&self) -> u32 {
        self.id
    }
}

impl Dispatcher {
    pub fn new(max_capacity: u32, capacity_increment: u32) -> Arc<Mutex<Self>> {
        Arc::new(
            Dispatcher {
                id: 1,
                inner: Arc::new(Mutex::new(DispatcherInner::default())),
                writing: false,
                video_activate: false,
                audio_activate: false,
                evaluating: false,
                key_only: false,
                waiting_key_frame: true,
                read_flag: 0,
                base_count: 0,
                video_frames: 0,
                audio_frames: 0,
                max_capacity,
                base_capacity: 50,
                double_capacity: 100,
                capacity_increment,
                notify_thread: None,
                key_index: LinkedList::new(),
                gop: AtomicU32::new(0),
                data_mode: MediaType::AV,
                last_audio_index: INVALID_INDEX,
                last_video_index: INVALID_INDEX,
            }
            .into(),
        )
    }

    pub fn start_dispatch(&mut self) {
        let inner = self.inner.clone();
        inner.lock().unwrap().running = true;
        let mtx = inner.lock().unwrap().notify_mutex.clone();
        let condvar = inner.lock().unwrap().data_condvar.clone();

        self.notify_thread = Some(thread::spawn(move || {
            while inner.lock().unwrap().running {
                println!("dispatch thread");
                let _ = condvar.wait(mtx.lock().unwrap());
            }

            println!("exit here");
        }));
        println!("dispatch started");
    }

    pub fn stop_dispatch(&mut self) {
        let mut inner = self.inner.clone();
        if inner.lock().unwrap().running == true {
            inner.lock().unwrap().running = false;
            inner.lock().unwrap().continue_notify = AtomicBool::new(true);

            inner.lock().unwrap().data_condvar.notify_all();
            self.notify_thread
                .take()
                .unwrap()
                .join()
                .expect("can not join notify thread");
        }
    }

    pub fn attach_receiver(&mut self, receiver: &Arc<Receiver>) {
        receiver.notify_read_start();

        let notifier = DataNotifier::new();
        notifier.set_receiver(receiver);

        self.inner
            .lock()
            .unwrap()
            .notifiers
            .insert(receiver.get_id(), notifier);
    }

    pub fn input_data(&mut self, data: Arc<Mutex<MediaData>>) {
        if !self.writing {
            self.writing = true;
        }
        let inner = self.inner.clone();
        let pts = data.lock().unwrap().pts;
        let buff = data.lock().unwrap().buff.clone();
        println!("input data, pts: {}, data: {:?}", pts, buff.lock().unwrap());
        inner.lock().unwrap().notify_mutex.lock().unwrap();
        let data_sample = DataSample::new(data);
        inner.lock().unwrap().circular_buffer.push_back(data_sample);

        inner
            .lock()
            .unwrap()
            .continue_notify
            .store(true, Ordering::Relaxed);

        println!("notify data");

        inner.lock().unwrap().data_condvar.notify_all();
    }
}