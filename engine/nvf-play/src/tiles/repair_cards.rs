//! The two pairs of cards a repair shows [documented: repair_services_menu.xml, repair_menu.xml,
//! names only]: the item's condition and its figure now, the same after the repair, the arrow
//! and the gains between them. The vendor's menu writes them (`super::repair_services_menu`),
//! the Pip-Boy's repair page under its own prefix (`super::repair_menu`), and the weapon mod menu
//! its own pair of cards named its own way (`super::item_mod_menu`, item_mod_menu.xml).
//!
//! - **The values are words** ([`CardValues`]): the repair menus' whole positive gains
//!   ([`Figures`], "+N%" and "+N" as before) and the mod menu's figures, which can be decimal,
//!   negative or worded ("0.40", "-0.50", "Silent"), go through one writer; each menu's model
//!   words its own figures [design].
//! - A card's `_Title` and `_Value` show through its `CI_TitleText` and `CI_ValueText`
//!   [documented: card_info.xml]. The condition card has a meter in place of a number.
//! - The stat card's title is the setting of the figure it shows (`sInventoryDamage`,
//!   `sInventoryDamageThreshold`), the file leaving it to code [documented: repair.md §3]. The
//!   short words the original shows there are not in the data files and are not transcribed
//!   [design: LEGAL.md].
//! - The condition card's title is the file's. It is written, from `sInventoryCondition`, only
//!   where the file left a card without one [hypothesis: which cards have one in the original's
//!   file is known only for the first, repair.md §3].
//! - The meter shows the card's `user0`, 0 to 1: the file makes the meter's `_Value` copy it, so
//!   a write on the meter would not last [documented: the files, `meter.xml`; observed: private
//!   job run07_repair_services, 1280x720, `_Value` 0.55, 0.60, 1.00, each the row's condition].
//! - The arrow is left to the file: it shows on a refused row too, and the Pip-Boy's file words
//!   its own rule [observed: the same job; documented: repair_menu.xml].
//! - **A title wider than its place** ([`fit_meters`]): the meter stands where the file puts it,
//!   which leaves room for a short word. The setting's word NVForever supplies, "Condition"
//!   (`fnv_ui::strings::BUILT_IN`), is longer, and the meter would cover its end; the short word
//!   the original shows is not transcribed. So the meter gives up room from its left and keeps
//!   its right end. A shorter word from a plugin's setting leaves the file's place [design].

use super::item_menu::{on, set};
use fnv_ui::{Ui, Value};

/// A card pair's figures: the condition, 0 to 1, and the number on the stat card if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Card {
    pub condition: f32,
    pub stat: Option<i32>,
}

/// What the cards show of an item.
#[derive(Clone, Debug, PartialEq)]
pub struct Figures {
    /// `sInventoryCondition`, for a card whose file gives none.
    pub condition_title: String,
    /// The setting's words for the stat card.
    pub stat_title: String,
    pub now: Card,
    /// After the repair; `None` hides that pair, the arrow and the gains.
    pub after: Option<Card>,
    /// What the repair adds to the condition, in percent.
    pub gain_percent: i32,
    /// What the repair adds to the stat.
    pub gain_stat: Option<i32>,
}

/// Between a title and a meter moved for it [design].
const METER_GAP: f64 = 10.0;

/// The sides, in the file's words [documented: the tiles' names].
const NOW: &str = "BrokenItem";
const AFTER: &str = "FixedItem";

fn health(root: &str, prefix: &str, side: &str) -> String {
    format!("{root}/{prefix}{side}Health")
}

fn stat(root: &str, prefix: &str, side: &str) -> String {
    format!("{root}/{prefix}{side}Stat")
}

/// The paths of a menu's card tiles: the two pairs, the arrow and the gains. A condition card's
/// meter is the card's child named after it with `Meter` after the name [documented: the repair
/// files' and item_mod_menu.xml's names]. The repair menus name all of them by one prefix and
/// side ([`CardTiles::repair`]); the weapon mod menu by its own (`super::item_mod_menu`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardTiles {
    pub now_health: String,
    pub now_stat: String,
    pub after_health: String,
    pub after_stat: String,
    pub arrow: String,
    pub health_gain: String,
    pub stat_gain: String,
}

impl CardTiles {
    /// The repair menus' cards under `root`, named with `prefix`.
    pub fn repair(root: &str, prefix: &str) -> Self {
        CardTiles {
            now_health: health(root, prefix, NOW),
            now_stat: stat(root, prefix, NOW),
            after_health: health(root, prefix, AFTER),
            after_stat: stat(root, prefix, AFTER),
            arrow: format!("{root}/{prefix}ArrowIcon"),
            health_gain: format!("{root}/{prefix}HealthImprovementText"),
            stat_gain: format!("{root}/{prefix}StatImprovementText"),
        }
    }

    /// A condition card's meter.
    pub fn meter(card: &str) -> String {
        let name = card.rsplit('/').next().unwrap_or(card);
        format!("{card}/{name}Meter")
    }

    fn sides(&self) -> [(&str, &str); 2] {
        [(&self.now_health, &self.now_stat), (&self.after_health, &self.after_stat)]
    }

    /// The paths [`write_values`] needs, for an adapter's check that its file was built.
    pub fn paths(&self) -> Vec<String> {
        let mut out = vec![self.arrow.clone(), self.health_gain.clone(), self.stat_gain.clone()];
        for (health, stat) in self.sides() {
            for card in [health, stat] {
                out.extend(super::item_card::CARD_TEXTS.map(|t| format!("{card}/{t}")));
                out.push(card.to_string());
            }
            let meter = Self::meter(health);
            let image = format!("{meter}/MeterImage");
            out.extend([meter, image]);
        }
        out
    }
}

/// One side of the cards as shown: the condition, 0 to 1, and the stat card's value in words.
#[derive(Clone, Debug, PartialEq)]
pub struct SideShown {
    pub condition: f32,
    /// `None` hides the stat card.
    pub stat: Option<String>,
}

/// What the cards show, as words: the repair menus' whole positive gains ([`Figures`]) and the
/// weapon mod menu's figures, which can be decimal, negative or worded ("0.40", "-0.40",
/// "Silent"), alike. The figures are worded by the menu's model; the cards only show them.
#[derive(Clone, Debug, PartialEq)]
pub struct CardValues {
    /// For a condition card whose file gives no title.
    pub condition_title: String,
    /// The stat cards' title.
    pub stat_title: String,
    pub now: SideShown,
    /// `None` hides that pair, the arrow (the file's rule) and the gains.
    pub after: Option<SideShown>,
    /// The gains' words, shown only with `after`; `None` hides one.
    pub health_gain: Option<String>,
    pub stat_gain: Option<String>,
}

impl From<&Figures> for CardValues {
    /// The repair menus' words: "+N%" for the condition whenever there is an after, "+N" for the
    /// stat when it has a gain.
    fn from(f: &Figures) -> Self {
        let side = |c: &Card| SideShown { condition: c.condition, stat: c.stat.map(|n| n.to_string()) };
        CardValues {
            condition_title: f.condition_title.clone(),
            stat_title: f.stat_title.clone(),
            now: side(&f.now),
            after: f.after.as_ref().map(side),
            health_gain: f.after.is_some().then(|| format!("+{}%", f.gain_percent)),
            stat_gain: f.after.and(f.gain_stat).map(|n| format!("+{n}")),
        }
    }
}

/// The paths [`write`] needs under `root`, for an adapter's check that its file was built.
pub fn paths(root: &str, prefix: &str) -> Vec<String> {
    CardTiles::repair(root, prefix).paths()
}

/// `_Title` from `title`, only where the file left the card without one.
pub(super) fn write_title_if_none(ui: &mut Ui, path: &str, title: &str) {
    let Some(t) = ui.find_tile(path) else { return };
    if ui.tile(t).get("_Title").is_none_or(|v| v.text().is_empty()) {
        ui.set_trait(t, "_Title", Value::Str(title.to_string()));
    }
}

fn write_side(ui: &mut Ui, (health, stat): (&str, &str), card: Option<&SideShown>, values: Option<&CardValues>) {
    set(ui, health, "visible", on(card.is_some()));
    let shown = card.and_then(|c| c.stat.as_deref());
    set(ui, stat, "visible", on(shown.is_some()));
    set(ui, stat, "_Value", Value::Str(shown.unwrap_or_default().to_string()));
    if let Some(v) = values {
        write_title_if_none(ui, health, &v.condition_title);
        set(ui, stat, "_Title", Value::Str(v.stat_title.clone()));
    }
    set(ui, health, "_Value", Value::Str(String::new()));
    set(ui, health, "user0", Value::Num(f64::from(card.map_or(0.0, |c| c.condition).clamp(0.0, 1.0))));
}

/// Write the cards `tiles`. `None`: nothing chosen, all hidden. Every tile is written, so nothing
/// stays of the item before.
pub fn write_values(ui: &mut Ui, tiles: &CardTiles, values: Option<&CardValues>) {
    let [now, after] = tiles.sides();
    write_side(ui, now, values.map(|v| &v.now), values);
    write_side(ui, after, values.and_then(|v| v.after.as_ref()), values);
    let after = values.filter(|v| v.after.is_some());
    for (path, gain) in [(&tiles.health_gain, after.and_then(|v| v.health_gain.as_deref())), (&tiles.stat_gain, after.and_then(|v| v.stat_gain.as_deref()))] {
        set(ui, path, "visible", on(gain.is_some()));
        set(ui, path, "string", Value::Str(gain.unwrap_or_default().to_string()));
    }
}

/// Write the repair cards under `root` whose tiles are named with `prefix`. `None`: nothing
/// chosen, all hidden. Every tile is written, so nothing stays of the item before.
pub fn write(ui: &mut Ui, root: &str, prefix: &str, figures: Option<&Figures>) {
    write_values(ui, &CardTiles::repair(root, prefix), figures.map(CardValues::from).as_ref());
}

/// Make room for a title wider than the file's place for it (module notes), on the texts as
/// measured: the meter starts past the title and ends where it did. Nothing moves for a title
/// that fits, or one that would leave the meter no room.
pub fn fit_meters(ui: &mut Ui, root: &str, prefix: &str) {
    fit_meters_of(ui, &CardTiles::repair(root, prefix));
}

/// [`fit_meters`] for the cards `tiles`.
pub fn fit_meters_of(ui: &mut Ui, tiles: &CardTiles) {
    for (health, _) in tiles.sides() {
        let title = ui.find_tile(&format!("{health}/CI_TitleText"));
        let (Some(title), Some(m)) = (title, ui.find_tile(&CardTiles::meter(health))) else { continue };
        let read = |t, name| ui.tile(t).get(name).map_or(0.0, Value::num);
        let start = read(title, "x") + read(title, "width") + METER_GAP;
        let (x, end) = (read(m, "x"), read(m, "x") + read(m, "width"));
        if start > x && start < end {
            ui.set_trait(m, "x", Value::Num(start));
            ui.set_trait(m, "width", Value::Num(end - start));
        }
    }
}

/// A line of the cards shown, for the log (smoke reads it).
pub fn describe(ui: &Ui, root: &str, prefix: &str) -> String {
    describe_of(ui, &CardTiles::repair(root, prefix))
}

/// [`describe`] for the cards `tiles`.
pub fn describe_of(ui: &Ui, tiles: &CardTiles) -> String {
    let mut out = Vec::new();
    for (health, stat) in tiles.sides() {
        for (card, condition) in [(health, true), (stat, false)] {
            if ui.get_float(&format!("{card}/visible")).is_none_or(|v| v == 0.0) {
                continue;
            }
            let title = ui.get_string(&format!("{card}/_Title")).unwrap_or_default();
            let value = if condition {
                ui.get_float(&format!("{card}/user0")).map_or_else(String::new, |v| format!("{:.0}%", v * 100.0))
            } else {
                ui.get_string(&format!("{card}/_Value")).unwrap_or_default()
            };
            out.push(format!("{title} {value}").trim().to_string());
        }
    }
    for path in [&tiles.health_gain, &tiles.stat_gain] {
        if ui.get_float(&format!("{path}/visible")).is_some_and(|v| v != 0.0) {
            out.push(ui.get_string(&format!("{path}/string")).unwrap_or_default());
        }
    }
    out.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    /// [design] A fixture written for this test: the tile names this module looks up, its own
    /// titles, no values or expressions from a game file.
    fn fixture() -> Ui {
        let card = |name: &str, inner: &str, title: &str| format!(r#"<rect name="{name}">{title}<text name="CI_TitleText"/><text name="CI_ValueText"/>{inner}</rect>"#);
        let meter = |name: &str| format!(r#"<rect name="{name}"><image name="MeterImage"/></rect>"#);
        let xml = format!(
            r#"<menu name="M"><rect name="R">{}{}{}{}<image name="XX_ArrowIcon"/><text name="XX_HealthImprovementText"/><text name="XX_StatImprovementText"/></rect></menu>"#,
            card("XX_BrokenItemHealth", &meter("XX_BrokenItemHealthMeter"), "<_Title>Own state</_Title>"),
            card("XX_BrokenItemStat", "", ""),
            card("XX_FixedItemHealth", &meter("XX_FixedItemHealthMeter"), ""),
            card("XX_FixedItemStat", "", ""),
        );
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("m.xml", &xml), "m.xml").unwrap();
        ui.update();
        ui
    }

    fn figures() -> Figures {
        Figures {
            condition_title: "State".into(),
            stat_title: "Force".into(),
            now: Card { condition: 0.25, stat: Some(4) },
            after: Some(Card { condition: 0.75, stat: Some(9) }),
            gain_percent: 50,
            gain_stat: Some(5),
        }
    }

    fn text(ui: &Ui, path: &str) -> String {
        ui.get_string(&format!("M/R/{path}")).unwrap()
    }

    fn number(ui: &Ui, path: &str) -> f64 {
        ui.get_float(&format!("M/R/{path}")).unwrap()
    }

    #[test]
    fn every_path_it_needs_is_in_the_list() {
        let ui = fixture();
        for path in paths("M/R", "XX_") {
            assert!(ui.find_tile(&path).is_some(), "{path}");
        }
        assert!(paths("M/R", "XX_").contains(&"M/R/XX_FixedItemHealth/XX_FixedItemHealthMeter/MeterImage".to_string()));
    }

    #[test]
    fn a_repair_shows_both_pairs_the_arrow_and_the_gains() {
        let mut ui = fixture();
        write(&mut ui, "M/R", "XX_", Some(&figures()));
        assert_eq!(number(&ui, "XX_BrokenItemHealth/user0"), 0.25);
        assert_eq!(number(&ui, "XX_FixedItemHealth/user0"), 0.75);
        assert!(ui.get_float("M/R/XX_BrokenItemHealth/XX_BrokenItemHealthMeter/_Value").is_none(), "the file's, from the card");
        assert_eq!(text(&ui, "XX_BrokenItemStat/_Value"), "4");
        assert_eq!(text(&ui, "XX_FixedItemStat/_Value"), "9");
        assert_eq!(text(&ui, "XX_FixedItemStat/_Title"), "Force");
        assert_eq!(text(&ui, "XX_HealthImprovementText/string"), "+50%");
        assert_eq!(text(&ui, "XX_StatImprovementText/string"), "+5");
        assert!(ui.get_float("M/R/XX_ArrowIcon/visible").is_none(), "the arrow is the file's");
        assert_eq!(describe(&ui, "M/R", "XX_"), "Own state 25%, Force 4, State 75%, Force 9, +50%, +5");
    }

    #[test]
    fn a_title_the_file_gives_stays_and_a_missing_one_is_the_setting() {
        let mut ui = fixture();
        write(&mut ui, "M/R", "XX_", Some(&figures()));
        assert_eq!(text(&ui, "XX_BrokenItemHealth/_Title"), "Own state");
        assert_eq!(text(&ui, "XX_FixedItemHealth/_Title"), "State");
    }

    #[test]
    fn no_repair_hides_the_after_pair_and_nothing_chosen_hides_all() {
        let mut ui = fixture();
        write(&mut ui, "M/R", "XX_", Some(&figures()));
        let beyond = Figures { after: None, gain_percent: 0, gain_stat: None, ..figures() };
        write(&mut ui, "M/R", "XX_", Some(&beyond));
        assert_eq!(number(&ui, "XX_BrokenItemHealth/visible"), 1.0);
        for hidden in ["XX_FixedItemHealth", "XX_FixedItemStat", "XX_HealthImprovementText", "XX_StatImprovementText"] {
            assert_eq!(number(&ui, &format!("{hidden}/visible")), 0.0, "{hidden}");
        }
        assert_eq!(text(&ui, "XX_HealthImprovementText/string"), "", "nothing stays of the item before");
        write(&mut ui, "M/R", "XX_", None);
        assert_eq!(number(&ui, "XX_BrokenItemHealth/visible"), 0.0);
        assert_eq!(number(&ui, "XX_BrokenItemStat/visible"), 0.0);
        assert_eq!(describe(&ui, "M/R", "XX_"), "");
    }

    #[test]
    fn an_item_without_a_figure_has_no_stat_cards() {
        let mut ui = fixture();
        let f = Figures { now: Card { condition: 2.0, stat: None }, after: Some(Card { condition: 1.0, stat: None }), gain_stat: None, ..figures() };
        write(&mut ui, "M/R", "XX_", Some(&f));
        assert_eq!(number(&ui, "XX_BrokenItemStat/visible"), 0.0);
        assert_eq!(number(&ui, "XX_FixedItemStat/visible"), 0.0);
        assert_eq!(number(&ui, "XX_StatImprovementText/visible"), 0.0);
        assert_eq!(number(&ui, "XX_BrokenItemHealth/user0"), 1.0, "held to its range");
    }

    /// [design] The fixture's own places, as `fixture`: a title of `width` at 3, a meter from 40
    /// to 140 beside it.
    fn places(ui: &mut Ui, width: f64) {
        let title = ui.find_tile("M/R/XX_BrokenItemHealth/CI_TitleText").unwrap();
        ui.set_trait(title, "x", Value::Num(3.0));
        ui.set_trait(title, "width", Value::Num(width));
        let meter = ui.find_tile("M/R/XX_BrokenItemHealth/XX_BrokenItemHealthMeter").unwrap();
        ui.set_trait(meter, "x", Value::Num(40.0));
        ui.set_trait(meter, "width", Value::Num(100.0));
    }

    #[test]
    fn a_wide_title_moves_the_meter_and_keeps_its_right_end() {
        let mut ui = fixture();
        places(&mut ui, 60.0);
        fit_meters(&mut ui, "M/R", "XX_");
        let at = |ui: &Ui, name: &str| number(ui, &format!("XX_BrokenItemHealth/XX_BrokenItemHealthMeter/{name}"));
        assert_eq!((at(&ui, "x"), at(&ui, "width")), (73.0, 67.0));
        fit_meters(&mut ui, "M/R", "XX_");
        assert_eq!((at(&ui, "x"), at(&ui, "width")), (73.0, 67.0), "every frame's call changes nothing");
    }

    /// The general values: decimal, negative and worded figures as given, a gain with no condition
    /// gain, and the repair words through `From<&Figures>` unchanged.
    #[test]
    fn card_values_show_any_words_and_the_repair_words_are_unchanged() {
        let mut ui = fixture();
        let tiles = CardTiles::repair("M/R", "XX_");
        let side = |stat: &str| SideShown { condition: 0.5, stat: Some(stat.into()) };
        let values = CardValues { condition_title: "State".into(), stat_title: "Spread".into(), now: side("0.90"), after: Some(side("0.40")), health_gain: None, stat_gain: Some("-0.50".into()) };
        write_values(&mut ui, &tiles, Some(&values));
        assert_eq!(describe_of(&ui, &tiles), "Own state 50%, Spread 0.90, State 50%, Spread 0.40, -0.50");
        assert_eq!(number(&ui, "XX_HealthImprovementText/visible"), 0.0);
        let words = CardValues { now: side(""), after: Some(side("Silent")), stat_gain: None, ..values };
        write_values(&mut ui, &tiles, Some(&words));
        assert_eq!(text(&ui, "XX_FixedItemStat/_Value"), "Silent");
        assert_eq!(number(&ui, "XX_StatImprovementText/visible"), 0.0);
        let repair = CardValues::from(&figures());
        assert_eq!((repair.health_gain.as_deref(), repair.stat_gain.as_deref()), (Some("+50%"), Some("+5")));
        assert_eq!(CardValues::from(&Figures { after: None, ..figures() }).health_gain, None);
    }

    #[test]
    fn a_title_that_fits_or_leaves_no_room_moves_nothing() {
        for width in [20.0, 500.0] {
            let mut ui = fixture();
            places(&mut ui, width);
            fit_meters(&mut ui, "M/R", "XX_");
            assert_eq!(number(&ui, "XX_BrokenItemHealth/XX_BrokenItemHealthMeter/x"), 40.0, "{width}");
            assert_eq!(number(&ui, "XX_BrokenItemHealth/XX_BrokenItemHealthMeter/width"), 100.0, "{width}");
        }
    }
}
