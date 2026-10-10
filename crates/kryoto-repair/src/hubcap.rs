use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dlc {
    pub appid: String,
    pub name: String,
}
