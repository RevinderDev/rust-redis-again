use std::collections::HashMap;
use std::sync::{Arc, Mutex};
pub type Database = Arc<Mutex<HashMap<String, Vec<u8>>>>;
