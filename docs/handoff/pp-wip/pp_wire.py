import sys
sys.path.insert(0, r'C:\fnv-private\scratch\nvse-one111pp')
from ed import edit

R = r'C:\Users\Loot\source\repos\nvf-nvse-one111pp\crates'
G = R + r'\fnv-game\src'

edit(G + r'\lib.rs', 'pub mod nvse_jip5;\n', 'pub mod nvse_jip5;\npub mod nvse_pp;\n')
edit(G + r'\nvse.rs', '''    if let Some(r) = crate::nvse_jip5::call(g, ctx, name, subject, args) {
        return Some(r);
    }
''', '''    if let Some(r) = crate::nvse_jip5::call(g, ctx, name, subject, args) {
        return Some(r);
    }
    if let Some(r) = crate::nvse_pp::call(g, name, subject, args) {
        return Some(r);
    }
''')

# game.rs: the state, the "value set" flag, the tick, the snapshot, the session,
# EvaluateString.
edit(G + r'\game.rs', '''    /// The INI settings the extender reads and sets (`crate::nvse_settings`).
    pub ini: crate::nvse_settings::IniState,
''', '''    /// The INI settings the extender reads and sets (`crate::nvse_settings`).
    pub ini: crate::nvse_settings::IniState,
    /// ppNVSE's AX variables, form instances, item-mod slots and call loops
    /// (`crate::nvse_pp`).
    pub pp: crate::nvse_pp::PpState,
    /// Whether the last [`Game::run_function`] call's function set its value
    /// (SetFunctionValue), for callers that tell "set to 0" from "not set" (CallLoop).
    pub(crate) udf_value_set: bool,
''')
edit(G + r'\game.rs', '''            ini: Default::default(),
''', '''            ini: Default::default(),
            pp: Default::default(),
            udf_value_set: false,
''')
edit(G + r'\game.rs', '''        let frame = self.udf.pop();
        let v = match lambda_value {''', '''        let frame = self.udf.pop();
        self.udf_value_set = frame.as_ref().is_some_and(|f| f.value.is_some());
        let v = match lambda_value {''')
edit(G + r'\game.rs', '''        self.run_main_loop();
        self.run_delayed();
''', '''        self.run_main_loop();
        self.run_delayed();
        // ppNVSE's CallLoop functions due (`crate::nvse_pp`).
        self.run_pp_loops();
''')
edit(G + r'\game.rs', '''    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub door_open_seen: BTreeMap<FormId, bool>,
}''', '''    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub door_open_seen: BTreeMap<FormId, bool>,
    /// ppNVSE's form instances [documented: CreateFormInstance, "tied to the users save
    /// game"] (`crate::nvse_pp`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form_instances: Vec<(FormId, crate::nvse_pp::Instance)>,
}''')
edit(G + r'\game.rs', '''            door_open_seen: self.door_open_seen.iter().filter(|&(&r, &seen)| self.world.peek(r).is_some_and(|s| s.open != seen)).map(|(&r, &seen)| (r, seen)).collect(),
        }''', '''            door_open_seen: self.door_open_seen.iter().filter(|&(&r, &seen)| self.world.peek(r).is_some_and(|s| s.open != seen)).map(|(&r, &seen)| (r, seen)).collect(),
            form_instances: self.pp.instances.iter().map(|(k, v)| (*k, v.clone())).collect(),
        }''')
edit(G + r'\game.rs', '''        g.door_open_seen = s.door_open_seen.clone();
''', '''        g.door_open_seen = s.door_open_seen.clone();
        g.pp_restore_instances(&s.form_instances);
''')
edit(G + r'\game.rs', '''    restarted_seen: BTreeSet<String>,
    handler_seq: u64,
}''', '''    restarted_seen: BTreeSet<String>,
    handler_seq: u64,
    /// ppNVSE's instance and item-mod handlers, and the instances to deconstruct.
    pp: crate::nvse_pp::PpSession,
}''')
edit(G + r'\game.rs', '''        Session { handlers, main_loop, restarted_seen: std::mem::take(&mut self.restarted_seen), handler_seq: self.handler_seq }''', '''        let pp = self.pp_take_session();
        Session { handlers, main_loop, restarted_seen: std::mem::take(&mut self.restarted_seen), handler_seq: self.handler_seq, pp }''')
edit(G + r'\game.rs', '''        self.restarted_seen = s.restarted_seen;
        self.handler_seq = s.handler_seq;
''', '''        self.restarted_seen = s.restarted_seen;
        self.handler_seq = s.handler_seq;
        self.pp_put_session(s.pp);
''')
edit(G + r'\game.rs', '''        let bytes = crate::mod_files::read_data(&format!("nvse\\\\user_defined_functions\\\\{key}"))?;
        let src = String::from_utf8_lossy(&bytes).into_owned();
        let c = fnv_script::compile::compile(&src, &self.data.table, &GameEnv { g: self }, None);
        if let Some(e) = c.errors().next() {
            let what = format!("CompileScript {path}: {}: {}", e.kind.replace('_', " "), e.detail);
            self.emit(EventKind::Skipped { unit: "CompileScript".into(), what });
            return None;
        }''', '''        let bytes = crate::mod_files::read_data(&format!("nvse\\\\user_defined_functions\\\\{key}"))?;
        let src = String::from_utf8_lossy(&bytes).into_owned();
        let id = self.compile_source(&format!("CompileScript {path}"), src)?;
        self.compiled_paths.insert(key, id);
        Some(id)
    }

    /// EvaluateString: whether `text` holds as an `if` condition [documented:
    /// EvaluateString]; each text is compiled once, as a function script.
    pub(crate) fn evaluate_string(&mut self, text: &str) -> Result<bool, String> {
        let key = format!("\\u{1}evaluate:{text}");
        let id = match self.compiled_paths.get(&key) {
            Some(&id) => id,
            None => {
                let src = format!("Begin Function {{ }}\\nif {text}\\nSetFunctionValue 1\\nendif\\nEnd\\n");
                let id = self.compile_source("EvaluateString", src).ok_or_else(|| format!("{text:?} does not compile"))?;
                self.compiled_paths.insert(key, id);
                id
            }
        };
        Ok(self.run_function(&fnv_script::vm::XVal::Ref(id), None, Vec::new(), None, RunCtx::default())?.truthy())
    }

    /// A function script from source text, kept as CompileScript keeps its files; `None`
    /// when it does not compile (the compiler's words go to the trace).
    fn compile_source(&mut self, what: &str, src: String) -> Option<u32> {
        let c = fnv_script::compile::compile(&src, &self.data.table, &GameEnv { g: self }, None);
        if let Some(e) = c.errors().next() {
            let what = format!("{what}: {}: {}", e.kind.replace('_', " "), e.detail);
            self.emit(EventKind::Skipped { unit: "CompileScript".into(), what });
            return None;
        }''')
edit(G + r'\game.rs', '''        let id = COMPILED_BASE + self.compiled_files.len() as u32;
        self.compiled_files.push(unit);
        self.compiled_paths.insert(key, id);
        Some(id)''', '''        let id = COMPILED_BASE + self.compiled_files.len() as u32;
        self.compiled_files.push(unit);
        Some(id)''')

# The VM: the variables GetPlayerCameraRotation and GetAkimboWeaponsAlt write.
edit(R + r'\fnv-script\src\vm.rs', '''        "getstringuidimensions" => &[3, 4, 5],
''', '''        "getstringuidimensions" => &[3, 4, 5],
        // [documented: GetPlayerCameraRotation, "store the players camera rotation in fx fy
        // fz"; GetAkimboWeaponsAlt, the left and right weapons into the two variables].
        "getplayercamerarotation" => &[0, 1, 2],
        "getakimboweaponsalt" => &[1, 2],
''')

edit(G + r'\mod_files.rs', '''        // ---- LU: read the file each time; write it at once ----''', '''        // ---- ppNVSE's line files ----
        // `path [start] [end]`: the file's lines as strings, from `start` (0 the first) up
        // to before `end` [documented: pReadFile, "0 5" reads the first five lines;
        // hypothesis: the end left out], the path under the game's folder as
        // ReadArrayFromFile's [documented: pWriteFile, "relative to the game's
        // installation path"]. A missing file gives an empty array [hypothesis].
        "preadfile" => {
            let path = text(args, 0).unwrap_or_default();
            let lines: Vec<String> = g.mod_files.read_root(&path).map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default();
            let start = num(args, 1, 0.0).max(0.0) as usize;
            let end = match args.get(2) {
                Some(XVal::Num(e)) => e.max(0.0) as usize,
                _ => lines.len(),
            }
            .min(lines.len());
            let entries = lines.into_iter().skip(start).take(end.saturating_sub(start)).enumerate().map(|(i, l)| (XKey::Num(i as f64), XVal::Str(l))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // `path array [append]`: one line per element [documented: pWriteFile], into
        // NVForever's own layer, never the game's folder (`write_root`); pWriteFileAlt's
        // path is under Data [documented: pWriteFileAlt, "relative to the game's DATA
        // path"; hypothesis: the calling mod's staging folder is its place in Data]. 1
        // when written, 0 without a path or an array [documented: pWriteFileAlt].
        "pwritefile" | "pwritefilealt" => {
            let path = text(args, 0).unwrap_or_default();
            let Some(XVal::Array(id)) = args.get(1) else { return Some(Ok(zero)) };
            if path.trim().is_empty() {
                return Some(Ok(zero));
            }
            let lines: Vec<String> = g.ext.array(*id).map(|a| a.entries.iter().map(|(_, v)| cell_text(v)).collect()).unwrap_or_default();
            let append = num(args, 2, 0.0) != 0.0;
            let body = lines.join("\n");
            let alt = name == "pwritefilealt";
            let old = if alt { g.mod_files.read(&path) } else { g.mod_files.read_root(&path) };
            let text = match old.filter(|_| append) {
                Some(o) if !o.is_empty() => format!("{o}\n{body}"),
                _ => body,
            };
            if alt {
                g.mod_files.write(&path, text);
            } else {
                g.mod_files.write_root(&path, text);
            }
            XVal::Num(1.0)
        }
        // ---- LU: read the file each time; write it at once ----''')
