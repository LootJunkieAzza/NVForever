//! Compute passes for FSR 3.1 frame generation (optical flow and frame interpolation).
//!
//! NVForever addition (2026-11-06). The upscaler's passes bind their resources in a fixed
//! order; AMD's frame-generation shaders number their bindings with gaps and declare their
//! sampler at binding 1000, so these passes are described by name instead: each pass has a
//! table of `(binding, GLSL variable)` pairs copied from its `.glsl` file's `#define`s, and
//! the kind of each variable (sampled, storage, buffer, uniform, sampler) comes from the
//! effect's `*_callbacks_glsl.h`. Tests compare the tables with those sources and with the
//! bindings the compiled SPIR-V declares.

use std::{borrow::Cow, num::NonZeroU64};

/// wgpu caps binding numbers below `max_bindings_per_bind_group` (1000 by default), and the
/// frame-interpolation shaders put `s_LinearClamp` at binding 1000. The SPIR-V's binding
/// decoration is rewritten to this number when the module is created.
pub(crate) const SAMPLER_BINDING: u32 = 31;
const SHADER_SAMPLER_BINDING: u32 = 1000;

/// What a GLSL resource is, as the bind group layout needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `texture2D` (float, filterable), `utexture2D`, `itexture2D` or a depth texture
    /// read through `texture2D` (unfilterable float).
    Sampled(wgpu::TextureSampleType),
    /// `image2D`/`uimage2D`/`iimage2D`: read-write storage of this format.
    Storage(wgpu::TextureFormat),
    /// A `buffer` block.
    StorageBuffer { read_only: bool },
    /// A `std140` uniform block.
    Uniform,
    /// `s_LinearClamp`.
    LinearSampler,
}

impl Kind {
    fn layout(self, binding: u32) -> wgpu::BindGroupLayoutEntry {
        let ty = match self {
            Kind::Sampled(sample_type) => wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            Kind::Storage(format) => wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::ReadWrite,
                format,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            Kind::StorageBuffer { read_only } => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            Kind::Uniform => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            Kind::LinearSampler => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        };
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        }
    }
}

/// A resource bound to one binding for one dispatch.
pub(crate) enum Res<'a> {
    /// The whole texture (every mip level).
    Texture(&'a wgpu::Texture),
    /// One mip level of a texture (clamped to its last, as AMD's Vulkan backend does for
    /// the inpainting pyramid's thirteen bindings).
    Mip(&'a wgpu::Texture, u32),
    Buffer(&'a wgpu::Buffer),
    /// A uniform slot: offset and size inside a buffer of several.
    Slot(&'a wgpu::Buffer, u64, u64),
    Sampler(&'a wgpu::Sampler),
}

/// The whole of `texture`, for a binding.
pub(crate) fn whole(texture: &wgpu::Texture) -> Res<'_> {
    Res::Texture(texture)
}

// NVForever (2026-11-10): the bind groups of a view's passes, kept from frame to frame. The
// passes' resources only alternate between a few sets (the ping-ponged textures, the frame's
// colour), and making a bind group and its texture views for every pass of every frame cost
// the render thread time for nothing.
/// What one binding of a kept bind group holds.
#[derive(PartialEq, Eq)]
enum Bound {
    Texture(wgpu::Texture, Option<u32>),
    Buffer(wgpu::Buffer, u64, u64),
    Sampler(wgpu::Sampler),
}

impl Bound {
    fn of(resource: &Res) -> Self {
        match resource {
            Res::Texture(texture) => Bound::Texture((*texture).clone(), None),
            Res::Mip(texture, level) => Bound::Texture((*texture).clone(), Some(*level)),
            Res::Buffer(buffer) => Bound::Buffer((*buffer).clone(), 0, 0),
            Res::Slot(buffer, offset, size) => Bound::Buffer((*buffer).clone(), *offset, *size),
            Res::Sampler(sampler) => Bound::Sampler((*sampler).clone()),
        }
    }

    fn is(&self, resource: &Res) -> bool {
        match (self, resource) {
            (Bound::Texture(kept, None), Res::Texture(texture)) => kept == *texture,
            (Bound::Texture(kept, Some(mip)), Res::Mip(texture, level)) => kept == *texture && mip == level,
            (Bound::Buffer(kept, 0, 0), Res::Buffer(buffer)) => kept == *buffer,
            (Bound::Buffer(kept, at, len), Res::Slot(buffer, offset, size)) => {
                *len != 0 && kept == *buffer && at == offset && len == size
            }
            (Bound::Sampler(kept), Res::Sampler(sampler)) => kept == *sampler,
            _ => false,
        }
    }
}

/// A view's kept bind groups, by pass and resources. It lives in the view (not the pass), so
/// a view that is dropped (a resize) lets go of its textures.
#[derive(Default)]
pub(crate) struct BindGroups(std::sync::Mutex<crate::kept::Kept<KeptKey, wgpu::BindGroup>>);

/// The pass's label and what its bindings hold.
type KeptKey = (&'static str, Vec<Bound>);

impl BindGroups {
    /// A dispatch (or prepare) of the view begins: bind groups not asked for in a while are
    /// dropped (`kept.rs`), with the textures they hold.
    pub fn next_frame(&self) {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).next_frame();
    }
}

/// More sets than the passes alternate between (about 60): past this the list starts again,
/// so a caller that hands over new textures every frame does not grow it without end.
/// Entries no longer asked for go sooner (`kept.rs`).
const KEPT_BIND_GROUPS: usize = 256;

pub(crate) struct FgPass {
    label: &'static str,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    /// The table's entries the compiled shader declares, as `(binding, GLSL name)`.
    active: Vec<(u32, &'static str)>,
}

impl FgPass {
    /// `table` lists `(binding, GLSL variable)` as the pass's `.glsl` defines them; `kind`
    /// gives each variable's type.
    pub fn new(
        device: &wgpu::Device,
        label: &'static str,
        spirv: &[u8],
        table: &[(u32, &'static str)],
        kind: &dyn Fn(&str) -> Kind,
    ) -> Self {
        let mut words = spirv::words(spirv);
        spirv::remap_binding(&mut words, SHADER_SAMPLER_BINDING, SAMPLER_BINDING);
        let declared = spirv::bindings(&words);
        let workgroup_size = spirv::local_size(&words);

        let mut active: Vec<(u32, &'static str)> = table
            .iter()
            .copied()
            .filter(|(binding, _)| declared.contains(binding))
            .collect();
        if declared.contains(&SAMPLER_BINDING) {
            active.push((SAMPLER_BINDING, "s_LinearClamp"));
        }
        active.sort_unstable();
        for binding in &declared {
            assert!(
                active.iter().any(|(b, _)| b == binding),
                "{label}: the shader declares binding {binding}, which the pass table lacks"
            );
        }
        // wgpu-hal's Vulkan backend numbers a bind group layout's descriptors 0, 1, 2... in
        // entry order (`create_bind_group_layout`, its `binding_map`) and rewrites only the
        // shaders it translates itself. Passthrough SPIR-V keeps its own numbers, so they are
        // made dense here, in the same order as the layout's entries. (The upscaler's passes
        // already bind 0..n without gaps.)
        let dense: Vec<(u32, u32)> = active.iter().enumerate().map(|(i, (b, _))| (*b, i as u32)).collect();
        spirv::remap_bindings(&mut words, &dense);

        let shader_module = if device.features().contains(wgpu::Features::PASSTHROUGH_SHADERS) {
            unsafe {
                device.create_shader_module_passthrough(wgpu::ShaderModuleDescriptorPassthrough {
                    label: Some(label),
                    entry_points: Cow::Owned(vec![wgpu::PassthroughShaderEntryPoint {
                        name: Cow::Borrowed("main"),
                        workgroup_size,
                    }]),
                    spirv: Some(Cow::Borrowed(&words)),
                    ..Default::default()
                })
            }
        } else {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::SpirV(Cow::Borrowed(&words)),
            })
        };

        let entries: Vec<_> = active.iter().enumerate().map(|(i, &(_, name))| kind(name).layout(i as u32)).collect();
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self { label, pipeline, bgl, active }
    }

    /// Bind what `resolve` returns for each GLSL variable the shader declares and dispatch.
    pub fn dispatch<'a>(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::ComputePass<'_>,
        kept: &BindGroups,
        resolve: &mut dyn FnMut(&str) -> Res<'a>,
        groups: [u32; 3],
    ) {
        let resources: Vec<Res<'a>> = self.active.iter().map(|&(_, name)| resolve(name)).collect();
        let mut kept = kept.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let found = kept.find(|(label, bound)| {
            *label == self.label && bound.len() == resources.len() && bound.iter().zip(&resources).all(|(b, r)| b.is(r))
        });
        let at = found.unwrap_or_else(|| {
            let views: Vec<Option<wgpu::TextureView>> = resources
                .iter()
                .map(|resource| match resource {
                    Res::Texture(texture) => Some(view(texture)),
                    Res::Mip(texture, level) => Some(mip_view(texture, *level)),
                    _ => None,
                })
                .collect();
            let entries: Vec<_> = resources
                .iter()
                .zip(&views)
                .enumerate()
                .map(|(binding, (resource, view))| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: match (resource, view) {
                        (Res::Buffer(buffer), _) => buffer.as_entire_binding(),
                        (Res::Slot(buffer, offset, size), _) => wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer,
                            offset: *offset,
                            size: NonZeroU64::new(*size),
                        }),
                        (Res::Sampler(sampler), _) => wgpu::BindingResource::Sampler(sampler),
                        (_, Some(view)) => wgpu::BindingResource::TextureView(view),
                        (_, None) => unreachable!("a texture's view was made above"),
                    },
                })
                .collect();
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(self.label),
                layout: &self.bgl,
                entries: &entries,
            });
            kept.push(KEPT_BIND_GROUPS, (self.label, resources.iter().map(Bound::of).collect()), bind_group)
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, kept.value(at), &[]);
        pass.dispatch_workgroups(groups[0].max(1), groups[1].max(1), groups[2].max(1));
    }

}

/// A view of one mip level (clamped to the texture's last, as AMD's Vulkan backend does
/// for the inpainting pyramid's thirteen bindings).
fn mip_view(texture: &wgpu::Texture, mip: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        base_mip_level: mip.min(texture.mip_level_count() - 1),
        mip_level_count: Some(1),
        ..Default::default()
    })
}

fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&Default::default())
}

/// A 2D texture for the frame-generation passes.
pub(crate) fn texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    mips: u32,
    extra: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | extra,
        view_formats: &[],
    })
}

/// Uniform slots, each `SLOT` bytes apart (the largest `min_uniform_buffer_offset_alignment`
/// wgpu allows), written in one copy per dispatch.
pub(crate) const SLOT: u64 = 256;

pub(crate) fn write_slots(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Buffer,
    slots: &[&[u8]],
) {
    use wgpu::util::DeviceExt as _;
    let mut bytes = vec![0u8; slots.len() * SLOT as usize];
    for (i, slot) in slots.iter().enumerate() {
        assert!(slot.len() as u64 <= SLOT);
        bytes[i * SLOT as usize..][..slot.len()].copy_from_slice(slot);
    }
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("wgpu-ffx frame generation constants"),
        contents: &bytes,
        usage: wgpu::BufferUsages::COPY_SRC,
    });
    encoder.copy_buffer_to_buffer(&staging, 0, target, 0, bytes.len() as u64);
}

pub(crate) fn uniform_slots(device: &wgpu::Device, label: &str, count: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: count * SLOT,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// FidelityFX SPD's dispatch setup (`ffxSpdSetup` in `ffx_spd.h`): thread groups per 64x64
/// tile of the rectangle, the group count and the mip count (`mips`, or log2 of the larger
/// side capped at 12 when `None`).
pub(crate) fn spd_setup(size: [u32; 2], mips: Option<u32>) -> ([u32; 2], u32, u32) {
    let groups = [(size[0].max(1) - 1) / 64 + 1, (size[1].max(1) - 1) / 64 + 1];
    let mips = mips.unwrap_or_else(|| (size[0].max(size[1]).max(1) as f32).log2().floor().min(12.0) as u32);
    (groups, groups[0] * groups[1], mips)
}

/// Just enough SPIR-V reading for the passes: binding decorations and the workgroup size.
pub(crate) mod spirv {
    const OP_EXECUTION_MODE: u32 = 16;
    const OP_DECORATE: u32 = 71;
    const MODE_LOCAL_SIZE: u32 = 17;
    const DECORATION_BINDING: u32 = 33;

    pub fn words(bytes: &[u8]) -> Vec<u32> {
        assert!(bytes.len().is_multiple_of(4) && bytes.len() >= 20, "not SPIR-V");
        let words: Vec<u32> = bytes.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        assert_eq!(words[0], 0x0723_0203, "not little-endian SPIR-V");
        words
    }

    fn instructions(words: &[u32]) -> impl Iterator<Item = (usize, u32, &[u32])> {
        let mut at = 5;
        std::iter::from_fn(move || {
            if at >= words.len() {
                return None;
            }
            let count = (words[at] >> 16) as usize;
            let opcode = words[at] & 0xffff;
            assert!(count > 0 && at + count <= words.len(), "malformed SPIR-V");
            let start = at;
            at += count;
            Some((start, opcode, &words[start + 1..start + count]))
        })
    }

    /// Every `Binding` decoration's number, sorted.
    pub fn bindings(words: &[u32]) -> Vec<u32> {
        let mut out: Vec<u32> = instructions(words)
            .filter(|(_, op, args)| *op == OP_DECORATE && args.len() >= 3 && args[1] == DECORATION_BINDING)
            .map(|(_, _, args)| args[2])
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn local_size(words: &[u32]) -> (u32, u32, u32) {
        instructions(words)
            .find(|(_, op, args)| *op == OP_EXECUTION_MODE && args.len() >= 5 && args[1] == MODE_LOCAL_SIZE)
            .map(|(_, _, args)| (args[2], args[3], args[4]))
            .expect("compute shader without a LocalSize execution mode")
    }

    pub fn remap_binding(words: &mut [u32], from: u32, to: u32) {
        remap_bindings(words, &[(from, to)]);
    }

    /// Rewrite every `Binding` decoration found in `map` (`(from, to)` pairs) at once.
    pub fn remap_bindings(words: &mut [u32], map: &[(u32, u32)]) {
        let at: Vec<(usize, u32)> = instructions(words)
            .filter(|(_, op, args)| *op == OP_DECORATE && args.len() >= 3 && args[1] == DECORATION_BINDING)
            .filter_map(|(start, _, args)| map.iter().find(|(from, _)| *from == args[2]).map(|(_, to)| (start + 3, *to)))
            .collect();
        for (index, to) in at {
            words[index] = to;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    /// `(binding, GLSL variable)` for one pass, read from the shader sources: the pass's
    /// `#define <PREFIX>_BIND_<X> <n>` lines, and the callbacks header's
    /// `layout(... binding = <PREFIX>_BIND_<X> ...) ... <variable>;` declarations.
    pub fn source_table(pass_glsl: &str, callbacks_glsl: &str) -> Vec<(u32, String)> {
        let mut numbers = BTreeMap::new();
        for line in pass_glsl.lines() {
            let mut words = line.split_whitespace();
            if words.next() == Some("#define")
                && let (Some(name), Some(value)) = (words.next(), words.next())
                && name.contains("_BIND_")
                && let Ok(number) = value.parse::<u32>()
            {
                numbers.insert(name.to_string(), number);
            }
        }
        let mut table = Vec::new();
        let lines: Vec<&str> = callbacks_glsl.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let line = line.trim();
            if !line.starts_with("layout") || line.starts_with("//") {
                continue;
            }
            let Some(binding_at) = line.find("binding") else { continue };
            let after = &line[binding_at + "binding".len()..];
            let define: String = after
                .trim_start_matches([' ', '='])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let Some(&number) = numbers.get(&define) else { continue };
            // The variable is the last identifier before `;`; a block's (`uniform cbFI_t`,
            // `buffer FrameInterpolationCounters_t`) instance name follows its closing brace.
            let declaration = line.split("//").next().unwrap().trim_end();
            let variable = if let Some(stripped) = declaration.strip_suffix(';') {
                stripped.split_whitespace().last().unwrap().to_string()
            } else {
                let close = lines[index..]
                    .iter()
                    .map(|l| l.trim())
                    .find(|l| l.starts_with('}'))
                    .expect("block without a closing brace");
                close.trim_start_matches('}').trim().trim_end_matches(';').trim().to_string()
            };
            table.push((number, variable));
        }
        table.sort();
        table.dedup();
        table
    }
}
