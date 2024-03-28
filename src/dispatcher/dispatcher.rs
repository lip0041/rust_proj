use super::{
    receiver::{self, Receiver},
    // DispatcherReceiver,
};
use crate::utils::{
    buffer::{MediaData, MediaType},
    Identity,
};
use crate::{debug, error, fatal, info, warn};
use std::{
    collections::{HashMap, LinkedList, VecDeque},
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering},
        Arc, Condvar, Mutex, MutexGuard, RwLock, Weak,
    },
    thread::{self, JoinHandle, Thread},
};
const INVALID_INDEX: u32 = std::u32::MAX;

#[derive(Debug)]
struct DataNotifier {
    pub audio_index: u32,
    pub video_index: u32,

    block: bool,
    read_index: u32,
    receiver: Mutex<Weak<Receiver>>,
    dispatcher: Mutex<Weak<Mutex<Dispatcher>>>,
}

impl DataNotifier {
    fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(DataNotifier {
            audio_index: INVALID_INDEX,
            video_index: INVALID_INDEX,

            block: false,
            read_index: INVALID_INDEX,
            receiver: Mutex::new(Weak::new()),
            dispatcher: Mutex::new(Weak::new()),
        }))
    }

    pub fn data_avaliable(&mut self, media_type: MediaType) -> bool {
        let dispatcher = self.dispatcher.lock().unwrap().upgrade().unwrap().clone();
        let receiver_id = self
            .receiver
            .lock()
            .unwrap()
            .upgrade()
            .unwrap()
            .clone()
            .get_id();
        match media_type {
            MediaType::VIDEO => {
                self.audio_index = dispatcher
                    .lock()
                    .unwrap()
                    .find_next_index(self.audio_index, media_type);

                return !dispatcher
                    .lock()
                    .unwrap()
                    .is_read(receiver_id, self.audio_index);
            }
            MediaType::AUDIO => {
                self.video_index = dispatcher
                    .lock()
                    .unwrap()
                    .find_next_index(self.video_index, media_type);

                return !dispatcher
                    .lock()
                    .unwrap()
                    .is_read(receiver_id, self.video_index);
            }
            MediaType::AV => {
                self.video_index = dispatcher
                    .lock()
                    .unwrap()
                    .find_next_index(self.video_index, media_type);

                return !dispatcher
                    .lock()
                    .unwrap()
                    .is_read(receiver_id, self.video_index);
            }
        }
    }

    pub fn set_receiver(&self, receiver: Arc<Receiver>) {
        *self.receiver.lock().unwrap() = Arc::downgrade(&receiver);
    }

    pub fn notify_data_receiver(&self) {
        debug!("notify data receiver 0");
        let temp = self.receiver.lock().unwrap().upgrade().unwrap();
        debug!("notify data receiver 2");

        temp.on_video_data();
        debug!("notify data receiver done");
    }

    fn set_read_index(&mut self, index: u32) {
        self.read_index = index
    }

    fn get_read_index(&self) -> u32 {
        self.read_index
    }
}

#[derive(Default, Debug)]
struct DataSample {
    reserve_flag: AtomicU16,
    media_data: Arc<Mutex<MediaData>>,
    seq: u64,
}

impl DataSample {
    fn new(data: Arc<Mutex<MediaData>>) -> Self {
        DataSample {
            reserve_flag: AtomicU16::new(0),
            media_data: data,
            seq: 0,
        }
    }
}

#[derive(Default, Debug)]
struct DispatcherInner {
    running: bool,
    notify_mutex: Arc<Mutex<()>>,
    buffer_mutex: Arc<RwLock<()>>,
    notifiers: Arc<RwLock<HashMap<u32, Arc<Mutex<DataNotifier>>>>>,
    data_condvar: Arc<Condvar>,

    continue_notify: Arc<AtomicBool>,
    recv_ref: AtomicU32,
    data_ref: AtomicU32,
    circular_buffer: VecDeque<DataSample>,
}

#[derive(Default, Debug)]
pub struct Dispatcher {
    id: u32,
    inner: Arc<Mutex<DispatcherInner>>,
    writing: bool,
    audio_activate: bool,
    video_activate: bool,
    evaluating: bool,
    key_only: bool,
    waiting_key_frame: bool,
    read_flag: i16,
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
        let notifiers = inner.lock().unwrap().notifiers.clone();
        let continue_notify = inner.lock().unwrap().continue_notify.clone();

        self.notify_thread = Some(thread::spawn(move || {
            while inner.lock().unwrap().running {
                debug!("dispatch 1");
                let notifiers = notifiers.read().unwrap();
                for (receiver_id, notifier) in notifiers.clone() {
                    debug!("dispatch 2");
                    notifier.lock().unwrap().notify_data_receiver();
                    debug!("dispatch 2");
                }
                debug!("dispatch 3");
                let _unused = condvar.wait(mtx.lock().unwrap());
                debug!("dispatch 4");
                continue_notify.store(false, Ordering::Relaxed);
            }
        }));
        debug!("dispatch started");
    }

    pub fn stop_dispatch(&mut self) {
        let mut inner = self.inner.clone();
        if inner.lock().unwrap().running == true {
            inner.lock().unwrap().running = false;
            inner.lock().unwrap().continue_notify = Arc::new(AtomicBool::new(true));

            inner.lock().unwrap().data_condvar.notify_all();
            self.notify_thread
                .take()
                .unwrap()
                .join()
                .expect("can not join notify thread");
        }
    }

    pub fn attach_receiver(&mut self, receiver: Arc<Receiver>) {
        debug!("attach in");
        let c_receiver = receiver.clone();
        c_receiver.notify_read_start();

        if self.read_flag == 0xffffu16 as i16 {
            error!("receiver limited!");
            return;
        }

        let base_notifier = DataNotifier::new();

        let inner = self.inner.clone();
        inner
            .lock()
            .unwrap()
            .notifiers
            .write()
            .unwrap()
            .insert(c_receiver.get_id(), base_notifier.clone());

        let mut notifier = base_notifier.lock().unwrap();
        notifier.set_receiver(receiver.clone());

        let usable_ref = !self.read_flag & (-(!self.read_flag));

        if (usable_ref & usable_ref - 1) != 0 {
            error!("usable_ref: {} invalid", usable_ref);
            return;
        }
        self.read_flag |= usable_ref;

        let mut val = usable_ref;
        let mut read_index = 0;
        while val != 0 {
            val >>= 1;
            read_index += 1;
        }

        notifier.set_read_index(read_index);

        debug!("attach in");
        if inner.lock().unwrap().circular_buffer.is_empty() {
            debug!("attach in");
            notifier.audio_index = INVALID_INDEX;
            notifier.video_index = INVALID_INDEX;
            // Mutex::unlock(notifier);
            self.set_receiver_data_ref(receiver.get_id(), MediaType::AUDIO, false);
            self.set_receiver_data_ref(receiver.get_id(), MediaType::VIDEO, false);
            self.video_activate = true;
            self.audio_activate = true;
            debug!("attach done");
            return;
        }

        if self.data_mode == MediaType::AUDIO {
            notifier.audio_index = (inner.lock().unwrap().circular_buffer.len() - 1) as u32;
            self.set_receiver_data_ref(receiver.get_id(), MediaType::AUDIO, true);
            notifier.video_index = INVALID_INDEX;
            self.set_receiver_data_ref(receiver.get_id(), MediaType::VIDEO, false);
        } else {
            if self.key_index.is_empty() {
                let temp_index =
                    self.find_next_index(*self.key_index.back().unwrap(), MediaType::AUDIO);
                if temp_index == *self.key_index.back().unwrap() {
                    notifier.audio_index = INVALID_INDEX;
                } else {
                    notifier.audio_index = temp_index;
                }

                notifier.video_index = *self.key_index.back().unwrap();
                self.set_receiver_data_ref(
                    receiver.get_id(),
                    MediaType::AUDIO,
                    temp_index != INVALID_INDEX,
                );
                self.set_receiver_data_ref(receiver.get_id(), MediaType::VIDEO, true);
                if self.last_audio_index == INVALID_INDEX {
                    self.audio_activate = true;
                }
            } else {
                notifier.audio_index = self.find_last_index(MediaType::AUDIO);
                notifier.video_index = INVALID_INDEX;
                self.set_receiver_data_ref(receiver.get_id(), MediaType::AUDIO, true);
                self.set_receiver_data_ref(receiver.get_id(), MediaType::VIDEO, false);
                if self.last_audio_index == INVALID_INDEX {
                    self.audio_activate = true;
                }
            }
        }
        debug!("attach done");
    }

    pub fn input_data(&mut self, data: Arc<Mutex<MediaData>>) {
        if !self.writing {
            self.writing = true;
        }
        let inner = self.inner.clone();
        let pts = data.lock().unwrap().pts;
        let buff = data.lock().unwrap().buff.clone();
        info!("input data, pts: {}", pts);
        inner.lock().unwrap().notify_mutex.lock().unwrap();
        let data_sample = DataSample::new(data);
        inner.lock().unwrap().circular_buffer.push_back(data_sample);

        inner
            .lock()
            .unwrap()
            .continue_notify
            .store(true, Ordering::Relaxed);

        debug!("notify data");

        inner.lock().unwrap().data_condvar.notify_all();
    }

    pub fn notify_read_ready(&mut self, receiver_id: u32, media_type: MediaType) {
        let inner = self.inner.clone();

        let notifier = inner
            .lock()
            .unwrap()
            .notifiers
            .read()
            .unwrap()
            .get(&receiver_id)
            .unwrap()
            .clone();

        if media_type == MediaType::AV {
            self.set_receiver_read_ref(receiver_id, MediaType::AUDIO, true);
            self.set_receiver_read_ref(receiver_id, MediaType::VIDEO, true);
        } else {
            self.set_receiver_read_ref(receiver_id, media_type, true);
        }

        let data_avaliable = notifier.lock().unwrap().data_avaliable(media_type);

        if media_type == MediaType::AV {
            self.set_receiver_read_ref(receiver_id, MediaType::AUDIO, data_avaliable);
            self.set_receiver_read_ref(receiver_id, MediaType::VIDEO, data_avaliable);
        } else {
            self.set_receiver_read_ref(receiver_id, media_type, data_avaliable);
        }

        if !data_avaliable {
            return;
        }

        inner
            .lock()
            .unwrap()
            .continue_notify
            .store(true, Ordering::Relaxed);

        inner.lock().unwrap().data_condvar.notify_one();
    }

    pub fn read_buffer_data(&mut self, media_type: MediaType) -> Arc<Mutex<MediaData>> {
        debug!("read buffer data");
        let inner = self.inner.clone();
        let binding = inner.lock().unwrap();
        let data = binding.circular_buffer.back();
        data.unwrap().media_data.clone()
    }

    fn set_receiver_data_ref(&mut self, receiver_id: u32, media_type: MediaType, ready: bool) {
        let inner = self.inner.clone();
        info!("trace");
        let notifiers = inner.lock().unwrap().notifiers.read().unwrap().clone();
        let receiver = notifiers.get(&receiver_id).unwrap();
        info!("trace");

        let index = receiver.lock().unwrap().get_read_index();
        info!("trace");

        if index == INVALID_INDEX {
            return;
        }
        let mut bit_ref = 0;
        if media_type == MediaType::AUDIO {
            let audio_bit = index * 2;
            bit_ref = 0x1 << audio_bit;
        } else if media_type == MediaType::VIDEO {
            let video_bit = index * 2 + 1;
            bit_ref = 0x1 << video_bit;
        }

        if ready {
            inner
                .lock()
                .unwrap()
                .data_ref
                .fetch_or(bit_ref, Ordering::Relaxed);
        } else {
            bit_ref = !bit_ref;
            inner
                .lock()
                .unwrap()
                .data_ref
                .fetch_and(bit_ref, Ordering::Relaxed);
        }
        info!("trace");
    }

    fn set_receiver_read_ref(&mut self, receiver_id: u32, media_type: MediaType, ready: bool) {
        let mut index = INVALID_INDEX;

        let inner = self.inner.clone();
        let notifiers = inner.lock().unwrap().notifiers.read().unwrap().clone();
        if notifiers.contains_key(&receiver_id) {
            index = notifiers
                .get(&receiver_id)
                .unwrap()
                .lock()
                .unwrap()
                .get_read_index();
        }

        if index == INVALID_INDEX {
            return;
        }
        let mut bit_ref = 0;
        if media_type == MediaType::AUDIO {
            let audio_bit = index * 2;
            bit_ref = 0x1 << audio_bit;
        } else if media_type == MediaType::VIDEO {
            let video_bit = index * 2 + 1;
            bit_ref = 0x1 << video_bit;
        }

        if ready {
            inner
                .lock()
                .unwrap()
                .recv_ref
                .fetch_or(bit_ref, Ordering::Relaxed);
        } else {
            bit_ref = !bit_ref;
            inner
                .lock()
                .unwrap()
                .recv_ref
                .fetch_and(bit_ref, Ordering::Relaxed);
        }
    }

    fn find_next_index(&self, index: u32, media_type: MediaType) -> u32 {
        let inner = self.inner.clone();
        let circular_buffer = &inner.lock().unwrap().circular_buffer;
        if (index + 1) as usize >= circular_buffer.len() || index == INVALID_INDEX {
            return index;
        }
        if media_type == MediaType::AV {
            return index + 1;
        }

        for i in (index + 1) as usize..circular_buffer.len() {
            if circular_buffer.get(i).is_some()
                && circular_buffer[i].media_data.lock().unwrap().media_type == media_type
            {
                if self.key_only
                    && media_type == MediaType::VIDEO
                    && !circular_buffer[i].media_data.lock().unwrap().key_frame
                {
                    continue;
                } else {
                    i as u32;
                }
            }
        }

        return index;
    }

    fn find_last_index(&self, media_type: MediaType) -> u32 {
        if self.inner.lock().unwrap().circular_buffer.is_empty() {
            return INVALID_INDEX;
        }
        if media_type == MediaType::AUDIO {
            return self.last_audio_index;
        } else {
            return self.last_video_index;
        }
    }

    fn is_read(&self, receiver_id: u32, index: u32) -> bool {
        if index as usize >= self.inner.lock().unwrap().circular_buffer.len() {
            return true;
        } else {
            self.is_data_read(
                receiver_id,
                &self.inner.lock().unwrap().circular_buffer[index as usize],
            )
        }
    }

    fn is_data_read(&self, receiver_id: u32, data: &DataSample) -> bool {
        let mut index = INVALID_INDEX;
        let inner = self.inner.clone();
        let notifiers = inner.lock().unwrap().notifiers.read().unwrap().clone();
        if notifiers.contains_key(&receiver_id) {
            index = notifiers
                .get(&receiver_id)
                .unwrap()
                .lock()
                .unwrap()
                .get_read_index();
        }

        if index == INVALID_INDEX {
            return false;
        }

        data.reserve_flag.load(Ordering::Relaxed) & 0x1 << index != 0
    }
}
