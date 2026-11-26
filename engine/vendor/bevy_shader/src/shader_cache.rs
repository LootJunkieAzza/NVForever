use crate::nvf_composed::ComposedStore;
use crate::shader::*;
use alloc::borrow::Cow;
use alloc::sync::Arc;
use bevy_asset::AssetId;
use bevy_platform::collections::{hash_map::EntryRef, HashMap, HashSet};
use core::hash::Hash;
use thiserror::Error;
use tracing::debug;
use tracing::warn;

pub(crate) fn wesl_module_path(import_path: &ShaderImport) -> Option<wesl::syntax::ModulePath> {
    match import_path {
        ShaderImport::Custom(name) => {
            let mut segments = name.split("::");
            let package = segments.next().filter(|s| !s.is_empty())?;
            let components = segments
                .map(|s| (!s.is_empty()).then(|| s.to_string()))
                .collect::<Option<Vec<_>>>()?;
            Some(wesl::syntax::ModulePath {
                origin: wesl::syntax::PathOrigin::Package(package.to_string()),
                components,
            })
        }
        // `ModulePath::from_path` would strip anything after a `.` as an extension.
        ShaderImport::AssetPath(path) => {
            let components: Vec<String> = path
                .split('/')
                .filter(|component| !component.is_empty())
                .map(str::to_string)
                .collect();
            (!components.is_empty()).then_some(wesl::syntax::ModulePath {
                origin: wesl::syntax::PathOrigin::Absolute,
                components,
            })
        }
    }
}

fn is_module_not_found(error: &wesl::Error) -> bool {
    match error {
        wesl::Error::ResolveError(wesl::error::ResolveError::ModuleNotFound(..))
        | wesl::Error::ImportError(wesl::error::ImportError::ResolveError(
            wesl::error::ResolveError::ModuleNotFound(..),
        )) => true,
        wesl::Error::Error(diagnostic) => is_module_not_found(&diagnostic.error),
        _ => false,
    }
}

/// Fully composed source code of a shader module, with all shader defs applied.
///
/// This is roughly equivalent to [`wgpu::ShaderSource`](https://docs.rs/wgpu/latest/wgpu/enum.ShaderSource.html),
/// but with less variants and more concrete types instead of [`Cow`].
///
/// This source will be parsed and validated by the renderer.
///
/// Any necessary shader translation (e.g. from WGSL to SPIR-V or vice versa)
/// must be done internally by the renderer.
#[derive(Clone, Debug)]
pub enum ShaderCacheSource<'a> {
    /// SPIR-V module represented as a slice of words.
    SpirV(&'a [u8]),
    /// WGSL module as a string slice.
    Wgsl(String),
}

/// An id of a pipeline, typically in the [`PipelineCache`](https://docs.rs/bevy/latest/bevy/render/render_resource/struct.PipelineCache.html)
/// Typically corresponds to a unique combination of [`Shader`] and [`ShaderDefVal`]s.
pub type CachedPipelineId = usize;

struct ShaderData<ShaderModule> {
    pipelines: HashSet<CachedPipelineId>,
    processed_shaders: HashMap<Box<[ShaderDefVal]>, Arc<ShaderModule>>,
    resolved_imports: HashMap<ShaderImport, AssetId<Shader>>,
    dependents: HashSet<AssetId<Shader>>,
    /// NVForever modules off the lock: bumped when the modules are cleared, so a module
    /// built from the source before is not kept (`ShaderCache::finish`).
    generation: u64,
}

impl<T> Default for ShaderData<T> {
    fn default() -> Self {
        Self {
            pipelines: Default::default(),
            processed_shaders: Default::default(),
            resolved_imports: Default::default(),
            dependents: Default::default(),
            generation: 0,
        }
    }
}

/// NVForever modules off the lock: what [`ShaderCache::prepare`] leaves to its caller.
///
/// Making a module (naga's parse, validation and output, then the driver's module) took
/// 751 ms of a Novac start's pipelines in all, and Bevy made each under the shader cache's
/// one lock, so the compile threads took turns [observed 2026-11-19, `pipeline timing:`
/// lines: lock waits 3.9 s in all]. Now the cache composes under the lock and hands the
/// source out; the caller makes the module without the lock and gives it back with
/// [`ShaderCache::finish`]. A module asked for by two pipelines at once is made once: the
/// second waits on the first's [`ModuleSlot`].
pub enum ModuleStep<ShaderModule> {
    /// Made before.
    Ready(Arc<ShaderModule>),
    /// The caller makes it: [`ModuleBuild::source`], then [`ShaderCache::finish`].
    Build(ModuleBuild<ShaderModule>),
    /// Another thread makes it; [`ModuleSlot::wait`] without holding the cache's lock.
    Wait(Arc<ModuleSlot<ShaderModule>>),
}

/// NVForever modules off the lock: a module one thread makes and others wait for.
pub struct ModuleSlot<ShaderModule> {
    made: std::sync::Mutex<Option<Result<Arc<ShaderModule>, ShaderCacheError>>>,
    ready: std::sync::Condvar,
}

impl<ShaderModule> ModuleSlot<ShaderModule> {
    fn new() -> Self {
        Self {
            made: std::sync::Mutex::new(None),
            ready: std::sync::Condvar::new(),
        }
    }

    fn fill(&self, made: Result<Arc<ShaderModule>, ShaderCacheError>) {
        let mut slot = self
            .made
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(made);
        }
        self.ready.notify_all();
    }

    fn failed(&self) -> bool {
        self.made
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(Result::is_err)
    }

    /// The module once its maker has finished. Never call it holding the cache's lock: the
    /// maker needs that lock to finish.
    pub fn wait(&self) -> Result<Arc<ShaderModule>, ShaderCacheError> {
        let mut slot = self
            .made
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(made) = slot.as_ref() {
                return made.clone();
            }
            slot = self
                .ready
                .wait(slot)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

/// NVForever modules off the lock: one module to make, from composed source.
pub struct ModuleBuild<ShaderModule> {
    id: AssetId<Shader>,
    defs: Box<[ShaderDefVal]>,
    generation: u64,
    source: OwnedShaderSource,
    validate: ValidateShader,
    slot: Option<Arc<ModuleSlot<ShaderModule>>>,
}

impl<ShaderModule> ModuleBuild<ShaderModule> {
    /// The composed source to make the module from.
    pub fn source(&self) -> ShaderCacheSource<'_> {
        match &self.source {
            OwnedShaderSource::SpirV(bytes) => ShaderCacheSource::SpirV(bytes),
            OwnedShaderSource::Wgsl(text) => ShaderCacheSource::Wgsl(text.clone()),
        }
    }

    /// Whether to validate it (`Shader::validate_shader`).
    pub fn validate(&self) -> &ValidateShader {
        &self.validate
    }
}

impl<ShaderModule> Drop for ModuleBuild<ShaderModule> {
    /// A build dropped unfinished (its maker panicked, or stopped early because its
    /// pipeline's other shader was not ready) still wakes the threads waiting. They get an
    /// error the pipeline cache retries (`ShaderImportNotYetAvailable`): the module itself
    /// never failed, so a waiter's pipeline is queued again and the next ask makes it (the
    /// failed slot is passed over, `step`), instead of failing for the rest of the run.
    fn drop(&mut self) {
        if let Some(slot) = self.slot.take() {
            slot.fill(Err(ShaderCacheError::ShaderImportNotYetAvailable));
        }
    }
}

enum OwnedShaderSource {
    SpirV(Vec<u8>),
    Wgsl(String),
}

/// A cache for shaders and shader imports, with asset state-tracking for
/// waiting to load shaders until all imports are resolved.
///
/// Note that the `RenderDevice` generic parameter is a means by which
/// to avoid a cyclic dependency with `bevy_render`, while also permitting
/// alternative rendering implementations. The actual processing of the
/// shader source into a usable compiled module is left to the renderer.
pub struct ShaderCache<ShaderModule, RenderDevice> {
    device: RenderDevice,
    data: HashMap<AssetId<Shader>, ShaderData<ShaderModule>>,
    load_module: fn(
        &RenderDevice,
        ShaderCacheSource,
        &ValidateShader,
    ) -> Result<ShaderModule, ShaderCacheError>,
    module_path_to_asset_id: HashMap<wesl::syntax::ModulePath, AssetId<Shader>>,
    shaders: HashMap<AssetId<Shader>, Shader>,
    import_path_shaders: HashMap<ShaderImport, AssetId<Shader>>,
    waiting_on_import: HashMap<ShaderImport, Vec<AssetId<Shader>>>,
    missing_import_logged: HashSet<AssetId<Shader>>,
    /// NVForever composed shaders (`nvf_composed.rs`): compositions kept from earlier runs,
    /// each shader's source hash, and the shader of each module path as text.
    composed: Option<Arc<ComposedStore>>,
    source_hashes: HashMap<AssetId<Shader>, u64>,
    path_ids: HashMap<String, AssetId<Shader>>,
    /// NVForever modules off the lock: modules being made, by shader and definitions.
    pending: HashMap<(AssetId<Shader>, Box<[ShaderDefVal]>), Arc<ModuleSlot<ShaderModule>>>,
    /// NVForever pipeline list: each shader's id by its `Shader::path`, the paths two shaders
    /// share (no pipeline of theirs is listed), and a count bumped whenever a shader is set.
    by_path: HashMap<String, AssetId<Shader>>,
    shared_paths: HashSet<String>,
    shaders_set: u64,
}

/// A compile time shader value definition to be inlined into the shader source.
/// Variant tuples contain the name of the definition, and the value.
#[expect(missing_docs, reason = "Enum variants are self-explanatory")]
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug, Hash)]
pub enum ShaderDefVal {
    Bool(Cow<'static, str>, bool),
    Int(Cow<'static, str>, i32),
    UInt(Cow<'static, str>, u32),
}

impl From<&'static str> for ShaderDefVal {
    fn from(key: &'static str) -> Self {
        ShaderDefVal::Bool(key.into(), true)
    }
}

impl From<String> for ShaderDefVal {
    fn from(key: String) -> Self {
        ShaderDefVal::Bool(key.into(), true)
    }
}

impl ShaderDefVal {
    /// Returns the value of the define as a string.
    pub fn value_as_string(&self) -> String {
        match self {
            ShaderDefVal::Bool(_, def) => def.to_string(),
            ShaderDefVal::Int(_, def) => def.to_string(),
            ShaderDefVal::UInt(_, def) => def.to_string(),
        }
    }
}

impl<ShaderModule, RenderDevice> ShaderCache<ShaderModule, RenderDevice> {
    /// Creates a new [`ShaderCache`] with the given shader module loading
    /// function. `load_module` is responsible for actually compiling shader
    /// source into a module usable by the render device.
    pub fn new(
        device: RenderDevice,
        load_module: fn(
            &RenderDevice,
            ShaderCacheSource,
            &ValidateShader,
        ) -> Result<ShaderModule, ShaderCacheError>,
    ) -> Self {
        Self {
            device,
            load_module,
            data: Default::default(),
            module_path_to_asset_id: Default::default(),
            shaders: Default::default(),
            import_path_shaders: Default::default(),
            waiting_on_import: Default::default(),
            missing_import_logged: Default::default(),
            composed: None,
            source_hashes: Default::default(),
            path_ids: Default::default(),
            pending: Default::default(),
            by_path: Default::default(),
            shared_paths: Default::default(),
            shaders_set: 0,
        }
    }

    /// NVForever pipeline list: the path a shader is known by across runs, when it has one no
    /// other shader has.
    pub fn shader_path(&self, id: AssetId<Shader>) -> Option<&str> {
        let path = self.shaders.get(&id)?.path.as_str();
        (!path.is_empty() && !self.shared_paths.contains(path)).then_some(path)
    }

    /// NVForever pipeline list: the shader known by this path now ([`Self::shader_path`]).
    pub fn shader_by_path(&self, path: &str) -> Option<AssetId<Shader>> {
        if self.shared_paths.contains(path) {
            return None;
        }
        self.by_path.get(path).copied()
    }

    /// NVForever pipeline list: changes whenever a shader is set or removed.
    pub fn shaders_set(&self) -> u64 {
        self.shaders_set
    }

    /// NVForever composed shaders: compose Wesl shaders through this store from now on,
    /// using what it kept and keeping what is composed (`None`: compose every time).
    pub fn set_composed_store(&mut self, store: Option<Arc<ComposedStore>>) {
        self.composed = store;
    }

    /// Attempts to retrieve or create a compiled shader module for the given
    /// shader id and shader definitions.
    ///
    /// The provided `pipeline` is tracked so it may later be reported "dirty"
    /// when a shader is removed or replaced.
    ///
    /// Note that the cache is keyed by `id` and `shader_defs`, meaning providing
    /// the same `shader_defs` in a different order, or with redundancies, will
    /// not result in cache hits, and thus require re-composing the module and
    /// calling `load_module` again.
    pub fn get(
        &mut self,
        pipeline: CachedPipelineId,
        id: AssetId<Shader>,
        shader_defs: &[ShaderDefVal],
    ) -> Result<Arc<ShaderModule>, ShaderCacheError> {
        match self.step(Some(pipeline), id, shader_defs, false)? {
            ModuleStep::Ready(module) => Ok(module),
            ModuleStep::Build(build) => {
                let made = (self.load_module)(&self.device, build.source(), &build.validate);
                self.finish(build, made)
            }
            ModuleStep::Wait(slot) => slot.wait(),
        }
    }

    /// NVForever modules off the lock ([`ModuleStep`]): the module if it was made, else
    /// what to make it from (composed now, under the lock) or the slot of the thread making it.
    /// `pipeline`: the pipeline to compile again when the shader changes (`None`: a pipeline
    /// of nvforever's pipeline list, which is not).
    pub fn prepare(
        &mut self,
        pipeline: Option<CachedPipelineId>,
        id: AssetId<Shader>,
        shader_defs: &[ShaderDefVal],
    ) -> Result<ModuleStep<ShaderModule>, ShaderCacheError> {
        self.step(pipeline, id, shader_defs, true)
    }

    /// NVForever modules off the lock: keep the module made from `build` (unless its shader
    /// changed meanwhile) and hand it to the threads waiting for it.
    pub fn finish(
        &mut self,
        mut build: ModuleBuild<ShaderModule>,
        made: Result<ShaderModule, ShaderCacheError>,
    ) -> Result<Arc<ShaderModule>, ShaderCacheError> {
        let made = made.map(Arc::new);
        let data = self.data.entry(build.id).or_default();
        if data.generation == build.generation
            && let Ok(module) = &made
        {
            data.processed_shaders
                .insert(build.defs.clone(), module.clone());
        }
        if let Some(slot) = build.slot.take() {
            let key = (build.id, core::mem::take(&mut build.defs));
            if self.pending.get(&key).is_some_and(|s| Arc::ptr_eq(s, &slot)) {
                self.pending.remove(&key);
            }
            slot.fill(made.clone());
        }
        made
    }

    /// The module made before, or what to make it from. `shared`: another thread may make it
    /// meanwhile (a slot is left for it); `get` makes it holding the lock and leaves none.
    fn step(
        &mut self,
        pipeline: Option<CachedPipelineId>,
        id: AssetId<Shader>,
        shader_defs: &[ShaderDefVal],
        shared: bool,
    ) -> Result<ModuleStep<ShaderModule>, ShaderCacheError> {
        let shader = self
            .shaders
            .get(&id)
            .ok_or(ShaderCacheError::ShaderNotLoaded(id))?;

        let data = self.data.entry(id).or_default();

        // Wesl imports are scanned as both module and item candidates.
        if !matches!(shader.source, Source::Wesl(_)) {
            let n_asset_imports = shader
                .imports
                .iter()
                .filter(|import| matches!(import, ShaderImport::AssetPath(_)))
                .count();
            let n_resolved_asset_imports = data
                .resolved_imports
                .keys()
                .filter(|import| matches!(import, ShaderImport::AssetPath(_)))
                .count();
            if n_asset_imports != n_resolved_asset_imports {
                return Err(ShaderCacheError::ShaderImportNotYetAvailable);
            }
        }

        if let Some(pipeline) = pipeline {
            data.pipelines.insert(pipeline);
        }

        let mut wesl_dependencies: Vec<AssetId<Shader>> = Vec::new();

        let generation = data.generation;
        let pending = shared
            .then(|| self.pending.get(&(id, Box::from(shader_defs))).cloned())
            .flatten()
            // A maker that stopped early (its pipeline's other shader failed first) leaves
            // its slot failed: make the module again.
            .filter(|slot| !slot.failed());
        let step = match data.processed_shaders.entry_ref(shader_defs) {
            EntryRef::Occupied(entry) => ModuleStep::Ready(entry.get().clone()),
            EntryRef::Vacant(_) if pending.is_some() => ModuleStep::Wait(pending.unwrap()),
            EntryRef::Vacant(_) => {
                debug!(
                    "processing shader {}, with shader defs {:?}",
                    id, shader_defs
                );
                let shader_source = match &shader.source {
                    Source::SpirV(data) => OwnedShaderSource::SpirV(data.to_vec()),
                    Source::Wesl(_) => {
                        if let Some(module_path) = wesl_module_path(&shader.import_path) {
                            let mut compiler_options = wesl::CompileOptions {
                                imports: true,
                                condcomp: true,
                                visibility: false,
                                ..Default::default()
                            };

                            let mut closure_defs = Vec::new();
                            let mut visited = HashSet::new();
                            let mut to_visit = vec![id];
                            while let Some(dep_id) = to_visit.pop() {
                                if !visited.insert(dep_id) {
                                    continue;
                                }
                                let Some(dep) = self.shaders.get(&dep_id) else {
                                    continue;
                                };
                                if dep_id != id {
                                    closure_defs.extend(dep.shader_defs.iter());
                                }
                                for import in &dep.imports {
                                    if let Some(import_id) = self.import_path_shaders.get(import) {
                                        to_visit.push(*import_id);
                                    }
                                }
                            }
                            let mut constants = alloc::collections::BTreeMap::new();
                            for shader_def in closure_defs
                                .into_iter()
                                .chain(shader_defs.iter())
                                .chain(shader.shader_defs.iter())
                            {
                                match shader_def {
                                    ShaderDefVal::Bool(key, value) => {
                                        compiler_options
                                            .features
                                            .flags
                                            .insert(key.to_string(), (*value).into());
                                    }
                                    ShaderDefVal::Int(key, value) => {
                                        compiler_options
                                            .features
                                            .flags
                                            .insert(key.to_string(), true.into());
                                        constants.insert(key.as_ref(), value.to_string());
                                    }
                                    ShaderDefVal::UInt(key, value) => {
                                        compiler_options
                                            .features
                                            .flags
                                            .insert(key.to_string(), true.into());
                                        constants.insert(key.as_ref(), value.to_string());
                                    }
                                }
                            }
                            let constants_source: String = constants
                                .iter()
                                .map(|(name, value)| format!("const {name} = {value};\n"))
                                .collect();

                            // NVForever composed shaders: a composition kept from an earlier
                            // run, while the modules it read have the same sources.
                            let composed_key = self.composed.as_ref().map(|_| {
                                ComposedStore::key(
                                    &module_path.to_string(),
                                    &compiler_options,
                                    &constants_source,
                                )
                            });
                            if let (Some(store), Some(key)) = (&self.composed, &composed_key)
                                && let Some((wgsl, used)) =
                                    store.get(key, |path| module_source_hash(&self.path_ids, &self.source_hashes, path))
                            {
                                store
                                    .hits
                                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                                for path in used {
                                    if let Some(dep_id) = self.path_ids.get(&path)
                                        && *dep_id != id
                                    {
                                        wesl_dependencies.push(*dep_id);
                                    }
                                }
                                OwnedShaderSource::Wgsl(wgsl)
                            } else {
                                let shader_resolver = ShaderResolver::new(
                                    &self.module_path_to_asset_id,
                                    &self.shaders,
                                    &constants_source,
                                );

                                let compiled = wesl::compile(
                                    &module_path,
                                    &compiler_options,
                                    &shader_resolver,
                                )
                                .map_err(|error| {
                                    // We use render_plain to avoid rendering ANSI codes
                                    // Workaround for: https://github.com/tokio-rs/tracing/issues/3378
                                    if is_module_not_found(&error) {
                                        if self.missing_import_logged.insert(id) {
                                            warn!(
                                                "Shader `{}` has an unresolved import:\n{}",
                                                shader.path,
                                                error.diagnostic().render_plain()
                                            );
                                        }
                                        ShaderCacheError::ShaderImportNotYetAvailable
                                    } else {
                                        ShaderCacheError::ProcessShaderError(
                                            error.diagnostic().render_plain(),
                                        )
                                    }
                                })?;

                                let mut composed_deps = Vec::new();
                                for used in &compiled.modules {
                                    let used_path = match &used.path.origin {
                                        wesl::syntax::PathOrigin::Package(pkg)
                                            if pkg.contains('/') =>
                                        {
                                            Cow::Owned(wesl::syntax::ModulePath {
                                                origin: wesl::syntax::PathOrigin::Package(
                                                    pkg.rsplit('/').next().unwrap().to_string(),
                                                ),
                                                components: used.path.components.clone(),
                                            })
                                        }
                                        _ => Cow::Borrowed(&used.path),
                                    };
                                    if let Some(dep_id) =
                                        self.module_path_to_asset_id.get(used_path.as_ref())
                                        && *dep_id != id
                                    {
                                        wesl_dependencies.push(*dep_id);
                                    }
                                    if self.composed.is_some() {
                                        let path = used_path.to_string();
                                        let hash = module_source_hash(&self.path_ids, &self.source_hashes, &path);
                                        composed_deps.push((path, hash));
                                    }
                                }

                                let wgsl = compiled.to_string();
                                if let (Some(store), Some(key)) = (&self.composed, composed_key) {
                                    store
                                        .misses
                                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                                    store.put(key, wgsl.clone(), composed_deps);
                                }
                                OwnedShaderSource::Wgsl(wgsl)
                            }
                        } else {
                            return Err(ShaderCacheError::ProcessShaderError(format!(
                                "Wesl shader `{}` has a malformed import path `{:?}`",
                                shader.path, shader.import_path
                            )));
                        }
                    }
                    Source::Wgsl(wgsl_source) => {
                        OwnedShaderSource::Wgsl(wgsl_source.to_string())
                    }
                };
                let validate = shader.validate_shader.clone();
                let defs: Box<[ShaderDefVal]> = shader_defs.into();
                let slot = shared.then(|| {
                    let slot = Arc::new(ModuleSlot::new());
                    self.pending.insert((id, defs.clone()), slot.clone());
                    slot
                });
                ModuleStep::Build(ModuleBuild {
                    id,
                    defs,
                    generation,
                    source: shader_source,
                    validate,
                    slot,
                })
            }
        };

        for dep_id in wesl_dependencies {
            self.data.entry(dep_id).or_default().dependents.insert(id);
        }

        Ok(step)
    }

    fn clear(&mut self, id: AssetId<Shader>) -> Vec<CachedPipelineId> {
        let mut shaders_to_clear = vec![id];
        let mut visited = HashSet::new();
        let mut pipelines_to_queue = Vec::new();
        while let Some(handle) = shaders_to_clear.pop() {
            if !visited.insert(handle) {
                continue;
            }
            // NVForever modules off the lock: a module being made from the old source is not
            // kept, and the next ask makes it again.
            self.pending.retain(|(pending_id, _), _| *pending_id != handle);
            if let Some(data) = self.data.get_mut(&handle) {
                data.generation += 1;
                data.processed_shaders.clear();
                pipelines_to_queue.extend(data.pipelines.iter().copied());
                shaders_to_clear.extend(data.dependents.iter().copied());
            }
        }

        pipelines_to_queue
    }

    /// Inserts and possibly replaces a shader at the given asset id.
    ///
    /// Returns a vec of which cached pipelines depended on it
    /// (directly or indirectly via a shader import) and thus must be recompiled.
    pub fn set_shader(&mut self, id: AssetId<Shader>, shader: Shader) -> Vec<CachedPipelineId> {
        self.missing_import_logged.clear();
        let mut pipelines_to_queue = self.clear(id);
        let path = &shader.import_path;
        self.import_path_shaders.insert(path.clone(), id);
        if let Some(waiting_shaders) = self.waiting_on_import.get_mut(path) {
            for waiting_shader in waiting_shaders.drain(..) {
                // resolve waiting shader import
                let data = self.data.entry(waiting_shader).or_default();
                data.resolved_imports.insert(path.clone(), id);
                // add waiting shader as dependent of this shader
                let data = self.data.entry(id).or_default();
                data.dependents.insert(waiting_shader);
            }
        }

        for import in shader.imports.iter() {
            if let Some(import_id) = self.import_path_shaders.get(import).copied() {
                // resolve import because it is currently available
                let data = self.data.entry(id).or_default();
                data.resolved_imports.insert(import.clone(), import_id);
                // add this shader as a dependent of the import
                let data = self.data.entry(import_id).or_default();
                data.dependents.insert(id);
            } else {
                let waiting = self.waiting_on_import.entry(import.clone()).or_default();
                waiting.push(id);
            }
        }

        if let Source::Wesl(_) = shader.source {
            match wesl_module_path(&shader.import_path) {
                Some(module_path) => {
                    if let Some(existing) = self.module_path_to_asset_id.get(&module_path).copied()
                        && existing != id
                    {
                        warn!(
                            "Shader module path `{module_path}` was registered to a different \
                            shader, replacing with `{}`",
                            shader.path
                        );
                        pipelines_to_queue.extend(self.clear(existing));
                    }
                    self.path_ids.insert(module_path.to_string(), id);
                    self.module_path_to_asset_id.insert(module_path, id);
                }
                None => warn!(
                    "Shader `{}` has a malformed import path `{:?}`",
                    shader.path, shader.import_path
                ),
            }
        }
        self.source_hashes.insert(id, shader_source_hash(&shader));
        // NVForever pipeline list: the shader by its path.
        self.shaders_set += 1;
        if let Some(other) = self.by_path.insert(shader.path.clone(), id)
            && other != id
            && self.shaders.contains_key(&other)
        {
            self.shared_paths.insert(shader.path.clone());
        }
        self.shaders.insert(id, shader);
        pipelines_to_queue
    }

    /// Removes the shader with the given asset id.
    ///
    /// Returns a vec of which cached pipelines depended on it
    /// (directly or indirectly via a shader import) and thus must be recompiled.
    pub fn remove(&mut self, id: AssetId<Shader>) -> Vec<CachedPipelineId> {
        let pipelines_to_queue = self.clear(id);
        self.source_hashes.remove(&id);
        self.shaders_set += 1;
        if let Some(shader) = self.shaders.remove(&id) {
            if self.by_path.get(&shader.path) == Some(&id) {
                self.by_path.remove(&shader.path);
            }
            if self.import_path_shaders.get(&shader.import_path) == Some(&id) {
                self.import_path_shaders.remove(&shader.import_path);
            }
            if let Source::Wesl(_) = shader.source
                && let Some(module_path) = wesl_module_path(&shader.import_path)
                && self.module_path_to_asset_id.get(&module_path) == Some(&id)
            {
                self.path_ids.remove(&module_path.to_string());
                self.module_path_to_asset_id.remove(&module_path);
            }
        }

        pipelines_to_queue
    }
}

/// NVForever composed shaders: the hash of the source of the shader that provides this
/// module now, or `None` when none does.
fn module_source_hash(
    path_ids: &HashMap<String, AssetId<Shader>>,
    source_hashes: &HashMap<AssetId<Shader>, u64>,
    path: &str,
) -> Option<u64> {
    let id = path_ids.get(path)?;
    Some(source_hashes.get(id).copied().unwrap_or(0))
}

/// NVForever composed shaders: the hash a shader's source is compared by.
fn shader_source_hash(shader: &Shader) -> u64 {
    match &shader.source {
        Source::Wesl(text) | Source::Wgsl(text) => {
            crate::nvf_composed::source_hash(text.as_bytes())
        }
        Source::SpirV(bytes) => crate::nvf_composed::source_hash(bytes),
    }
}

/// A Wesl import resolver. Maps module paths to actual Wesl shader source.
pub struct ShaderResolver<'a> {
    module_path_to_asset_id: &'a HashMap<wesl::syntax::ModulePath, AssetId<Shader>>,
    shaders: &'a HashMap<AssetId<Shader>, Shader>,
    constants_source: &'a str,
}

impl<'a> ShaderResolver<'a> {
    /// Creates a shader resolver with the given map of module paths to shader asset ids,
    /// map of shader asset ids to shader source, and the source of the virtual
    /// `constants` module. This resolver is not meant to be long living.
    pub fn new(
        module_path_to_asset_id: &'a HashMap<wesl::syntax::ModulePath, AssetId<Shader>>,
        shaders: &'a HashMap<AssetId<Shader>, Shader>,
        constants_source: &'a str,
    ) -> Self {
        Self {
            module_path_to_asset_id,
            shaders,
            constants_source,
        }
    }
}

impl<'a> wesl::Resolver for ShaderResolver<'a> {
    fn resolve_source(
        &self,
        module_path: &wesl::syntax::ModulePath,
    ) -> Result<Cow<'_, str>, wesl::error::ResolveError> {
        let module_path = self.canonical_path(module_path);
        if module_path.origin == wesl::syntax::PathOrigin::Package("constants".to_string())
            && module_path.components.is_empty()
        {
            return Ok(Cow::Borrowed(self.constants_source));
        }
        let asset_id = self
            .module_path_to_asset_id
            .get(&module_path)
            .ok_or_else(|| {
                wesl::error::ResolveError::ModuleNotFound(
                    module_path.clone(),
                    "no shader registered for this module path".to_string(),
                )
            })?;

        let shader = self.shaders.get(asset_id).ok_or_else(|| {
            wesl::error::ResolveError::ModuleNotFound(
                module_path.clone(),
                "shader asset not loaded".to_string(),
            )
        })?;
        Ok(Cow::Borrowed(shader.source.as_str()))
    }

    fn canonical_path(&self, module_path: &wesl::syntax::ModulePath) -> wesl::syntax::ModulePath {
        match &module_path.origin {
            wesl::syntax::PathOrigin::Package(pkg) if pkg.contains('/') => {
                wesl::syntax::ModulePath {
                    origin: wesl::syntax::PathOrigin::Package(
                        pkg.rsplit('/').next().unwrap().to_string(),
                    ),
                    components: module_path.components.clone(),
                }
            }
            _ => module_path.clone(),
        }
    }

    fn display_name(&self, module_path: &wesl::syntax::ModulePath) -> Option<String> {
        let module_path = self.canonical_path(module_path);
        let asset_id = self.module_path_to_asset_id.get(&module_path)?;
        let shader = self.shaders.get(asset_id)?;
        Some(shader.path.clone())
    }
}

/// Type of error returned by a `PipelineCache` when the creation of a GPU pipeline object failed.
#[expect(missing_docs, reason = "Enum variants are self-explanatory")]
#[derive(Error, Debug, Clone)]
pub enum ShaderCacheError {
    #[error(
        "Pipeline could not be compiled because the following shader could not be loaded: {0:?}"
    )]
    ShaderNotLoaded(AssetId<Shader>),
    #[error("Failed to process shader:\n{0}")]
    ProcessShaderError(String),
    #[error("Shader import not yet available.")]
    ShaderImportNotYetAvailable,
    #[error("Could not create shader module: {0}")]
    CreateShaderModule(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache() -> ShaderCache<String, ()> {
        ShaderCache::new((), |_, source, _| match source {
            ShaderCacheSource::Wgsl(wgsl) => Ok(wgsl),
            _ => panic!("expected wgsl output"),
        })
    }

    #[test]
    fn import_resolution() {
        let mut cache = test_cache();

        let (maths_id, lighting_id, root_id) = set_test_shaders(&mut cache);

        let compiled = cache
            .get(0, root_id, &[ShaderDefVal::Bool("BRIGHT".into(), true)])
            .unwrap();
        assert!(compiled.contains("fn fragment"));
        assert!(compiled.contains("* 2.0"));
        assert!(compiled.contains("+ 0.1"));

        let (maths, lighting, _) = test_shaders();
        assert!(cache.set_shader(lighting_id, lighting).contains(&0));
        let _ = cache.get(0, root_id, &[ShaderDefVal::Bool("BRIGHT".into(), true)]);
        assert!(cache.set_shader(maths_id, maths).contains(&0));
    }

    #[test]
    fn constants_module() {
        let mut cache = test_cache();
        let mut shader = Shader::from_wesl(
            r#"
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> scale: f32;
var<uniform> batch: array<vec4<f32>, constants::BATCH_SIZE>;

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(scale) + batch[0];
}
"#,
            "shaders/constants_user.wesl",
        );
        shader.shader_defs = vec![ShaderDefVal::UInt("BATCH_SIZE".into(), 4)];
        let id = AssetId::Uuid {
            uuid: bevy_asset::uuid::Uuid::from_u128(7),
        };
        cache.set_shader(id, shader);

        let compiled = cache
            .get(
                0,
                id,
                &[ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 2)],
            )
            .unwrap();
        assert!(compiled.contains("= 2;"));
        assert!(compiled.contains("= 4;"));
    }

    #[test]
    fn compile_error_display_names() {
        let mut cache = test_cache();
        let (maths, _, root) = test_shaders();
        let (maths_id, lighting_id, root_id) = test_ids();
        let broken_lighting = Shader::from_wesl(
            "fn brighten(x: f32) -> f32 { return x + ; }",
            "embedded://bevy_pbr/render/lighting.wesl",
        );
        cache.set_shader(maths_id, maths);
        cache.set_shader(lighting_id, broken_lighting);
        cache.set_shader(root_id, root);

        let error = cache
            .get(0, root_id, &[ShaderDefVal::Bool("BRIGHT".into(), true)])
            .expect_err("syntax error");
        let ShaderCacheError::ProcessShaderError(message) = error else {
            panic!("expected ProcessShaderError, got: {error:?}");
        };
        assert!(message.contains("embedded://bevy_pbr/render/lighting.wesl"));
    }

    #[test]
    fn import_retry() {
        let mut cache = test_cache();
        let (maths, lighting, root) = test_shaders();
        let (maths_id, lighting_id, root_id) = test_ids();

        cache.set_shader(root_id, root);
        assert!(matches!(
            cache.get(0, root_id, &[]),
            Err(ShaderCacheError::ShaderImportNotYetAvailable)
        ));

        cache.set_shader(lighting_id, lighting);
        cache.set_shader(maths_id, maths);
        cache.get(0, root_id, &[]).unwrap();
    }

    fn test_shaders() -> (Shader, Shader, Shader) {
        let maths = Shader::from_wesl(
            "fn double(x: f32) -> f32 { return x * 2.0; }",
            "embedded://bevy_render/maths.wesl",
        );
        let lighting = Shader::from_wesl(
            "fn brighten(x: f32) -> f32 { return x + 0.1; }",
            "embedded://bevy_pbr/render/lighting.wesl",
        );
        let root = Shader::from_wesl(
            r#"
import bevy_pbr::render::lighting::brighten;

@fragment
fn fragment() -> @location(0) vec4<f32> {
    var value = bevy_render::maths::double(1.0);
    @if(BRIGHT) { value = brighten(value); }
    return vec4<f32>(value);
}
"#,
            "shaders/root.wesl",
        );
        (maths, lighting, root)
    }

    // NVForever modules off the lock: two asks of one module make it once, the second
    // waiting on the first's slot; a module made from a source replaced meanwhile is not kept.
    #[test]
    fn a_module_asked_twice_at_once_is_made_once_and_a_stale_one_is_not_kept() {
        let mut cache = test_cache();
        let (_, lighting_id, root_id) = set_test_shaders(&mut cache);
        let bright = [ShaderDefVal::Bool("BRIGHT".into(), true)];
        let ModuleStep::Build(build) = cache.prepare(Some(0), root_id, &bright).unwrap() else {
            panic!("first ask: made by the caller");
        };
        let ModuleStep::Wait(slot) = cache.prepare(Some(1), root_id, &bright).unwrap() else {
            panic!("second ask while the first makes it: waits");
        };
        let ShaderCacheSource::Wgsl(source) = build.source() else { panic!("wgsl") };
        let made = cache.finish(build, Ok(source)).unwrap();
        assert_eq!(slot.wait().unwrap(), made, "the waiter gets the same module");
        assert!(
            matches!(cache.prepare(Some(2), root_id, &bright).unwrap(), ModuleStep::Ready(m) if Arc::ptr_eq(&m, &made)),
            "kept once made"
        );

        let ModuleStep::Build(build) = cache.prepare(Some(3), root_id, &[]).unwrap() else {
            panic!("other definitions: made");
        };
        let (_, lighting, _) = test_shaders();
        cache.set_shader(lighting_id, lighting);
        let ShaderCacheSource::Wgsl(source) = build.source() else { panic!("wgsl") };
        cache.finish(build, Ok(source)).unwrap();
        assert!(
            matches!(cache.prepare(Some(4), root_id, &[]).unwrap(), ModuleStep::Build(_)),
            "an import was replaced while it was made: made again, not kept"
        );
    }

    #[test]
    fn a_build_dropped_unfinished_wakes_its_waiters() {
        let mut cache = test_cache();
        let (_, _, root_id) = set_test_shaders(&mut cache);
        let ModuleStep::Build(build) = cache.prepare(Some(0), root_id, &[]).unwrap() else { panic!() };
        let ModuleStep::Wait(slot) = cache.prepare(Some(1), root_id, &[]).unwrap() else { panic!() };
        drop(build);
        assert!(
            matches!(slot.wait(), Err(ShaderCacheError::ShaderImportNotYetAvailable)),
            "the waiter gets an error its pipeline retries, not a final one"
        );
        assert!(
            matches!(cache.prepare(Some(2), root_id, &[]).unwrap(), ModuleStep::Build(_)),
            "the next ask makes it again"
        );
    }

    #[test]
    fn shaders_are_found_by_a_path_only_they_have() {
        let mut cache = test_cache();
        let (maths_id, _, root_id) = set_test_shaders(&mut cache);
        assert_eq!(cache.shader_path(root_id), Some("shaders/root.wesl"));
        assert_eq!(cache.shader_by_path("shaders/root.wesl"), Some(root_id));
        let before = cache.shaders_set();
        let other = AssetId::Uuid { uuid: bevy_asset::uuid::Uuid::from_u128(9) };
        let (_, _, root) = test_shaders();
        cache.set_shader(other, root);
        assert!(cache.shaders_set() > before);
        assert_eq!(cache.shader_path(root_id), None, "two shaders share the path: neither");
        assert_eq!(cache.shader_by_path("shaders/root.wesl"), None);
        assert!(cache.shader_path(maths_id).is_some());
    }

    fn test_ids() -> (AssetId<Shader>, AssetId<Shader>, AssetId<Shader>) {
        let id = |n| AssetId::Uuid {
            uuid: bevy_asset::uuid::Uuid::from_u128(n),
        };
        (id(1), id(2), id(3))
    }

    #[test]
    fn library_def_scoping() {
        let mut cache = test_cache();
        let id = |n| AssetId::Uuid {
            uuid: bevy_asset::uuid::Uuid::from_u128(n),
        };

        let mut lib_a = Shader::from_wesl(
            "var<uniform> batch_a: array<vec4<f32>, constants::BATCH_SIZE>;",
            "embedded://bevy_a/bindings.wesl",
        );
        lib_a.shader_defs = vec![ShaderDefVal::UInt("BATCH_SIZE".into(), 3)];
        let mut lib_b = Shader::from_wesl(
            "var<uniform> batch_b: array<vec4<f32>, constants::BATCH_SIZE>;",
            "embedded://bevy_b/bindings.wesl",
        );
        lib_b.shader_defs = vec![ShaderDefVal::UInt("BATCH_SIZE".into(), 7)];

        let root_a = Shader::from_wesl(
            r#"
import bevy_a::bindings::batch_a;
@fragment
fn fragment() -> @location(0) vec4<f32> { return batch_a[0]; }
"#,
            "shaders/root_a.wesl",
        );
        let root_b = Shader::from_wesl(
            r#"
import bevy_b::bindings::batch_b;
@fragment
fn fragment() -> @location(0) vec4<f32> { return batch_b[0]; }
"#,
            "shaders/root_b.wesl",
        );

        cache.set_shader(id(1), lib_a);
        cache.set_shader(id(2), lib_b);
        cache.set_shader(id(3), root_a);
        cache.set_shader(id(4), root_b);

        let compiled_a = cache.get(0, id(3), &[]).unwrap();
        let compiled_b = cache.get(1, id(4), &[]).unwrap();
        assert!(compiled_a.contains("= 3;") && !compiled_a.contains("= 7;"));
        assert!(compiled_b.contains("= 7;") && !compiled_b.contains("= 3;"));
    }

    #[test]
    fn cyclic_import_invalidation() {
        let mut cache = test_cache();
        let id = |n| AssetId::Uuid {
            uuid: bevy_asset::uuid::Uuid::from_u128(n),
        };
        let module_a = Shader::from_wesl(
            "import bevy_cycle::b::from_b;\nfn from_a() -> f32 { return 1.0; }",
            "embedded://bevy_cycle/a.wesl",
        );
        let module_b = Shader::from_wesl(
            "import bevy_cycle::a::from_a;\nfn from_b() -> f32 { return 2.0; }",
            "embedded://bevy_cycle/b.wesl",
        );
        cache.set_shader(id(1), module_a);
        cache.set_shader(id(2), module_b);
        let module_a = Shader::from_wesl(
            "import bevy_cycle::b::from_b;\nfn from_a() -> f32 { return 3.0; }",
            "embedded://bevy_cycle/a.wesl",
        );
        cache.set_shader(id(1), module_a);
    }

    // NVForever composed shaders: a second cache (a later run) composes nothing that the
    // first kept, and composes again what read a module whose source changed.
    #[test]
    fn composed_store_skips_compositions_of_unchanged_sources() {
        use core::sync::atomic::Ordering::Relaxed;
        let bright = [ShaderDefVal::Bool("BRIGHT".into(), true)];
        let first = Arc::new(ComposedStore::from_bytes(&[]));
        let mut cache = test_cache();
        cache.set_composed_store(Some(first.clone()));
        let (_, _, root_id) = set_test_shaders(&mut cache);
        let composed = cache.get(0, root_id, &bright).unwrap();
        assert_eq!(first.misses.load(Relaxed), 1);

        let second = Arc::new(ComposedStore::from_bytes(&first.to_bytes()));
        let mut cache = test_cache();
        cache.set_composed_store(Some(second.clone()));
        let (_, lighting_id, root_id) = set_test_shaders(&mut cache);
        assert_eq!(cache.get(0, root_id, &bright).unwrap(), composed);
        assert_eq!(
            (second.hits.load(Relaxed), second.misses.load(Relaxed)),
            (1, 0)
        );
        assert!(
            cache.get(1, root_id, &[]).unwrap() != composed,
            "other definitions: composed"
        );
        assert_eq!(second.misses.load(Relaxed), 1);

        let lighting = Shader::from_wesl(
            "fn brighten(x: f32) -> f32 { return x + 0.25; }",
            "embedded://bevy_pbr/render/lighting.wesl",
        );
        cache.set_shader(lighting_id, lighting);
        let changed = cache.get(0, root_id, &bright).unwrap();
        assert!(changed.contains("+ 0.25"), "an imported module changed: composed again");
        assert_eq!(second.misses.load(Relaxed), 2);
    }

    fn set_test_shaders(
        cache: &mut ShaderCache<String, ()>,
    ) -> (AssetId<Shader>, AssetId<Shader>, AssetId<Shader>) {
        let (maths, lighting, root) = test_shaders();
        let (maths_id, lighting_id, root_id) = test_ids();
        cache.set_shader(maths_id, maths);
        cache.set_shader(lighting_id, lighting);
        cache.set_shader(root_id, root);
        (maths_id, lighting_id, root_id)
    }
}
