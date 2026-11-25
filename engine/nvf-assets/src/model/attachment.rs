//! Same-decode source identities for attached models; no second VFS/name join.
use super::*;

#[derive(Debug, Clone)]
pub struct ModelObject {
    pub block: usize,
    pub name: Option<String>,
    pub parent: Option<usize>,
    pub local: Transform,
    pub flags: u32,
    pub material: Option<[f32; 9]>,
    pub alpha: Option<[f32; 2]>,
    pub stencil: Option<[f32; 3]>,
    pub shape: Option<usize>,
    pub rig_node: Option<usize>,
}

pub(super) fn objects(
    objects: &[SceneObject],
    dec: &Decoded,
    shapes: &HashMap<usize, usize>,
    rig: Option<&HashMap<usize, usize>>,
) -> Vec<ModelObject> {
    let by_block: HashMap<_, _> = objects.iter().map(|o| (o.block, o)).collect();
    let mut pending: Vec<_> = objects
        .iter()
        .filter(|o| o.parent.is_none())
        .map(|o| (o.block, None))
        .rev()
        .collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    while let Some((block, parent)) = pending.pop() {
        let Some(raw) = dec.get(block as i32) else {
            continue;
        };
        let scene_obj = by_block.get(&block).copied();
        let is_particle_sys = scene_obj.is_none()
            && (dec.is_a(block, "NiParticleSystem") || dec.is_a(block, "BSStripParticleSystem"));
        if scene_obj.is_none() && !is_particle_sys {
            continue;
        }
        if !seen.insert(block) {
            continue;
        }
        let mut material = None;
        let mut alpha = None;
        let mut stencil = None;
        for &p in raw.refs("properties").unwrap_or_default() {
            let Some(value) = dec.get(p) else { continue };
            if p >= 0 && dec.is_a(p as usize, "NiMaterialProperty") {
                let color = |key| {
                    value
                        .floats(key)
                        .filter(|v| v.len() >= 3)
                        .map_or([0.; 3], |v| [v[0], v[1], v[2]])
                };
                let (s, e) = (color("specular_color"), color("emissive_color"));
                material = Some([
                    s[0],
                    s[1],
                    s[2],
                    e[0],
                    e[1],
                    e[2],
                    value.float("glossiness").unwrap_or(0.),
                    value.float("alpha").unwrap_or(1.),
                    value.float("emissive_mult").unwrap_or(1.),
                ]);
            } else if p >= 0 && dec.is_a(p as usize, "NiAlphaProperty") {
                alpha = Some([
                    value.int("flags").unwrap_or(0) as f32,
                    value.int("threshold").unwrap_or(0) as f32,
                ]);
            } else if p >= 0 && dec.is_a(p as usize, "NiStencilProperty") {
                stencil = Some([
                    value.int("flags").unwrap_or(0) as f32,
                    value
                        .int("stencil_ref")
                        .or_else(|| value.int("stencil_reference"))
                        .unwrap_or(0) as f32,
                    value.int("stencil_mask").unwrap_or(0) as f32,
                ]);
            }
        }
        let (name, local) = match scene_obj {
            Some(o) => (o.name.clone(), to_bevy(&o.transform)),
            None => (
                raw.text("name").map(str::to_string),
                to_bevy(&fnv_nif::scene::Transform::from_struct(raw).unwrap_or(fnv_nif::scene::Transform::IDENTITY)),
            ),
        };
        let at = out.len();
        out.push(ModelObject {
            block,
            name,
            parent,
            local,
            flags: raw.int("flags").unwrap_or(0) as u32,
            material,
            alpha,
            stencil,
            shape: shapes.get(&block).copied(),
            rig_node: rig.and_then(|index| index.get(&block).copied()),
        });
        if let Some(o) = scene_obj {
            // For non-rig attachments, also include direct particle-system child blocks
            // (`NiParticleSystem` / `BSStripParticleSystem`) omitted from `scene_objects`
            // so script particle queries/setters can address them by block name [design].
            let mut children: Vec<usize> = o
                .children
                .iter()
                .copied()
                .filter(|&c| {
                    by_block
                        .get(&c)
                        .is_some_and(|child| child.parent == Some(block))
                })
                .collect();
            if rig.is_none() {
                for &c in raw.refs("children").unwrap_or_default() {
                    if let Ok(c) = usize::try_from(c) {
                        if !by_block.contains_key(&c)
                            && (dec.is_a(c, "NiParticleSystem")
                                || dec.is_a(c, "BSStripParticleSystem"))
                            && !children.contains(&c)
                        {
                            children.push(c);
                        }
                    }
                }
            }
            pending.extend(children.into_iter().rev().map(|c| (c, Some(at))));
        }
    }
    out
}
