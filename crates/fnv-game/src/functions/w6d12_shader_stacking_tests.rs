use super::ScriptLooks;
use fnv_sim::FormId;

#[test]
fn non_hologram_playmagicshadervisuals_stacks_duplicate_instances_in_order() {
    let mut l = ScriptLooks::default();
    let actor = FormId(0x14);
    let sh_a = FormId(0x144);
    let sh_b = FormId(0x145);
    let no_holo = |_: FormId| None;
    assert!(l.play(actor, sh_a, None, no_holo));
    assert_eq!(l.shaders.iter().filter(|p| p.on == actor && p.shader == sh_a).count(), 1);
    assert!(l.play(actor, sh_a, None, no_holo));
    assert_eq!(l.shaders.iter().filter(|p| p.on == actor && p.shader == sh_a).count(), 2);
    assert!(l.play(actor, sh_b, None, no_holo));
    let list: Vec<FormId> = l.shaders.iter().filter(|p| p.on == actor).map(|p| p.shader).collect();
    assert_eq!(list, vec![sh_a, sh_a, sh_b]);
}
