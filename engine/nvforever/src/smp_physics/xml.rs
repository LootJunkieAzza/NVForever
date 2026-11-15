//! The SMP physics XML (docs/spec-notes/smp-physics.md §2): a small reader for the
//! element tree, and the system it describes: bones with their templates
//! (`bone-default`), generic constraints with theirs (`generic-constraint-default`), the
//! collision shapes and NV SMP's `nv-virtual-legs`.
//!
//! Names match case-insensitively [documented: NV SMP Physics README, "Names match
//! case-insensitively"]; they are kept lower case here.

use bevy::math::Vec3;
use std::collections::HashMap;

/// An element: its name (lower case), attributes (names lower case), text and children.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Element>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn number(&self) -> Option<f32> {
        self.text.trim().parse().ok()
    }

    fn vec3(&self) -> Vec3 {
        let f = |n: &str| self.attr(n).and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
        Vec3::new(f("x"), f("y"), f("z"))
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Parse an XML document into its root element. Comments, the declaration and
/// processing instructions are skipped; `None` for a document with no element or a tag
/// left open.
pub fn parse(src: &str) -> Option<Element> {
    let mut stack: Vec<Element> = vec![Element::default()];
    let mut rest = src;
    while let Some(lt) = rest.find('<') {
        let text = &rest[..lt];
        if let Some(top) = stack.last_mut() {
            if !text.trim().is_empty() {
                top.text.push_str(&unescape(text));
            }
        }
        rest = &rest[lt..];
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after.find("-->")?;
            rest = &after[end + 3..];
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest.find('>')?;
            rest = &rest[end + 1..];
            continue;
        }
        let end = rest.find('>')?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            let done = stack.pop()?;
            if done.name != name {
                return None;
            }
            stack.last_mut()?.children.push(done);
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let (name, mut attrs_src) = match tag.find(char::is_whitespace) {
            Some(i) => (&tag[..i], &tag[i..]),
            None => (tag, ""),
        };
        let mut el = Element { name: name.to_ascii_lowercase(), ..Default::default() };
        loop {
            attrs_src = attrs_src.trim_start();
            let Some(eq) = attrs_src.find('=') else { break };
            let key = attrs_src[..eq].trim().to_ascii_lowercase();
            let after = attrs_src[eq + 1..].trim_start();
            let quote = after.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let close = after[1..].find(quote)?;
            el.attrs.push((key, unescape(&after[1..1 + close])));
            attrs_src = &after[close + 2..];
        }
        if self_closing {
            stack.last_mut()?.children.push(el);
        } else {
            stack.push(el);
        }
    }
    if stack.len() != 1 {
        return None;
    }
    stack.pop()?.children.into_iter().next()
}

/// A bone's physical values: FSMP's bone properties, those this solver uses
/// (docs/spec-notes/smp-physics.md §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoneProps {
    /// 0: animation driven (kinematic).
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub wind_factor: f32,
    pub friction: f32,
}

impl Default for BoneProps {
    fn default() -> Self {
        BoneProps { mass: 0.0, linear_damping: 0.0, angular_damping: 0.0, wind_factor: 1.0, friction: 0.5 }
    }
}

impl BoneProps {
    fn apply(&mut self, el: &Element) {
        for c in &el.children {
            let Some(v) = c.number() else { continue };
            match c.name.as_str() {
                "mass" => self.mass = v,
                "lineardamping" => self.linear_damping = v,
                "angulardamping" => self.angular_damping = v,
                "wind-factor" => self.wind_factor = v,
                "friction" => self.friction = v,
                _ => {}
            }
        }
    }
}

/// A generic (6-DOF) constraint's values (docs/spec-notes/smp-physics.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ConstraintProps {
    pub linear_lower: Vec3,
    pub linear_upper: Vec3,
    pub angular_lower: Vec3,
    pub angular_upper: Vec3,
    pub linear_stiffness: Vec3,
    pub angular_stiffness: Vec3,
    pub linear_damping: Vec3,
    pub angular_damping: Vec3,
}

impl ConstraintProps {
    fn apply(&mut self, el: &Element) {
        for c in &el.children {
            let v = c.vec3();
            match c.name.as_str() {
                "linearlowerlimit" => self.linear_lower = v,
                "linearupperlimit" => self.linear_upper = v,
                "angularlowerlimit" => self.angular_lower = v,
                "angularupperlimit" => self.angular_upper = v,
                "linearstiffness" => self.linear_stiffness = v,
                "angularstiffness" => self.angular_stiffness = v,
                "lineardamping" => self.linear_damping = v,
                "angulardamping" => self.angular_damping = v,
                _ => {}
            }
        }
    }

    /// The constraint holds its bodies at one point: no linear freedom.
    pub fn is_pinned(&self) -> bool {
        (self.linear_upper - self.linear_lower).abs().max_element() < 1e-4
    }

    /// The widest swing either way about x or y, radians (the twist axis z left out).
    pub fn swing(&self) -> f32 {
        self.angular_lower.x.abs().max(self.angular_upper.x.abs()).max(self.angular_lower.y.abs()).max(self.angular_upper.y.abs())
    }

    /// How far the bodies may move apart or together, the widest axis.
    pub fn slack(&self) -> f32 {
        self.linear_lower.abs().max(self.linear_upper.abs()).max_element()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoneDef {
    pub name: String,
    pub props: BoneProps,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintDef {
    pub a: String,
    pub b: String,
    pub props: ConstraintProps,
}

/// A collision shape: `per-vertex-shape` or `per-triangle-shape`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeDef {
    pub name: String,
    pub margin: f32,
    pub tags: Vec<String>,
    pub no_collide: Vec<String>,
    /// The bones named in its `weight-threshold`s.
    pub bones: Vec<String>,
}

/// An SMP system as its XML gives it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SystemDef {
    pub bones: Vec<BoneDef>,
    pub constraints: Vec<ConstraintDef>,
    pub shapes: Vec<ShapeDef>,
    /// `nv-virtual-legs`: the scale of the fitted leg capsules.
    pub virtual_legs: Option<f32>,
    /// Elements of kinds this reader does not handle, with their counts (for the log).
    pub unknown: Vec<(String, usize)>,
}

impl SystemDef {
    /// The bone named `name` (lower case), if the XML lists it.
    pub fn bone(&self, name: &str) -> Option<&BoneDef> {
        self.bones.iter().find(|b| b.name == name)
    }
}

/// The system an SMP XML describes; `None` when it is no XML or its root is not
/// `<system>`.
pub fn system(src: &str) -> Option<SystemDef> {
    let root = parse(src)?;
    if root.name != "system" {
        return None;
    }
    let mut bone_defaults: HashMap<String, BoneProps> = HashMap::new();
    let mut constraint_defaults: HashMap<String, ConstraintProps> = HashMap::new();
    let mut out = SystemDef::default();
    let mut unknown: HashMap<String, usize> = HashMap::new();
    let lower = |s: &str| s.trim().to_ascii_lowercase();
    let constraint = |el: &Element, out: &mut SystemDef, defaults: &HashMap<String, ConstraintProps>| {
        let (Some(a), Some(b)) = (el.attr("bodya"), el.attr("bodyb")) else { return };
        let mut props = el.attr("template").and_then(|t| defaults.get(&lower(t)).copied()).unwrap_or_default();
        props.apply(el);
        out.constraints.push(ConstraintDef { a: lower(a), b: lower(b), props });
    };
    for el in &root.children {
        match el.name.as_str() {
            "bone-default" => {
                let mut p = el.attr("extends").and_then(|t| bone_defaults.get(&lower(t)).copied()).unwrap_or_default();
                p.apply(el);
                bone_defaults.insert(lower(el.attr("name").unwrap_or("")), p);
            }
            "bone" => {
                let Some(name) = el.attr("name") else { continue };
                let mut props = el.attr("template").and_then(|t| bone_defaults.get(&lower(t)).copied()).unwrap_or_else(|| bone_defaults.get("").copied().unwrap_or_default());
                props.apply(el);
                out.bones.push(BoneDef { name: lower(name), props });
            }
            "generic-constraint-default" => {
                let mut p = el.attr("extends").and_then(|t| constraint_defaults.get(&lower(t)).copied()).unwrap_or_default();
                p.apply(el);
                constraint_defaults.insert(lower(el.attr("name").unwrap_or("")), p);
            }
            "generic-constraint" => constraint(el, &mut out, &constraint_defaults),
            "constraint-group" => {
                for c in &el.children {
                    if c.name == "generic-constraint" {
                        constraint(c, &mut out, &constraint_defaults);
                    } else {
                        *unknown.entry(c.name.clone()).or_default() += 1;
                    }
                }
            }
            "per-vertex-shape" | "per-triangle-shape" => {
                let mut s = ShapeDef { name: lower(el.attr("name").unwrap_or("")), margin: 1.0, ..Default::default() };
                for c in &el.children {
                    match c.name.as_str() {
                        "margin" => s.margin = c.number().unwrap_or(1.0),
                        "tag" => s.tags.push(lower(&c.text)),
                        "no-collide-with-tag" => s.no_collide.push(lower(&c.text)),
                        "weight-threshold" => s.bones.extend(c.attr("bone").map(lower)),
                        _ => {}
                    }
                }
                out.shapes.push(s);
            }
            "nv-virtual-legs" => out.virtual_legs = Some(el.attr("scale").and_then(|v| v.trim().parse().ok()).unwrap_or(1.0)),
            other => *unknown.entry(other.to_string()).or_default() += 1,
        }
    }
    let mut unknown: Vec<(String, usize)> = unknown.into_iter().collect();
    unknown.sort();
    out.unknown = unknown;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- a comment with <tags> -->
<system>
	<bone name="Bip01 Pelvis"/>
	<bone-default name="anchor"><restitution>1</restitution></bone-default>
	<bone-default name="cloth">
		<mass>0.02</mass>
		<linearDamping>0.6</linearDamping>
		<wind-factor>1</wind-factor>
	</bone-default>
	<bone name="SMP Coat C00 R0" template="anchor"/>
	<bone name="SMP Coat C00 R1" template="cloth"><mass>0.05</mass></bone>
	<generic-constraint-default name="warp">
		<frameInB/>
		<linearLowerLimit x="0" y="0" z="0"/>
		<linearUpperLimit x="0" y="0" z="0"/>
		<angularLowerLimit x="-0.35" y="-0.3" z="-0.1"/>
		<angularUpperLimit x="0.35" y="0.3" z="0.1"/>
		<angularStiffness x="2" y="2" z="2"/>
	</generic-constraint-default>
	<constraint-group>
		<generic-constraint bodyA="SMP Coat C00 R1" bodyB="SMP Coat C00 R0" template="warp"/>
	</constraint-group>
	<per-vertex-shape name="UpperBody:Coat">
		<margin>2.5</margin>
		<tag>cloth</tag>
		<no-collide-with-tag>cloth</no-collide-with-tag>
		<weight-threshold bone="SMP Coat C00 R1">1</weight-threshold>
	</per-vertex-shape>
	<nv-virtual-legs fit="Legs" scale="0.9"/>
</system>"#;

    #[test]
    fn reads_bones_templates_constraints_and_shapes() {
        let s = system(SAMPLE).expect("system");
        assert_eq!(s.bones.len(), 3);
        assert_eq!(s.bones[0].name, "bip01 pelvis");
        assert_eq!(s.bones[0].props.mass, 0.0);
        assert_eq!(s.bone("smp coat c00 r0").unwrap().props.mass, 0.0);
        let r1 = s.bone("smp coat c00 r1").unwrap().props;
        assert_eq!((r1.mass, r1.linear_damping, r1.wind_factor), (0.05, 0.6, 1.0));
        assert_eq!(s.constraints.len(), 1);
        let c = &s.constraints[0];
        assert_eq!((c.a.as_str(), c.b.as_str()), ("smp coat c00 r1", "smp coat c00 r0"));
        assert!(c.props.is_pinned());
        assert!((c.props.swing() - 0.35).abs() < 1e-6);
        assert_eq!(c.props.angular_stiffness, Vec3::splat(2.0));
        assert_eq!(s.shapes[0].margin, 2.5);
        assert_eq!(s.shapes[0].bones, vec!["smp coat c00 r1".to_string()]);
        assert_eq!(s.shapes[0].no_collide, vec!["cloth".to_string()]);
        assert_eq!(s.virtual_legs, Some(0.9));
        assert!(s.unknown.is_empty());
    }

    #[test]
    fn a_tag_left_open_or_another_root_is_refused() {
        assert!(parse("<system><bone name='a'></system>").is_none());
        assert!(system("<other/>").is_none());
        assert!(system("not xml").is_none());
    }

    #[test]
    fn entities_and_single_quotes() {
        let e = parse("<a b='x &amp; y'>1 &lt; 2</a>").unwrap();
        assert_eq!(e.attr("b"), Some("x & y"));
        assert_eq!(e.text, "1 < 2");
    }
}
