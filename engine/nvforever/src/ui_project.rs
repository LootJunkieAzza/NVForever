//! Binds projected UI off-screen textures (`nvf_play::tiles::projected_ui::ProjectedUiTargets`)
//! onto target 3D model surfaces (`MeshMaterial3d<NifMaterial>`) by `(reference, block)`
//! (Part of #2418; docs/spec-notes/held-ui-projection-state.md,
//! docs/spec-notes/local-map-ui-service.md).
//!
//! Zero-work fast path by default: when `ProjectedUiTargets` is empty and no surfaces are
//! bound, this system returns immediately without walking models or touching materials [design].
//!
//! Each matched mesh entity receives its own unlit `NifMaterial` copy displaying the projected
//! `Handle<Image>` (never mutating shared model materials in place). On withdrawal, target
//! removal, or session rollover, the original material handle is restored only if the entity
//! still holds this module's owned material copy (ownership-conditional restoration) [design].

use crate::nif_mirror::{self, MirrorSystems, Rigs, Visuals};
use bevy::prelude::*;
use fnv_game::nif_blocks::ModelView;
use fnv_sim::FormId;
use nvf_assets::actor::ShapeName;
use nvf_assets::NifMaterial;
use nvf_play::tiles::projected_ui::ProjectedUiTargets;
use std::collections::{HashMap, HashSet};

pub struct UiProjectPlugin;

impl Plugin for UiProjectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoundSurfaces>()
            .add_systems(PostUpdate, bind.after(MirrorSystems));
    }
}

struct BoundSurface {
    original: Handle<NifMaterial>,
    owned: Handle<NifMaterial>,
    image: Handle<Image>,
}

/// Active per-entity material overrides owned by [`UiProjectPlugin`].
#[derive(Resource, Default)]
pub struct BoundSurfaces {
    entries: HashMap<Entity, BoundSurface>,
}

impl BoundSurfaces {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

type TreeNodes<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Name>,
        Option<&'static ShapeName>,
        Option<&'static Children>,
        Has<MeshMaterial3d<NifMaterial>>,
    ),
>;

fn matches_block(
    entity: Entity,
    block_lower: &str,
    tree: &TreeNodes,
    rig: &nif_mirror::RigNames,
) -> bool {
    let Ok((name, shape, _, _)) = tree.get(entity) else {
        return false;
    };
    if let Some(shape) = shape {
        if shape.0.eq_ignore_ascii_case(block_lower) {
            return true;
        }
    }
    if let Some(name) = name {
        if name.as_str().eq_ignore_ascii_case(block_lower) {
            return true;
        }
    }
    if let Some(rig_name) = rig.name_of(entity) {
        if rig_name.eq_ignore_ascii_case(block_lower) {
            return true;
        }
    }
    false
}

/// Collect mesh entities under `root` whose own name/ShapeName matches `block_lower`, or that
/// are direct mesh children of a named group node matching `block_lower` [design].
fn matching_meshes(
    root: Entity,
    block_lower: &str,
    tree: &TreeNodes,
    rig: &nif_mirror::RigNames,
    out: &mut Vec<Entity>,
) {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        let Ok((_, _, children, has_mat)) = tree.get(entity) else {
            continue;
        };
        if matches_block(entity, block_lower, tree, rig) {
            if has_mat {
                out.push(entity);
            } else if let Some(children) = children {
                for child in children.iter() {
                    if tree.get(child).is_ok_and(|(_, _, _, child_has_mat)| child_has_mat) {
                        out.push(child);
                    }
                }
            }
        }
        if let Some(children) = children {
            stack.extend(children.iter());
        }
    }
}

fn projected_material(source: &NifMaterial, image: Handle<Image>) -> NifMaterial {
    let mut m = source.clone();
    m.base.base_color = Color::WHITE;
    m.base.base_color_texture = Some(image);
    m.base.emissive_texture = None;
    m.base.emissive = LinearRgba::BLACK;
    m.base.normal_map_texture = None;
    m.base.unlit = true;
    nvf_assets::authored::clear_for_unlit_copy(&mut m);
    m
}

#[allow(clippy::too_many_arguments)]
fn bind(
    targets: Option<Res<ProjectedUiTargets>>,
    rigs: Rigs,
    visuals: Visuals,
    tree: TreeNodes,
    mut mesh_mats: Query<&mut MeshMaterial3d<NifMaterial>>,
    mut materials: Option<ResMut<Assets<NifMaterial>>>,
    mut bound: ResMut<BoundSurfaces>,
) {
    let empty_targets = targets.as_deref().is_none_or(ProjectedUiTargets::is_empty);
    if empty_targets && bound.is_empty() {
        return;
    }
    let Some(materials) = materials.as_deref_mut() else {
        return;
    };

    let mut desired: HashMap<Entity, Handle<Image>> = HashMap::new();
    if let Some(targets) = targets.as_deref() {
        let mut matched = Vec::new();
        for (target, output) in targets.iter() {
            matched.clear();
            let views: &[ModelView] = if target.reference == FormId::PLAYER_REF {
                &[ModelView::FirstPerson, ModelView::ThirdPerson]
            } else {
                &[ModelView::Reference]
            };
            for &view in views {
                let Some(root) = nif_mirror::root_of(target.reference.0, view, &rigs, &visuals) else {
                    continue;
                };
                let rig = nif_mirror::rig_names(target.reference.0, view, &rigs);
                matching_meshes(root, &target.block, &tree, &rig, &mut matched);
            }
            for entity in matched.drain(..) {
                desired.insert(entity, output.image.clone());
            }
        }
    }

    // Restore any bound surface no longer targeted, preserving external material swaps.
    let keep: HashSet<Entity> = desired.keys().copied().collect();
    let stale: Vec<Entity> = bound.entries.keys().copied().filter(|e| !keep.contains(e)).collect();
    for entity in stale {
        if let Some(prev) = bound.entries.remove(&entity) {
            if let Ok(mut mat) = mesh_mats.get_mut(entity) {
                if mat.0 == prev.owned {
                    mat.0 = prev.original;
                }
            }
            materials.remove(&prev.owned);
        }
    }

    // Bind new targets or refresh existing owned materials when the projected image changes.
    for (entity, image) in desired {
        let Ok(mut mat) = mesh_mats.get_mut(entity) else {
            continue;
        };
        if let Some(existing) = bound.entries.get_mut(&entity) {
            if mat.0 == existing.owned {
                if existing.image != image {
                    if let Some(mut owned_asset) = materials.get_mut(&existing.owned) {
                        owned_asset.base.base_color_texture = Some(image.clone());
                    }
                    existing.image = image;
                }
                continue;
            }
            // External swap replaced our owned material: retire the old copy and re-bind over the new one.
            let old_owned = existing.owned.clone();
            materials.remove(&old_owned);
            bound.entries.remove(&entity);
        }

        let original = mat.0.clone();
        let source = materials.get(&original).cloned().unwrap_or_default();
        let owned = materials.add(projected_material(&source, image.clone()));
        mat.0 = owned.clone();
        bound.entries.insert(entity, BoundSurface { original, owned, image });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_game::ui_projection::Target;
    use nvf_play::RefVisual;

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Assets<NifMaterial>>()
            .init_resource::<ProjectedUiTargets>()
            .add_plugins(UiProjectPlugin);
        app
    }

    #[test]
    fn binds_projected_texture_to_named_shape_updates_on_resize_and_restores_on_withdrawal() {
        let mut app = test_app();
        let shared_mat = app.world_mut().resource_mut::<Assets<NifMaterial>>().add(NifMaterial::default());
        let proj_img_1 = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
        let proj_img_2 = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());

        let root = app.world_mut().spawn((RefVisual(0x42), Transform::IDENTITY)).id();
        let screen = app
            .world_mut()
            .spawn((
                ShapeName("screen".into()),
                MeshMaterial3d(shared_mat.clone()),
                ChildOf(root),
            ))
            .id();
        let frame = app
            .world_mut()
            .spawn((
                Name::new("Frame"),
                MeshMaterial3d(shared_mat.clone()),
                ChildOf(root),
            ))
            .id();

        let target = Target { reference: FormId(0x42), block: "screen".into() };
        app.world_mut()
            .resource_mut::<ProjectedUiTargets>()
            .insert_synthetic_image_for_test(target.clone(), proj_img_1.clone(), 64, 64);

        app.update();

        let bound_handle = app.world().get::<MeshMaterial3d<NifMaterial>>(screen).unwrap().0.clone();
        assert_ne!(bound_handle, shared_mat, "screen gets its own material copy");
        assert_eq!(app.world().get::<MeshMaterial3d<NifMaterial>>(frame).unwrap().0, shared_mat, "unrelated sibling shape untouched");
        {
            let mats = app.world().resource::<Assets<NifMaterial>>();
            let m = mats.get(&bound_handle).unwrap();
            assert_eq!(m.base.base_color_texture.as_ref(), Some(&proj_img_1));
            assert!(m.base.unlit);
            assert!(mats.get(&shared_mat).unwrap().base.base_color_texture.is_none(), "shared original material never mutated");
        }

        // Resize updates base_color_texture in place on the same owned material handle.
        app.world_mut()
            .resource_mut::<ProjectedUiTargets>()
            .insert_synthetic_image_for_test(target.clone(), proj_img_2.clone(), 128, 128);
        app.update();
        assert_eq!(app.world().get::<MeshMaterial3d<NifMaterial>>(screen).unwrap().0, bound_handle);
        assert_eq!(
            app.world().resource::<Assets<NifMaterial>>().get(&bound_handle).unwrap().base.base_color_texture.as_ref(),
            Some(&proj_img_2)
        );

        // Withdrawal restores the exact original material and frees the owned copy.
        assert!(app.world_mut().resource_mut::<ProjectedUiTargets>().remove_synthetic_output_for_test(&target));
        app.update();
        assert_eq!(app.world().get::<MeshMaterial3d<NifMaterial>>(screen).unwrap().0, shared_mat);
        assert!(app.world().resource::<Assets<NifMaterial>>().get(&bound_handle).is_none(), "owned material freed");
        assert!(app.world().resource::<BoundSurfaces>().is_empty());
    }

    #[test]
    fn ownership_conditional_restore_never_clobbers_external_material_swap() {
        let mut app = test_app();
        let orig_mat = app.world_mut().resource_mut::<Assets<NifMaterial>>().add(NifMaterial::default());
        let foreign_mat = app.world_mut().resource_mut::<Assets<NifMaterial>>().add(NifMaterial::default());
        let proj_img = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());

        let root = app.world_mut().spawn((RefVisual(0x55), Transform::IDENTITY)).id();
        let screen = app
            .world_mut()
            .spawn((Name::new("Screen"), MeshMaterial3d(orig_mat.clone()), ChildOf(root)))
            .id();

        let target = Target { reference: FormId(0x55), block: "screen".into() };
        app.world_mut()
            .resource_mut::<ProjectedUiTargets>()
            .insert_synthetic_image_for_test(target.clone(), proj_img, 64, 64);
        app.update();
        let owned = app.world().get::<MeshMaterial3d<NifMaterial>>(screen).unwrap().0.clone();
        assert_ne!(owned, orig_mat);

        // External system swaps the shape's material, then projection withdraws.
        app.world_mut().get_mut::<MeshMaterial3d<NifMaterial>>(screen).unwrap().0 = foreign_mat.clone();
        app.world_mut().resource_mut::<ProjectedUiTargets>().remove_synthetic_output_for_test(&target);
        app.update();

        assert_eq!(app.world().get::<MeshMaterial3d<NifMaterial>>(screen).unwrap().0, foreign_mat, "external material swap preserved");
        assert!(app.world().resource::<Assets<NifMaterial>>().get(&owned).is_none(), "owned copy freed cleanly");
    }

    #[test]
    fn empty_projections_are_zero_work_and_missing_block_leaves_model_unchanged() {
        let mut app = test_app();
        let orig_mat = app.world_mut().resource_mut::<Assets<NifMaterial>>().add(NifMaterial::default());
        let proj_img = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());

        let root = app.world_mut().spawn((RefVisual(0x77), Transform::IDENTITY)).id();
        let shape = app
            .world_mut()
            .spawn((Name::new("OtherBlock"), MeshMaterial3d(orig_mat.clone()), ChildOf(root)))
            .id();

        // Empty targets -> no-op.
        app.update();
        assert!(app.world().resource::<BoundSurfaces>().is_empty());
        assert_eq!(app.world().get::<MeshMaterial3d<NifMaterial>>(shape).unwrap().0, orig_mat);

        // Non-matching block name -> leaves model completely untouched.
        let target = Target { reference: FormId(0x77), block: "missingblock".into() };
        app.world_mut()
            .resource_mut::<ProjectedUiTargets>()
            .insert_synthetic_image_for_test(target, proj_img, 32, 32);
        app.update();
        assert!(app.world().resource::<BoundSurfaces>().is_empty());
        assert_eq!(app.world().get::<MeshMaterial3d<NifMaterial>>(shape).unwrap().0, orig_mat);
    }
}
