use super::*;

const PISTOL: FormId = FormId(0x0000_0E01);
const STAT: FormId = FormId(0x0000_0E02);

fn pistol() -> Weapon {
    let mut w = Weapon { edid: None, model: Some("plain.nif".into()), anim_type: None, damage: 0, projectile: None, explosion_damage: 0.0, skill: None, embedded: false, non_playable: false, not_for_npcs: false, first_person: Some(STAT), first_person_model: Some("1st.nif".into()), mod_models: Default::default(), mod_first_person: [None; 7], mod_first_person_models: Default::default(), held_world_model: None, grip: None, hide_backpack: false, addons: Vec::new() };
    w.mod_models[2] = Some("ext_scope.nif".into());
    w.mod_first_person[2] = Some(STAT);
    w.mod_first_person_models[2] = Some("1st_ext_scope.nif".into());
    w
}

fn forms() -> ActorForms {
    let mut f = ActorForms::default();
    f.weapons.insert(PISTOL, pistol());
    f.alt_textures.insert((STAT, *b"MODL"), Vec::new());
    f.alt_textures.insert((PISTOL, *b"MODL"), Vec::new());
    f
}

/// WAP's way (gr_10mmP.txt): every world model set, every first-person form cleared.
fn wap() -> WeaponModelChange {
    let mut c = WeaponModelChange::default();
    for k in 0..8 {
        c.world[k] = Some(Some(format!("new{k}.nif")));
        c.first_person[k] = Some((None, None));
    }
    c
}

#[test]
fn cleared_first_person_forms_draw_the_new_world_models_in_both_views() {
    let mut f = forms();
    let mut sync = WeaponModelSync::default();
    sync.accept(BTreeMap::from([(PISTOL, wap())]), &mut f);
    let w = &f.weapons[&PISTOL];
    assert_eq!(w.model_for(false, 0), Some("new0.nif"));
    assert_eq!(w.model_for(true, 0), Some("new0.nif"));
    assert_eq!(w.model_for(true, 3), Some("new3.nif"));
    assert_eq!(w.model_for(false, 7), Some("new7.nif"));
    // The record's textures were made for the record's models.
    assert!(f.weapon_alt_textures(PISTOL, true, 0).is_empty());
    assert!(!f.alt_textures.contains_key(&(PISTOL, *b"MODL")));
    assert!(!f.alt_textures.contains_key(&(STAT, *b"MODL")));
}

#[test]
fn a_change_that_goes_puts_the_record_back() {
    let mut f = forms();
    let mut sync = WeaponModelSync::default();
    sync.accept(BTreeMap::from([(PISTOL, wap())]), &mut f);
    sync.accept(BTreeMap::new(), &mut f);
    assert_eq!(f.weapons[&PISTOL], pistol());
    assert!(f.alt_textures.contains_key(&(PISTOL, *b"MODL")));
    assert!(f.alt_textures.contains_key(&(STAT, *b"MODL")));
}

#[test]
fn only_the_slots_set_change() {
    let mut f = forms();
    let mut c = WeaponModelChange::default();
    c.world[1] = Some(Some("ext.nif".into()));
    let mut sync = WeaponModelSync::default();
    sync.accept(BTreeMap::from([(PISTOL, c)]), &mut f);
    let w = &f.weapons[&PISTOL];
    assert_eq!(w.model_for(false, 0), Some("plain.nif"));
    assert_eq!(w.model_for(false, 1), Some("ext.nif"));
    assert_eq!(w.model_for(true, 0), Some("1st.nif"));
    assert_eq!(w.model_for(true, 3), Some("1st_ext_scope.nif"));
    // The world model is the record's, so are its textures.
    assert!(f.alt_textures.contains_key(&(PISTOL, *b"MODL")));
}
