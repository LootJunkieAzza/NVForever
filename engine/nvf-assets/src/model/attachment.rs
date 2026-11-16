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
        let Some(o) = by_block.get(&block).filter(|_| seen.insert(block)) else {
            continue;
        };
        let Some(raw) = dec.get(block as i32) else {
            continue;
        };
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
        let at = out.len();
        out.push(ModelObject {
            block,
            name: o.name.clone(),
            parent,
            local: to_bevy(&o.transform),
            flags: raw.int("flags").unwrap_or(0) as u32,
            material,
            alpha,
            stencil,
            shape: shapes.get(&block).copied(),
            rig_node: rig.and_then(|index| index.get(&block).copied()),
        });
        pending.extend(
            o.children
                .iter()
                .rev()
                .filter(|&&c| {
                    by_block
                        .get(&c)
                        .is_some_and(|child| child.parent == Some(block))
                })
                .map(|&c| (c, Some(at))),
        );
    }
    out
}
