use serde::{Deserialize, Serialize};

pub fn default_section_order() -> Vec<String> {
    vec![
        "me".to_string(),
        "top-ranks".to_string(),
        "historical".to_string(),
        "beatmaps".to_string(),
        "medals".to_string(),
        "recent-activity".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileDetails {
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub devices: Vec<String>,
    #[serde(default = "default_section_order")]
    pub section_order: Vec<String>,
    #[serde(default)]
    pub country: String,
}

impl Default for ProfileDetails {
    fn default() -> Self {
        Self {
            about: String::new(),
            location: String::new(),
            devices: Vec::new(),
            section_order: default_section_order(),
            country: String::new(),
        }
    }
}
