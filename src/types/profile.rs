use serde::{Deserialize, Serialize};

pub const SUPPORTED_DEVICES: &[&str] = &["Mouse", "Keyboard", "Tablet", "Touchscreen", "Controller"];

pub const DEFAULT_SECTIONS: &[&str] = &[
    "me",
    "top-ranks",
    "historical",
    "beatmaps",
    "medals",
    "recent-activity",
];

pub fn default_section_order() -> Vec<String> {
    DEFAULT_SECTIONS.iter().map(|s| s.to_string()).collect()
}

pub fn default_playmode() -> String {
    "osu".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileDetails {
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub interests: String,
    #[serde(default)]
    pub occupation: String,
    #[serde(default)]
    pub twitter: String,
    #[serde(default)]
    pub discord: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub devices: Vec<String>,
    #[serde(default = "default_section_order")]
    pub section_order: Vec<String>,
    #[serde(default)]
    pub country: String,
    #[serde(default = "default_playmode")]
    pub playmode: String,
    #[serde(default)]
    pub pinned_scores: Vec<String>,
}

impl Default for ProfileDetails {
    fn default() -> Self {
        Self {
            about: String::new(),
            location: String::new(),
            interests: String::new(),
            occupation: String::new(),
            twitter: String::new(),
            discord: String::new(),
            website: String::new(),
            devices: Vec::new(),
            section_order: default_section_order(),
            country: String::new(),
            playmode: default_playmode(),
            pinned_scores: Vec::new(),
        }
    }
}

impl ProfileDetails {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.about.len() > 5000 {
            return Err("About text exceeds maximum length of 5000 characters");
        }
        if self.location.len() > 100 {
            return Err("Location exceeds maximum length of 100 characters");
        }
        if self.interests.len() > 200 {
            return Err("Interests exceeds maximum length of 200 characters");
        }
        if self.occupation.len() > 200 {
            return Err("Occupation exceeds maximum length of 200 characters");
        }
        if self.twitter.len() > 100 {
            return Err("Twitter username exceeds maximum length of 100 characters");
        }
        if self.discord.len() > 100 {
            return Err("Discord username exceeds maximum length of 100 characters");
        }
        if self.website.len() > 200 {
            return Err("Website exceeds maximum length of 200 characters");
        }
        if self.pinned_scores.len() > 20 {
            return Err("Too many pinned scores (maximum 20)");
        }
        for device in &self.devices {
            if !SUPPORTED_DEVICES.iter().any(|d| d.eq_ignore_ascii_case(device)) {
                return Err("Unsupported input device");
            }
        }
        if self.section_order.len() != DEFAULT_SECTIONS.len() {
            return Err("Section order must contain exactly 6 sections");
        }
        for section in DEFAULT_SECTIONS {
            if !self.section_order.iter().any(|s| s == section) {
                return Err("Section order is missing required sections");
            }
        }
        if !self.country.is_empty() && crate::utils::country_code_to_byte(&self.country) == 0 {
            return Err("Invalid country code");
        }
        if !self.playmode.is_empty() && !["osu", "taiko", "fruits", "mania"].contains(&self.playmode.as_str()) {
            return Err("Invalid default game mode");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_details_validation() {
        let valid = ProfileDetails::default();
        assert!(valid.validate().is_ok());

        let mut invalid_about = ProfileDetails::default();
        invalid_about.about = "a".repeat(5001);
        assert!(invalid_about.validate().is_err());

        let mut invalid_location = ProfileDetails::default();
        invalid_location.location = "b".repeat(101);
        assert!(invalid_location.validate().is_err());

        let mut invalid_device = ProfileDetails::default();
        invalid_device.devices.push("SmartFridge".to_string());
        assert!(invalid_device.validate().is_err());

        let mut valid_device = ProfileDetails::default();
        valid_device.devices.push("Tablet".to_string());
        valid_device.devices.push("keyboard".to_string());
        assert!(valid_device.validate().is_ok());

        let mut invalid_sections = ProfileDetails::default();
        invalid_sections.section_order = vec!["me".to_string(), "me".to_string()];
        assert!(invalid_sections.validate().is_err());

        let mut invalid_country = ProfileDetails::default();
        invalid_country.country = "ZZZ".to_string();
        assert!(invalid_country.validate().is_err());

        let mut valid_country = ProfileDetails::default();
        valid_country.country = "DE".to_string();
        assert!(valid_country.validate().is_ok());
    }
}
