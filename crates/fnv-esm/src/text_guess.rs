//! A guess at the code page of a load order's text, for a start where none is set
//! (#1443; docs/spec-notes/fonts.md §2.1, "How it is chosen").
//!
//! The plugins carry no code page [observed: docs/spec-notes/esm.md, "Text"], and a
//! Russian install's INI says `sLanguage=ENGLISH` [observed: fonts.md §2.1], so the text
//! itself is read. In Windows-1251 the Cyrillic letters are the codes 0xC0 to 0xFF (and
//! 0xA8, 0xB8) [documented: WHATWG windows-1251 index]; a Russian name is nearly all such
//! bytes. Western European text in Windows-1252 has letters there too (accented ones), but
//! only a few among plain ASCII letters. So a plugin whose letters are mostly 0xC0 to 0xFF
//! is taken for Cyrillic [design; hypothesis: one Russian install looked at, where its
//! language plugins' names and lines are]. Windows-1250 is not told apart from 1252: the
//! letters of Polish or Czech text are mostly ASCII as well, so it is still set by hand.
//! For 936, valid GBK pairs need independent evidence: ordinary Russian bytes can
//! also decode to Han. See the simplified Chinese heuristic in fonts.md §2.1.

use crate::raw::{record_flags, visit_records};
use crate::Plugin;
use fnv_util::text::CodePage;

/// Text bytes read from a plugin at most: enough to tell its script, little time.
const SAMPLE_BYTES: usize = 64 * 1024;
/// High letters a plugin needs before it counts at all.
const MIN_HIGH: usize = 100;

/// What [`guess`] saw in one plugin.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PluginText {
    pub plugin: String,
    /// Letter bytes read: ASCII letters and the codes 0xC0 to 0xFF.
    pub letters: usize,
    /// Of them, the codes 0xC0 to 0xFF.
    pub high: usize,
    gbk: GbkSample,
}

impl PluginText {
    /// Mostly high letters: Cyrillic in Windows-1251 (module notes).
    pub fn looks_cyrillic(&self) -> bool {
        self.high >= MIN_HIGH && self.high * 2 >= self.letters
    }

    fn candidate(&self) -> Option<CodePage> {
        if self.gbk.looks_gbk() {
            Some(CodePage::Windows936)
        } else {
            self.looks_cyrillic().then_some(CodePage::Windows1251)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct GbkSample {
    non_ascii: usize,
    valid_non_ascii: usize,
    pairs: usize,
    han_pairs: usize,
    distinctive_pairs: usize,
}

impl GbkSample {
    fn read(&mut self, text: &[u8]) {
        let text = text.split(|&b| b == 0).next().unwrap_or_default();
        let high = text.iter().filter(|b| !b.is_ascii()).count();
        self.non_ascii += high;
        if high == 0 {
            return;
        }
        let Some(decoded) = CodePage::Windows936.decode_strict(text) else {
            return;
        };
        self.valid_non_ascii += high;
        let mut bytes = text.iter().copied();
        for ch in decoded.chars() {
            let lead = bytes.next().expect("validated GB18030 code point");
            // The WHATWG GB18030 decoder accepts 0x80 as a one-byte euro sign.
            if lead <= 0x80 {
                continue;
            }
            let trail = bytes.next().expect("validated GB18030 lead byte");
            if trail.is_ascii_digit() {
                bytes.nth(1); // skip the last two bytes of a validated four-byte sequence
                continue;
            }
            self.pairs += 1;
            if matches!(
                ch,
                '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
            ) {
                self.han_pairs += 1;
            }
            if outside_russian_alphabet(lead) || outside_russian_alphabet(trail) {
                self.distinctive_pairs += 1;
            }
        }
    }

    fn looks_gbk(&self) -> bool {
        self.pairs >= 50
            && self.valid_non_ascii * 10 >= self.non_ascii * 9
            && self.han_pairs * 2 >= self.pairs
            && self.distinctive_pairs * 5 >= self.pairs
    }
}

/// A byte that Windows-1251 uses for a Cyrillic letter outside the Russian alphabet.
fn outside_russian_alphabet(byte: u8) -> bool {
    matches!(CodePage::Windows1251.char_of(byte), 'Ђ'..='Џ' | 'ђ'..='џ' | 'Ґ' | 'ґ')
}

/// The guess and what it was made from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Guess {
    /// A page supported by enough sampled plugin text, if any.
    pub code_page: Option<CodePage>,
    pub plugins: Vec<PluginText>,
}

impl Guess {
    /// The plugins that met the Windows-1251 letter-byte heuristic, by name.
    pub fn cyrillic(&self) -> Vec<&str> {
        self.plugins.iter().filter(|p| p.looks_cyrillic()).map(|p| p.plugin.as_str()).collect()
    }

    /// The plugins that support the selected page, by name.
    pub fn supporting_plugins(&self) -> Vec<&str> {
        let Some(page) = self.code_page else {
            return Vec::new();
        };
        self.plugins
            .iter()
            .filter(|p| p.candidate() == Some(page))
            .map(|p| p.plugin.as_str())
            .collect()
    }
}

/// Count the letter bytes of a text: (letters, high letters).
fn count(text: &[u8]) -> (usize, usize) {
    text.iter().fold((0, 0), |(l, h), &b| match b {
        0xC0..=0xFF => (l + 1, h + 1),
        b if b.is_ascii_alphabetic() => (l + 1, h),
        _ => (l, h),
    })
}

/// The names (FULL) and dialogue lines (NAM1) of one plugin, up to [`SAMPLE_BYTES`];
/// compressed records are passed over (reading them costs more than they tell).
pub fn plugin_text(plugin: &Plugin) -> PluginText {
    let mut out = PluginText { plugin: plugin.name.clone(), ..Default::default() };
    let mut read = 0usize;
    visit_records(plugin.nodes.get(1..).unwrap_or_default(), &mut |r, _| {
        if read >= SAMPLE_BYTES || r.header.flags & record_flags::COMPRESSED != 0 {
            return;
        }
        let _ = r.visit_subrecords(|sig, data| {
            if sig == b"FULL" || sig == b"NAM1" {
                let (l, h) = count(data);
                out.letters += l;
                out.high += h;
                let gbk_len = data.len().min(SAMPLE_BYTES.saturating_sub(read));
                out.gbk.read(&data[..gbk_len]);
                read += data.len();
            }
        });
    });
    out
}

/// Guess the code page of a load order's text (module notes).
pub fn guess(plugins: &[Plugin]) -> Guess {
    let plugins: Vec<PluginText> = plugins.iter().map(plugin_text).collect();
    Guess { code_page: decide(&plugins), plugins }
}

fn decide(plugins: &[PluginText]) -> Option<CodePage> {
    let (gbk, other) = plugins.iter().fold((0, 0), |(g, o), p| {
        if p.gbk.looks_gbk() {
            (g + p.gbk.non_ascii, o)
        } else {
            (g, o + p.gbk.non_ascii)
        }
    });
    if gbk > other {
        return Some(CodePage::Windows936);
    }
    let (cyrillic, other) = plugins.iter().fold((0, 0), |(c, o), p| {
        if p.candidate() == Some(CodePage::Windows1251) {
            (c + p.high, o)
        } else {
            (c, o + p.high)
        }
    });
    (cyrillic > other).then_some(CodePage::Windows1251)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(name: &str, s: &str, page: CodePage) -> PluginText {
        let bytes = page.encode(&s.repeat(40)).unwrap();
        let (letters, high) = count(&bytes);
        let mut gbk = GbkSample::default();
        gbk.read(&bytes);
        PluginText { plugin: name.into(), letters, high, gbk }
    }

    #[test]
    fn cyrillic_text_is_told_from_western() {
        use CodePage::*;
        assert!(text("ru", "Стимулятор Докторский саквояж", Windows1251).looks_cyrillic());
        assert!(!text("fr", "Stimulant Sacoche de médecin, très utile", Windows1252).looks_cyrillic());
        assert!(!text("de", "Größe Ärztetasche für Überlebende", Windows1252).looks_cyrillic());
        assert!(!text("pl", "Torba lekarska, żółć i źdźbło", Windows1250).looks_cyrillic());
        assert!(!text("en", "Stimpak Doctor's Bag", Windows1252).looks_cyrillic());
        let russian = text(
            "park",
            "«Утром я видел ёжика, а вечером — лису».",
            Windows1251,
        );
        assert_eq!(decide(&[russian]), Some(Windows1251));
        // A few high letters are not enough to count.
        assert!(!text("short", "Ж", Windows1251).looks_cyrillic());
    }

    #[test]
    fn the_larger_text_decides() {
        use CodePage::*;
        let en = text("FalloutNV.esm", "Stimpak Doctor's Bag Café", Windows1252);
        let ru = text("FalloutNV_lang.esp", "Стимулятор Докторский саквояж", Windows1251);
        let fr = text("Mod.esp", "Sacoche de médecin, très utile, déjà vu, à côté, où", Windows1252);
        assert_eq!(decide(std::slice::from_ref(&en)), None);
        assert_eq!(decide(&[en.clone(), ru.clone()]), Some(Windows1251));
        assert_eq!(decide(&[en, fr.clone()]), None);
        // A Western mod with more high letters than the Russian one outweighs it.
        let small_ru = PluginText { plugin: "ru".into(), letters: 150, high: 140, ..Default::default() };
        let big_fr = PluginText { plugin: "fr".into(), letters: 10_000, high: 900, ..Default::default() };
        assert_eq!(decide(&[small_ru, big_fr]), None);
        assert_eq!(decide(&[]), None);
    }

    #[test]
    fn letters_are_counted() {
        assert_eq!(count(b"Ab c1!"), (3, 0));
        assert_eq!(count(&[0xC0, 0xFF, b'a', 0xA8, 0x20]), (3, 2));
    }

    #[test]
    fn gbk_evidence_counts_two_byte_pairs_in_whole_fields() {
        let mut sample = GbkSample::default();
        sample.read(b"\x80\xd6\xd0\x95\x32\x82\x36\x81\x40\0\xff");
        assert_eq!(sample.non_ascii, 6);
        assert_eq!(sample.valid_non_ascii, 6);
        assert_eq!(sample.pairs, 2);
        assert_eq!(sample.han_pairs, 2);
        assert_eq!(sample.distinctive_pairs, 1);

        sample.read(b"\x81");
        sample.read(b"@");
        assert_eq!(sample.non_ascii, 7);
        assert_eq!(sample.valid_non_ascii, 6);
        assert_eq!(sample.pairs, 2);
        assert!(!sample.looks_gbk());
    }
}
