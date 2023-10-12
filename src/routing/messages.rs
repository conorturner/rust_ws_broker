use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub sid: Option<Uuid>, // session_id
    t: String,
    d: String,
}