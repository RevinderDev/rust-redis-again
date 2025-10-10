use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct DbValue {
    pub value: Vec<u8>,
    pub expires_at: Option<Instant>,
}

pub type Database = Arc<Mutex<HashMap<String, DbValue>>>;
