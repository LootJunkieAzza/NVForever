// Soft edges of alpha-tested shapes in a multisampled view, written by the depth prepass
// (nif_material.rs `soft_edges`, "Prepass coverage").
//
// A cut-out's edge is anti-aliased by covering only a share of the pixel's samples where
// the alpha falls below the cutoff. The prepass fragment returns that share as a sample
// mask, so the depth buffer holds the shape on exactly the samples it covers, and the
// main pass draws it with a depth test for equality: no alpha test there, no alpha to
// coverage, and nothing hidden is lit.

// The share of a pixel's samples an alpha-tested fragment covers: 1 at the cutoff and
// above, falling to 0 within a pixel outside it.
fn edge_share(alpha: f32, cutoff: f32) -> f32 {
    return saturate((alpha - cutoff) / max(fwidth(alpha), 0.0001) + 1.0);
}

// The samples, as a sample mask, of the pixel at `frag` that a fragment covering `share`
// of it takes: share x count of them, rounded. Which ones: taken in an order that puts
// each next sample far from those before it (the sample positions of the standard
// patterns), starting at a sample that changes from pixel to pixel, so neighbouring
// pixels at the same share do not all keep the same corner [chosen].
fn sample_mask(share: f32, frag: vec2<f32>) -> u32 {
    // The order, a sample index in each 4 bits, lowest first.
    @if(COVERAGE_SAMPLES_2) let count = 2u;
    @if(COVERAGE_SAMPLES_2) let order = 0x10u;
    @if(COVERAGE_SAMPLES_4) let count = 4u;
    @if(COVERAGE_SAMPLES_4) let order = 0x2130u;
    @if(COVERAGE_SAMPLES_8) let count = 8u;
    @if(COVERAGE_SAMPLES_8) let order = 0x52614370u;
    let n = u32(clamp(share * f32(count) + 0.5, 0.0, f32(count)));
    let start = (u32(frag.x) + 2u * u32(frag.y)) % count;
    var mask = 0u;
    for (var i = 0u; i < n; i += 1u) {
        mask |= 1u << ((order >> (4u * ((start + i) % count))) & 15u);
    }
    return mask;
}
