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
}

impl DataNotifier {
    fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(DataNotifier {
            audio_index: INVALID_INDEX,
            video_index: INVALID_INDEX,

            block: false,
            read_index: INVALID_INDEX,
            receiver: Mutex::new(Weak::new()),
        }))
    }

    pub fn set_receiver(&self, receiver: Arc<Receiver>) {
        *self.receiver.lock().unwrap() = Arc::downgrade(&receiver);
    }

    pub fn is_mixed(&self) -> bool {
        let receiver = self.receiver.lock().unwrap().upgrade().unwrap().clone();
        receiver.is_mix_read()
    }

    pub fn notify_data_receiver(&self, media_type: MediaType) {
        debug!("notify_data_receiver");
        let receiver = self.receiver.lock().unwrap().upgrade();
        if receiver.is_none() || self.block {
            warn!("receiver is none");
            return;
        }
        match media_type {
            MediaType::AUDIO => receiver.unwrap().on_audio_data(),
            MediaType::VIDEO => receiver.unwrap().on_video_data(),
            MediaType::AV => receiver.unwrap().on_media_data(),
        }
    }

    pub fn get_receiver_read_index(&self, media_type: MediaType) -> u32 {
        match media_type {
            MediaType::AUDIO => self.audio_index,
            MediaType::VIDEO => self.video_index,
            MediaType::AV => {
                if self.audio_index != INVALID_INDEX && self.video_index != INVALID_INDEX {
                    if self.audio_index <= self.video_index {
                        self.audio_index
                    } else {
                        self.video_index
                    }
                } else if self.audio_index == INVALID_INDEX && self.video_index == INVALID_INDEX {
                    INVALID_INDEX
                } else {
                    if self.audio_index == INVALID_INDEX {
                        self.video_index
                    } else {
                        self.audio_index
                    }
                }
            }
        }
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
            media_data: data.clone(),
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
                warn!("in notify thread");
                let data_ref = inner.lock().unwrap().data_ref.load(Ordering::Relaxed);
                let recv_ref = inner.lock().unwrap().recv_ref.load(Ordering::Relaxed);
                let notify_ref = data_ref & recv_ref;
                for (recv_id, notifier) in notifiers.read().unwrap().clone() {
                    let read_index = notifier.lock().unwrap().get_read_index();
                    if 0x0001 << (read_index * 2) & notify_ref != 0
                        || 0x0001 << (read_index * 2 + 1) & notify_ref != 0
                    {
                        let mixed = notifier.lock().unwrap().is_mixed();
                        if mixed {
                            notifier.lock().unwrap().notify_data_receiver(MediaType::AV);
                        } else {
                            if 0x0001 << (read_index * 2) & notify_ref != 0 {
                                notifier
                                    .lock()
                                    .unwrap()
                                    .notify_data_receiver(MediaType::AUDIO);
                            } else if 0x0001 << (read_index * 2 + 1) & notify_ref != 0 {
                                notifier
                                    .lock()
                                    .unwrap()
                                    .notify_data_receiver(MediaType::VIDEO);
                            }
                        }
                    }
                }
                warn!("before wait");
                let _unused = condvar.wait(mtx.lock().unwrap());
                warn!("after wait");
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
        let receiver = receiver.clone();
        receiver.notify_read_start();

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
            .insert(receiver.get_id(), base_notifier.clone());

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

        receiver.set_read_index(read_index);
        notifier.set_read_index(read_index);

        if inner.lock().unwrap().circular_buffer.is_empty() {
            notifier.audio_index = INVALID_INDEX;
            notifier.video_index = INVALID_INDEX;
            drop(notifier);
            self.set_receiver_data_ref(receiver.get_read_index(), MediaType::AUDIO, false);
            self.set_receiver_data_ref(receiver.get_read_index(), MediaType::VIDEO, false);
            self.video_activate = true;
            self.audio_activate = true;
            debug!("attach done");
            return;
        }

        if self.data_mode == MediaType::AUDIO {
            notifier.audio_index = (inner.lock().unwrap().circular_buffer.len() - 1) as u32;
            notifier.video_index = INVALID_INDEX;
            drop(notifier);
            self.set_receiver_data_ref(receiver.get_read_index(), MediaType::AUDIO, true);
            self.set_receiver_data_ref(receiver.get_read_index(), MediaType::VIDEO, false);
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
                drop(notifier);
                self.set_receiver_data_ref(
                    receiver.get_read_index(),
                    MediaType::AUDIO,
                    temp_index != INVALID_INDEX,
                );
                self.set_receiver_data_ref(receiver.get_read_index(), MediaType::VIDEO, true);
                if self.last_audio_index == INVALID_INDEX {
                    self.audio_activate = true;
                }
            } else {
                notifier.audio_index = self.find_last_index(MediaType::AUDIO);
                notifier.video_index = INVALID_INDEX;
                drop(notifier);
                self.set_receiver_data_ref(receiver.get_read_index(), MediaType::AUDIO, true);
                self.set_receiver_data_ref(receiver.get_read_index(), MediaType::VIDEO, false);
                if self.last_audio_index == INVALID_INDEX {
                    self.audio_activate = true;
                }
            }
        }
        debug!("attach done");
    }

    pub fn input_data(&mut self, data: Arc<Mutex<MediaData>>) {
        debug!("trace");
        if !self.writing {
            self.writing = true;
        }
        let inner = self.inner.clone();
        let pts = data.lock().unwrap().pts;
        let buff = data.lock().unwrap().buff.clone();
        let key_frame = data.lock().unwrap().key_frame;
        let media_type = data.lock().unwrap().media_type;
        info!(
            "input data, pts: {}, key_frame: {}, media_type: {:?}",
            pts, key_frame, media_type
        );

        let data_sample = DataSample::new(data.clone());

        inner.lock().unwrap().circular_buffer.push_back(data_sample);

        let buffer_len = inner.lock().unwrap().circular_buffer.len() as u32;

        let data = data.lock().unwrap();
        if key_frame {
            fatal!("input key frame, cur_len: {}", buffer_len);
            self.audio_activate = true;
            self.video_activate = true;
        }

        if media_type == MediaType::AUDIO {
            self.last_audio_index = buffer_len - 1;
            self.activate_data_ref(MediaType::AUDIO, false);
            self.audio_frames += 1;
        } else {
            self.last_video_index = buffer_len - 1;
            self.activate_data_ref(MediaType::VIDEO, key_frame);
            self.video_frames += 1;
        }

        if self.audio_activate && media_type == MediaType::AUDIO {
            self.activate_receiver_index(buffer_len - 1, MediaType::AUDIO);
        }

        if self.video_activate && key_frame {
            self.activate_receiver_index(buffer_len - 1, MediaType::VIDEO);
        }
        inner
            .lock()
            .unwrap()
            .continue_notify
            .store(true, Ordering::Relaxed);
        inner.lock().unwrap().data_condvar.notify_all();
        debug!("input done");
    }

    fn activate_data_ref(&mut self, media_type: MediaType, key_frame: bool) {
        let mut bit_ref = 0x0000;
        let inner = self.inner.clone();
        let notifiers = inner.lock().unwrap().notifiers.write().unwrap().clone();
        for (recv_id, notifier) in notifiers {
            let index = notifier.lock().unwrap().get_read_index();
            if media_type == MediaType::AUDIO {
                bit_ref |= 0x1 << (index * 2);
                continue;
            }
            if index != INVALID_INDEX {
                bit_ref |= 0x1 << (index * 2 + 1);
            }
        }

        inner
            .lock()
            .unwrap()
            .data_ref
            .fetch_or(bit_ref, Ordering::Relaxed);
    }

    fn activate_receiver_index(&mut self, index: u32, media_type: MediaType) {
        let inner = self.inner.clone();
        let notifiers = inner.lock().unwrap().notifiers.write().unwrap().clone();
        for (recv_id, notifier) in notifiers {
            let mut notifier = notifier.lock().unwrap();
            let circular_buffer = &inner.lock().unwrap().circular_buffer;
            if media_type == MediaType::VIDEO {
                if notifier.video_index != index
                    && (notifier.video_index == INVALID_INDEX
                        || self.is_read(
                            notifier.get_read_index(),
                            notifier.video_index,
                            circular_buffer,
                        ))
                {
                    notifier.video_index = index;
                    fatal!(
                        "recv_id: {}, activate video: {}",
                        recv_id,
                        notifier.video_index
                    );
                }
            } else {
                if notifier.audio_index != index
                    && (notifier.audio_index == INVALID_INDEX
                        || self.is_read(
                            notifier.get_read_index(),
                            notifier.audio_index,
                            circular_buffer,
                        ))
                {
                    notifier.audio_index = index;
                    fatal!(
                        "recv_id: {}, activate audio: {}",
                        recv_id,
                        notifier.audio_index
                    );
                }
            }
        }

        if media_type == MediaType::VIDEO {
            self.video_activate = false;
        } else {
            self.audio_activate = false;
        }
    }

    pub fn notify_read_ready(&mut self, recv_id: u32, media_type: MediaType) {
        info!("notify read ready");
        let inner = self.inner.clone();

        let notifier = inner
            .lock()
            .unwrap()
            .notifiers
            .read()
            .unwrap()
            .get(&recv_id)
            .unwrap()
            .clone();

        let read_index = notifier.lock().unwrap().get_read_index();
        if media_type == MediaType::AV {
            self.set_receiver_read_ref(read_index, MediaType::AUDIO, true);
            self.set_receiver_read_ref(read_index, MediaType::VIDEO, true);
        } else {
            self.set_receiver_read_ref(read_index, media_type, true);
        }

        let mut data_available = false;
        {
            let circular_buffer = &inner.lock().unwrap().circular_buffer;
            match media_type {
                MediaType::AUDIO => {
                    let audio_index = notifier.lock().unwrap().audio_index;
                    let index = self.find_receiver_next_index(
                        audio_index,
                        media_type,
                        read_index,
                        &circular_buffer,
                    );
                    notifier.lock().unwrap().audio_index = index;
                    data_available = !self.is_read(read_index, index, &circular_buffer);
                }
                MediaType::VIDEO | MediaType::AV => {
                    let video_index = notifier.lock().unwrap().video_index;
                    let index = self.find_receiver_next_index(
                        video_index,
                        media_type,
                        read_index,
                        &circular_buffer,
                    );
                    notifier.lock().unwrap().video_index = index;
                    data_available = !self.is_read(read_index, index, &circular_buffer);
                }
            }

            let video_index = notifier.lock().unwrap().video_index;
            let audio_index = notifier.lock().unwrap().audio_index;
            info!(
                "notify read ready done, type: {:?}, audio: {}, video: {}, data_available: {:?}",
                media_type, audio_index, video_index, data_available
            );
        }

        if media_type == MediaType::AV {
            self.set_receiver_read_ref(read_index, MediaType::AUDIO, data_available);
            self.set_receiver_read_ref(read_index, MediaType::VIDEO, data_available);
        } else {
            self.set_receiver_read_ref(read_index, media_type, data_available);
        }

        if !data_available {
            return;
        }
        inner
            .lock()
            .unwrap()
            .continue_notify
            .store(true, Ordering::Relaxed);
        inner.lock().unwrap().data_condvar.notify_one();
    }

    pub fn read_buffer_data(
        &mut self,
        recv_id: u32,
        media_type: MediaType,
    ) -> (bool, Option<Arc<Mutex<MediaData>>>) {
        debug!("read buffer data in");
        let inner = self.inner.clone();

        if !inner
            .lock()
            .unwrap()
            .notifiers
            .read()
            .unwrap()
            .contains_key(&recv_id)
        {
            error!("read error");
            return (false, None);
        }

        let inner_lock = inner.lock().unwrap();
        let binding = inner_lock.notifiers.read().unwrap();
        let circular_buffer = &inner_lock.circular_buffer;

        let notifier = binding.get(&recv_id).unwrap();

        let mut index = notifier.lock().unwrap().get_receiver_read_index(media_type);
        let read_index = notifier.lock().unwrap().get_read_index();

        if index >= circular_buffer.len() as u32 {
            error!(
                "read error, read_index: {}, buffer len: {}",
                index,
                circular_buffer.len()
            );
            return (false, None);
        }

        // todo: key only

        if self.is_data_read(read_index, circular_buffer.get(index as usize).unwrap()) {
            self.updata_receiver_read_index(notifier.clone(), index, media_type, &circular_buffer);
        }

        index = notifier.lock().unwrap().get_receiver_read_index(media_type);
        if index >= circular_buffer.len() as u32
            || self.is_data_read(read_index, circular_buffer.get(index as usize).unwrap())
        {
            error!("already read");
            return (false, None);
        }

        let data = circular_buffer.get(index as usize).unwrap();

        data.reserve_flag.fetch_or(
            0x1 << notifier.lock().unwrap().get_read_index(),
            Ordering::Relaxed,
        );

        debug!("read buffer data in");
        self.updata_receiver_read_index(notifier.clone(), index, media_type, &circular_buffer);
        debug!("read buffer data in");

        (true, Some(data.media_data.clone()))
    }

    pub fn clear_data_bit(&mut self, read_index: u32, media_type: MediaType) {
        if media_type == MediaType::AV {
            self.set_receiver_data_ref(read_index, media_type, false);
        } else {
            self.set_receiver_data_ref(read_index, MediaType::VIDEO, false);
            self.set_receiver_data_ref(read_index, MediaType::AUDIO, false);
        }
    }

    pub fn clear_read_bit(&mut self, read_index: u32, media_type: MediaType) {
        if media_type == MediaType::AV {
            self.set_receiver_read_ref(read_index, media_type, false);
        } else {
            self.set_receiver_read_ref(read_index, MediaType::VIDEO, false);
            self.set_receiver_read_ref(read_index, MediaType::AUDIO, false);
        }
    }

    fn updata_receiver_read_index(
        &mut self,
        notifier: Arc<Mutex<DataNotifier>>,
        index: u32,
        media_type: MediaType,
        circular_buffer: &VecDeque<DataSample>,
    ) {
        let next_index = self.find_receiver_next_index(
            index,
            media_type,
            notifier.lock().unwrap().get_read_index(),
            &circular_buffer,
        );

        if index == next_index {
            return;
        }
        match media_type {
            MediaType::AUDIO => notifier.lock().unwrap().audio_index = next_index,
            MediaType::VIDEO => notifier.lock().unwrap().video_index = next_index,
            MediaType::AV => {
                notifier.lock().unwrap().video_index = next_index;
                notifier.lock().unwrap().audio_index = next_index
            }
        }
        let audio_index = notifier.lock().unwrap().audio_index;
        let video_index = notifier.lock().unwrap().video_index;
        info!(
            "after update type: {:?}, audio_index: {}, video_index: {}",
            media_type, audio_index, video_index
        );
    }

    fn set_receiver_data_ref(&mut self, read_index: u32, media_type: MediaType, ready: bool) {
        let inner = self.inner.clone();

        let index = read_index;

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
    }

    fn set_receiver_read_ref(&mut self, read_index: u32, media_type: MediaType, ready: bool) {
        let mut index = read_index;

        let inner = self.inner.clone();

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
        if index == INVALID_INDEX || (index + 1) as usize >= circular_buffer.len() {
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

    fn find_receiver_next_index(
        &self,
        index: u32,
        media_type: MediaType,
        read_index: u32,
        circular_buffer: &VecDeque<DataSample>,
    ) -> u32 {
        if index == INVALID_INDEX || index + 1 >= circular_buffer.len() as u32 {
            return index;
        }

        if !self.is_read(read_index, index, &circular_buffer) {
            return index;
        }

        if media_type == MediaType::AV {
            return index + 1;
        }

        for i in index + 1..circular_buffer.len() as u32 {
            if circular_buffer
                .get(i as usize)
                .unwrap()
                .media_data
                .lock()
                .unwrap()
                .media_type
                == media_type
            {
                return i;
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

    fn is_read(&self, read_index: u32, index: u32, circular_buffer: &VecDeque<DataSample>) -> bool {
        if index as usize >= circular_buffer.len() {
            return true;
        } else {
            self.is_data_read(read_index, &circular_buffer[index as usize])
        }
    }

    fn is_data_read(&self, read_index: u32, data: &DataSample) -> bool {
        if read_index == INVALID_INDEX {
            return false;
        }
        (data.reserve_flag.load(Ordering::Relaxed) & (0x0001 << read_index)) != 0
    }
}
