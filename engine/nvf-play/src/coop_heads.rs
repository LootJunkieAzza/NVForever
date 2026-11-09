//! The head a co-op player shows as on the others' maps (queue item 134, owner
//! 2026-11-09: "each player should be vaultboy head on the map, maybe make it
//! customizable with different heads from different perks").
//!
//! - **The choices**: the Vault Boy faces of the game's message icons
//!   (`interface\icons\message icons\glow_message_vaultboy_*.dds`), then the picture of each
//!   perk the game has (PERK `ICON`, named by its `FULL`), playable and not hidden, each
//!   picture once [design]. The faces' names are NVForever's own words (the files have
//!   none).
//! - **The default** is the plain Vault Boy, [`PLAIN`]; [`MapHead`] holds the choice (empty
//!   for the default).
//! - **Kept** with the character in the save (`coop_map_head`), and sent to the others with
//!   the look (`nvforever`'s `coop.rs` `encode_look`).
//! - **Chosen** on the Pip-Boy's World Map and Local Map (the Map Icon button,
//!   `crate::pipboy::data`), in a co-op session.

use bevy::prelude::*;

/// The plain Vault Boy: the default head.
pub const PLAIN: &str = "interface\\icons\\message icons\\glow_message_vaultboy_neutral.dds";

/// The Vault Boy faces of the message icons: NVForever's name for each, and the file's
/// part after `glow_message_vaultboy_` [observed: Fallout - Textures2.bsa].
const FACES: [(&str, &str); 21] = [
    ("Vault Boy", "neutral"),
    ("Very Happy", "very_happy"),
    ("Thinking", "thinking"),
    ("Surprised", "surprised"),
    ("Sad", "sad"),
    ("Angry", "angry"),
    ("In Pain", "in_pain"),
    ("Ninja", "ninja"),
    ("White Glove", "white_glove"),
    ("Wasteland", "wasteland"),
    ("Goodsprings", "goodsprings"),
    ("Primm", "primm"),
    ("Novac", "novac"),
    ("Freeside", "freeside"),
    ("The Strip", "strip"),
    ("NCR", "ncr"),
    ("Legion", "legion"),
    ("Brotherhood", "brotherhood"),
    ("Great Khans", "khans"),
    ("Boomers", "boomers"),
    ("Followers", "apocalypse"),
];

/// This player's head for the others' maps: an icon's path under `textures\`, empty for
/// [`PLAIN`].
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct MapHead(pub String);

impl MapHead {
    pub fn picture(&self) -> &str {
        picture(&self.0)
    }
}

/// The picture of a head as sent (empty: the plain Vault Boy).
pub fn picture(head: &str) -> &str {
    if head.is_empty() {
        PLAIN
    } else {
        head
    }
}

/// A head the player can choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    /// The icon's path under `textures\`, lower case.
    pub path: String,
}

/// The heads offered: the faces, then the perks' pictures by name (module notes).
pub fn choices(perks: impl IntoIterator<Item = (String, String)>) -> Vec<Choice> {
    let mut out: Vec<Choice> = FACES.iter().map(|(name, file)| Choice { name: (*name).into(), path: format!("interface\\icons\\message icons\\glow_message_vaultboy_{file}.dds") }).collect();
    let mut perks: Vec<Choice> = perks
        .into_iter()
        .filter(|(name, path)| !name.trim().is_empty() && !path.trim().is_empty())
        .map(|(name, path)| Choice { name, path: path.trim().trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase() })
        .collect();
    perks.sort_by(|a, b| a.name.cmp(&b.name));
    for c in perks {
        if !out.iter().any(|o| o.path == c.path) {
            out.push(c);
        }
    }
    out
}

/// The perks of `game`'s data offered as heads: playable, not hidden, not traits, with a
/// picture.
pub fn perk_heads(game: &fnv_game::Game) -> Vec<(String, String)> {
    game.data.perks.values().filter(|p| p.playable && !p.hidden && !p.is_trait).filter_map(|p| Some((p.name.clone(), p.icon.clone()?))).collect()
}

/// The head as kept and sent: empty for the plain Vault Boy.
pub fn kept(path: &str) -> String {
    if path.eq_ignore_ascii_case(PLAIN) {
        String::new()
    } else {
        path.to_string()
    }
}

pub struct CoopHeadsPlugin;

impl Plugin for CoopHeadsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapHead>();
        crate::save::add_save_hook(
            app,
            crate::save::SaveHook {
                key: "coop_map_head",
                save: |w| w.get_resource::<MapHead>().filter(|h| !h.0.is_empty()).map(|h| serde_json::Value::String(h.0.clone())),
                load: |w, v| {
                    let head = v.as_str().map(kept).unwrap_or_default();
                    info!("co-op map head: {} from the save", if head.is_empty() { "the plain Vault Boy" } else { head.as_str() });
                    w.insert_resource(MapHead(head));
                },
                remap: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_faces_come_first_then_each_perk_picture_once_by_name() {
        let perks = vec![
            ("Toughness".to_string(), "Interface\\Icons\\PipboyImages\\Perks\\perk_toughness.dds".to_string()),
            ("Action Boy".to_string(), "interface\\icons\\pipboyimages\\perks\\perk_action_boy.dds".to_string()),
            ("Action Girl".to_string(), "interface\\icons\\pipboyimages\\perks\\perk_action_boy.dds".to_string()),
            ("".to_string(), "x.dds".to_string()),
        ];
        let c = choices(perks);
        assert_eq!(c[0], Choice { name: "Vault Boy".into(), path: PLAIN.into() });
        assert_eq!(c.len(), FACES.len() + 2);
        assert_eq!(c[FACES.len()].name, "Action Boy");
        assert_eq!(c[FACES.len() + 1].path, "interface\\icons\\pipboyimages\\perks\\perk_toughness.dds");
    }

    #[test]
    fn the_plain_head_is_kept_and_sent_as_nothing() {
        assert_eq!(kept(PLAIN), "");
        assert_eq!(picture(""), PLAIN);
        assert_eq!(kept("a.dds"), "a.dds");
    }
}
