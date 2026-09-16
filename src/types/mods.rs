use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Mods: u32 {
        const NOMOD       = 0;
        const NOFAIL      = 1 << 0;
        const EASY        = 1 << 1;
        const TOUCHSCREEN = 1 << 2;
        const HIDDEN      = 1 << 3;
        const HARDROCK    = 1 << 4;
        const SUDDENDEATH = 1 << 5;
        const DOUBLETIME  = 1 << 6;
        const RELAX       = 1 << 7;
        const HALFTIME    = 1 << 8;
        const NIGHTCORE   = 1 << 9;
        const FLASHLIGHT  = 1 << 10;
        const AUTOPLAY    = 1 << 11;
        const SPUNOUT     = 1 << 12;
        const AUTOPILOT   = 1 << 13;
        const PERFECT     = 1 << 14;
        const KEY4        = 1 << 15;
        const KEY5        = 1 << 16;
        const KEY6        = 1 << 17;
        const KEY7        = 1 << 18;
        const KEY8        = 1 << 19;
        const FADEIN      = 1 << 20;
        const RANDOM      = 1 << 21;
        const CINEMA      = 1 << 22;
        const TARGET      = 1 << 23;
        const KEY9        = 1 << 24;
        const KEYCOOP     = 1 << 25;
        const KEY1        = 1 << 26;
        const KEY3        = 1 << 27;
        const KEY2        = 1 << 28;
        const SCOREV2     = 1 << 29;
        const MIRROR      = 1 << 30;
    }
}

impl serde::Serialize for Mods {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.bits())
    }
}

impl<'de> serde::Deserialize<'de> for Mods {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bits = u32::deserialize(deserializer)?;
        Ok(Mods::from_bits_truncate(bits))
    }
}

impl Mods {
    pub const SPEED_CHANGING: Mods = Self::DOUBLETIME.union(Self::NIGHTCORE).union(Self::HALFTIME);

    pub const INVALID_STANDARD: Mods = Self::AUTOPILOT.union(Self::RELAX).union(Self::AUTOPLAY).union(Self::CINEMA).union(Self::TARGET);

    pub const INVALID_AUTOPILOT: Mods = Self::RELAX.union(Self::AUTOPLAY).union(Self::CINEMA).union(Self::TARGET);

    pub const INVALID_RELAX: Mods = Self::AUTOPILOT.union(Self::AUTOPLAY).union(Self::CINEMA).union(Self::TARGET);

    pub fn short_name(self) -> String {
        if self.is_empty() {
            return "NM".to_string();
        }

        let mod_map: &[(Mods, &str)] = &[
            (Mods::NOFAIL, "NF"),
            (Mods::EASY, "EZ"),
            (Mods::TOUCHSCREEN, "TD"),
            (Mods::HIDDEN, "HD"),
            (Mods::HARDROCK, "HR"),
            (Mods::SUDDENDEATH, "SD"),
            (Mods::DOUBLETIME, "DT"),
            (Mods::RELAX, "RX"),
            (Mods::HALFTIME, "HT"),
            (Mods::NIGHTCORE, "NC"),
            (Mods::FLASHLIGHT, "FL"),
            (Mods::AUTOPLAY, "AU"),
            (Mods::SPUNOUT, "SO"),
            (Mods::AUTOPILOT, "AP"),
            (Mods::PERFECT, "PF"),
            (Mods::KEY4, "K4"),
            (Mods::KEY5, "K5"),
            (Mods::KEY6, "K6"),
            (Mods::KEY7, "K7"),
            (Mods::KEY8, "K8"),
            (Mods::FADEIN, "FI"),
            (Mods::RANDOM, "RN"),
            (Mods::CINEMA, "CN"),
            (Mods::TARGET, "TP"),
            (Mods::KEY9, "K9"),
            (Mods::KEYCOOP, "CO"),
            (Mods::KEY1, "K1"),
            (Mods::KEY3, "K3"),
            (Mods::KEY2, "K2"),
            (Mods::SCOREV2, "V2"),
            (Mods::MIRROR, "MR"),
        ];

        let mut result = String::new();
        for (m, name) in mod_map {
            if self.contains(*m) && *m != Mods::SPEED_CHANGING {
                result.push_str(name);
            }
        }
        result
    }

    pub fn filter_invalid_combos(mut self) -> Mods {
        if self.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && self.contains(Mods::HALFTIME) {
            self.remove(Mods::HALFTIME);
        }
        if self.contains(Mods::EASY) && self.contains(Mods::HARDROCK) {
            self.remove(Mods::HARDROCK);
        }
        if self.contains(Mods::RELAX) && self.contains(Mods::AUTOPILOT) {
            self.remove(Mods::AUTOPILOT);
        }
        if self.contains(Mods::PERFECT) && self.contains(Mods::SUDDENDEATH) {
            self.remove(Mods::SUDDENDEATH);
        }
        self
    }

    pub fn from_short_str(s: &str) -> Mods {
        let mod_map: &[(&str, Mods)] = &[
            ("EZ", Mods::EASY),
            ("NF", Mods::NOFAIL),
            ("HD", Mods::HIDDEN),
            ("PF", Mods::PERFECT),
            ("SD", Mods::SUDDENDEATH),
            ("HR", Mods::HARDROCK),
            ("NC", Mods::NIGHTCORE),
            ("DT", Mods::DOUBLETIME),
            ("HT", Mods::HALFTIME),
            ("FL", Mods::FLASHLIGHT),
            ("SO", Mods::SPUNOUT),
            ("RX", Mods::RELAX),
            ("AP", Mods::AUTOPILOT),
        ];

        let upper = s.to_uppercase();
        let mut mods = Mods::empty();

        let chars: Vec<char> = upper.chars().collect();
        let mut i = 0;
        while i + 1 < chars.len() {
            let pair: String = [chars[i], chars[i + 1]].iter().collect();
            for (name, m) in mod_map {
                if pair == *name {
                    mods |= *m;
                    break;
                }
            }
            i += 2;
        }

        mods.filter_invalid_combos()
    }
}

impl std::fmt::Display for Mods {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.short_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mods_display_and_parsing() {
        assert_eq!(Mods::NOMOD.short_name(), "NM");

        let hddt = Mods::HIDDEN | Mods::DOUBLETIME;
        assert_eq!(hddt.short_name(), "HDDT");

        let parsed = Mods::from_short_str("HDHR");
        assert_eq!(parsed, Mods::HIDDEN | Mods::HARDROCK);

        let parsed_nc = Mods::from_short_str("NC");
        assert_eq!(parsed_nc, Mods::NIGHTCORE);
    }
}
