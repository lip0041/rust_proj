use std::sync::{Arc, Mutex};

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    #[default]
    AV,
    AUDIO,
    VIDEO,
}

pub trait Identity {
    fn get_id(&self) -> u32;
}

#[derive(Default, Debug)]
pub struct MediaData {
    pub key_frame: bool,
    pub pts: u64,
    pub media_type: MediaType,
    pub buff: Arc<Mutex<Buffer>>,
}

#[derive(Debug, Default)]
pub struct Buffer {
    size: usize,
    capacity: usize,
    data: Arc<Mutex<Vec<u8>>>,
}

impl Buffer {
    pub fn new(size: usize, capacity: usize) -> Buffer {
        Buffer {
            size,
            capacity,
            data: Arc::new(Mutex::new(Vec::with_capacity(capacity))),
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn data(&self) -> Arc<Mutex<Vec<u8>>> {
        self.data.clone()
    }

    pub fn replace(&mut self, new_data: Vec<u8>) {
        let mut data = self.data.lock().unwrap();
        data.resize(new_data.len(), 0);
        data.copy_from_slice(&new_data);
    }

    pub fn append(&mut self, append_data: &mut Vec<u8>) {
        let mut data = self.data.lock().unwrap();
        data.append(append_data);
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        let mut data = self.data.lock().unwrap();
        data.reserve(capacity);
        self.capacity = capacity;
    }
}
