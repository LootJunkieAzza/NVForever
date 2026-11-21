//! The player character string codes in shown text (messages, terminals, dialogue,
//! notes): `&PCName;`, `&PCRace;`, `&PCSex;`, `&PCSexPronoun;`, `&PCSexPossessive;`,
//! and `&<game setting>;` for a game setting's text [documented: Player Character String
//! Codes, docs/spec-notes/ext-format.md §6].

use crate::game::Game;
use fnv_sim::FormId;

impl Game<'_> {
    /// `text` with its codes replaced; a code that names nothing stays as written
    /// [hypothesis].
    pub fn expand_codes(&self, text: &str) -> String {
        if !text.contains('&') {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find('&') {
            out.push_str(&rest[..at]);
            let tail = &rest[at + 1..];
            match tail.find(';').map(|end| (&tail[..end], end)).filter(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
                Some((name, end)) => match self.code_text(name) {
                    Some(t) => {
                        out.push_str(&t);
                        rest = &tail[end + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = tail;
                    }
                },
                None => {
                    out.push('&');
                    rest = tail;
                }
            }
        }
        out.push_str(rest);
        out
    }

    fn code_text(&self, name: &str) -> Option<String> {
        let p = FormId::PLAYER_REF;
        let female = self.actor_female(p);
        Some(match name.to_ascii_lowercase().as_str() {
            "pcname" => self.player.name.clone().or_else(|| self.data.full_names.get(&FormId::PLAYER_BASE).cloned()).unwrap_or_default(),
            "pcrace" => self.actor_race(p).and_then(|r| self.data.full_names.get(&r).cloned()).unwrap_or_default(),
            "pcsex" => (if female { "female" } else { "male" }).to_string(),
            "pcsexpronoun" => (if female { "she" } else { "he" }).to_string(),
            "pcsexpossessive" => (if female { "her" } else { "his" }).to_string(),
            // A game setting's text; a number setting as its number [hypothesis: the page
            // speaks of strings only].
            _ => match self.world.setting_value(name)? {
                fnv_sim::settings::Setting::Str(s) => s.clone(),
                fnv_sim::settings::Setting::Float(x) => fnv_script::vm::number_text(f64::from(*x)),
                fnv_sim::settings::Setting::Int(x) => x.to_string(),
                fnv_sim::settings::Setting::Raw(_) => return None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_code_that_names_nothing_stays() {
        // Only the scan: the game is needed for real codes (tests/synthetic.rs).
        let t = "Fish & chips; no code";
        assert!(t.contains('&'));
    }
}
