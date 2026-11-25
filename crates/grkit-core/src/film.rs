//! The film stock of an effect (feature 128): what Grain and Glow need to draw
//! the grain and the halation of that stock.
//!
//! An effect that is a fit of a film simulation can name the kind of film, its
//! base, its anti-halation layer, and its exposure index. The GRF holds them in
//! four bytes at `abi::GRF_FILM`: the `FLAG_*` bits, a zero byte, and the
//! exposure index as a u16. INSTALL copies them into the record, and modules
//! read them through the service `picture_film`.

use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};

/// Bytes of the GRF extension that hold the film, from offset 120.
pub const SIZE: usize = 4;
/// A slide (reversal) film, scanned directly; else a negative printed on paper.
pub const FLAG_POSITIVE: u8 = 1;
/// A motion picture film on a PET base; else a still film on a triacetate base.
pub const FLAG_CINE: u8 = 2;
/// A weak anti-halation layer, which lets about five times the light of a
/// strong one back into the red layer.
pub const FLAG_WEAK_ANTIHALATION: u8 = 4;
const FLAGS: u8 = FLAG_POSITIVE | FLAG_CINE | FLAG_WEAK_ANTIHALATION;
/// The bit that `picture_film` sets for an effect with a film.
pub const KNOWN: u32 = 0x80;
/// The exposure index of a film, in ISO.
pub const ISO_MIN: u16 = 1;
pub const ISO_MAX: u16 = 12800;

/// The film stock of an effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Film {
    pub positive: bool,
    pub cine: bool,
    pub weak_antihalation: bool,
    /// The exposure index, the box speed or that of a push.
    pub iso: u16,
}

fn choice(value: &Value, key: &str, no: &str, yes: &str) -> Result<bool> {
    match value[key].as_str() {
        Some(v) if v == no => Ok(false),
        Some(v) if v == yes => Ok(true),
        _ => bail!("Expected film {key} {no} or {yes}"),
    }
}

impl Film {
    /// Reads the `film` object of a definition or of the decoded JSON form.
    pub fn from_json(value: &Value) -> Result<Self> {
        let object = value
            .as_object()
            .context("Expected film with type, use, antihalation, and iso")?;
        ensure!(
            object.len() == 4
                && ["type", "use", "antihalation", "iso"]
                    .iter()
                    .all(|k| object.contains_key(*k)),
            "Expected film with type, use, antihalation, and iso"
        );
        let iso = value["iso"]
            .as_u64()
            .filter(|v| (u64::from(ISO_MIN)..=u64::from(ISO_MAX)).contains(v))
            .context("Expected a film iso of 1 to 12800")? as u16;
        Ok(Self {
            positive: choice(value, "type", "negative", "positive")?,
            cine: choice(value, "use", "still", "cine")?,
            weak_antihalation: choice(value, "antihalation", "strong", "weak")?,
            iso,
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "type": if self.positive { "positive" } else { "negative" },
            "use": if self.cine { "cine" } else { "still" },
            "antihalation": if self.weak_antihalation { "weak" } else { "strong" },
            "iso": self.iso,
        })
    }

    pub fn flags(&self) -> u8 {
        (if self.positive { FLAG_POSITIVE } else { 0 })
            | (if self.cine { FLAG_CINE } else { 0 })
            | (if self.weak_antihalation {
                FLAG_WEAK_ANTIHALATION
            } else {
                0
            })
    }

    pub fn to_bytes(&self) -> [u8; SIZE] {
        let iso = self.iso.to_le_bytes();
        [self.flags(), 0, iso[0], iso[1]]
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() == SIZE, "Expected {SIZE} film bytes");
        let iso = u16::from_le_bytes([bytes[2], bytes[3]]);
        ensure!(
            bytes[0] & !FLAGS == 0 && bytes[1] == 0 && (ISO_MIN..=ISO_MAX).contains(&iso),
            "Unsupported film bytes"
        );
        Ok(Self {
            positive: bytes[0] & FLAG_POSITIVE != 0,
            cine: bytes[0] & FLAG_CINE != 0,
            weak_antihalation: bytes[0] & FLAG_WEAK_ANTIHALATION != 0,
            iso,
        })
    }

    /// The value of `picture_film` for this film.
    pub fn service_value(&self) -> u32 {
        KNOWN | u32::from(self.flags()) | u32::from(self.iso) << 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_and_bytes_round_trip() {
        let value = json!({"type": "positive", "use": "still", "antihalation": "weak", "iso": 64});
        let film = Film::from_json(&value).unwrap();
        assert_eq!(film.to_json(), value);
        assert_eq!(film.to_bytes(), [5, 0, 64, 0]);
        assert_eq!(Film::from_bytes(&film.to_bytes()).unwrap(), film);
        assert_eq!(film.service_value(), 0x80 | 5 | 64 << 16);
    }

    #[test]
    fn rejects_unknown_values() {
        for value in [
            json!({"type": "slide", "use": "still", "antihalation": "weak", "iso": 64}),
            json!({"type": "negative", "use": "still", "antihalation": "none", "iso": 64}),
            json!({"type": "negative", "use": "still", "antihalation": "weak", "iso": 0}),
            json!({"type": "negative", "use": "still", "antihalation": "weak"}),
            json!({"type": "negative", "use": "still", "antihalation": "weak", "iso": 64, "x": 1}),
        ] {
            assert!(Film::from_json(&value).is_err(), "{value}");
        }
        assert!(Film::from_bytes(&[8, 0, 64, 0]).is_err());
        assert!(Film::from_bytes(&[0, 1, 64, 0]).is_err());
        assert!(Film::from_bytes(&[0, 0, 0, 0]).is_err());
    }
}
