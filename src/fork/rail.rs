//! How the account rail looks: a name, an emoji or a picture per account,
//! and whether the rail is folded away.
//!
//! Kept in one file, `rail.json`, in the unnamed profile's config directory,
//! with pictures copied beside it under `rail/`. Accounts are keyed by profile
//! name, the unnamed one by the empty string. A missing or unreadable file
//! means defaults, never an error: the rail is decoration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// Side of the stored picture, in pixels: a 40-point button on a 2x display.
const PICTURE_SIDE: u32 = 96;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Look {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// File name of the picture inside `rail/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Rail {
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub looks: BTreeMap<String, Look>,
    #[serde(skip)]
    dir: PathBuf,
}

impl Rail {
    pub fn load(config: &Path) -> Self {
        let mut rail: Rail = std::fs::read(config.join("rail.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        rail.dir = config.to_owned();
        rail
    }

    pub fn save(&self) {
        if self.dir.as_os_str().is_empty() {
            return;
        }
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(self.dir.join("rail.json"), bytes) {
                    log::warn!("rail not saved: {error}");
                }
            }
            Err(error) => log::warn!("rail not saved: {error}"),
        }
    }

    pub fn look(&self, key: &str) -> Look {
        self.looks.get(key).cloned().unwrap_or_default()
    }

    pub fn set_look(&mut self, key: &str, look: Look) {
        let old = self.looks.get(key).and_then(|l| l.picture.clone());
        if old.is_some() && old != look.picture {
            let _ = std::fs::remove_file(self.pictures().join(old.unwrap()));
        }
        if look == Look::default() {
            self.looks.remove(key);
        } else {
            self.looks.insert(key.to_owned(), look);
        }
        self.save();
    }

    pub fn pictures(&self) -> PathBuf {
        self.dir.join("rail")
    }

    /// Copies `source`, shrunk to a square thumbnail, beside the rail and
    /// returns its file name. The original may move or vanish later.
    pub fn import_picture(&self, key: &str, source: &Path) -> Result<String, String> {
        let picture = image::open(source).map_err(|error| error.to_string())?;
        let side = picture.width().min(picture.height());
        let square = picture.crop_imm(
            (picture.width() - side) / 2,
            (picture.height() - side) / 2,
            side,
            side,
        );
        let thumb = square.thumbnail(PICTURE_SIDE, PICTURE_SIDE).to_rgba8();
        std::fs::create_dir_all(self.pictures()).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let file = format!("{}-{stamp}.png", if key.is_empty() { "main" } else { key });
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(thumb)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        std::fs::write(self.pictures().join(&file), bytes).map_err(|error| error.to_string())?;
        Ok(file)
    }

    /// The picture as a texture, decoded once and cached by file name.
    pub fn texture(&self, ctx: &egui::Context, file: &str) -> Option<egui::TextureHandle> {
        let id = egui::Id::new(("zapzapfast-rail-picture", file));
        if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
            return Some(texture);
        }
        let picture = image::open(self.pictures().join(file)).ok()?.to_rgba8();
        let size = [picture.width() as usize, picture.height() as usize];
        let texture = ctx.load_texture(
            format!("rail-{file}"),
            egui::ColorImage::from_rgba_unmultiplied(size, picture.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        Some(texture)
    }
}

/// The first grapheme typed, if it is an emoji; anything else is dropped.
pub fn emoji_of(text: &str) -> Option<String> {
    let first = text.trim().graphemes(true).next()?;
    crate::emoji::is_emoji(first).then(|| first.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_emoji_is_kept() {
        assert_eq!(emoji_of(" 🐶 perro").as_deref(), Some("🐶"));
        assert_eq!(emoji_of("abc"), None);
        assert_eq!(emoji_of(""), None);
    }

    #[test]
    fn looks_survive_a_reload() {
        let dir = std::env::temp_dir().join(format!("zapfast-rail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut rail = Rail::load(&dir);
        rail.hidden = true;
        rail.set_look(
            "work",
            Look {
                name: Some("Trabajo".into()),
                emoji: Some("💼".into()),
                picture: None,
            },
        );
        let again = Rail::load(&dir);
        assert!(again.hidden);
        assert_eq!(again.look("work").name.as_deref(), Some("Trabajo"));
        assert_eq!(again.look(""), Look::default());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
