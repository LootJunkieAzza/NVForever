// [design] terrain.wesl evaluates a folded base's stitched surfaces as for its near
// draw, then this endpoint stores linear unlit colour without scorch, lighting, fog or
// exposure, at alpha 1. The blended layers over it, and bases that are not folded, are
// captured by terrain_capture.wesl (terrain_surface.rs `SurfaceCaptureMaterial`).
import bevy_pbr::render::forward_io::FragmentOutput;

fn surface_bake(colour: vec3<f32>) -> FragmentOutput {
    var out: FragmentOutput;
    out.color = vec4(colour, 1.0);
    return out;
}
