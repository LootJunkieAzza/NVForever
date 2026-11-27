// The sun contact shadows' clear (nvforever sun_contact_shadows.rs): 1, "the sun reaches
// every texel", written when no march pipeline could draw this frame (still compiling, a
// multisampled view, no depth yet). A shader of its own on purpose: a march shader that
// fails to compile must not leave the texture black, which would darken the whole sun.
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@fragment
fn clear(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0);
}