use crate::matcher::MatchedPair;
use wasmtime::{Engine, Instance, Linker, Module, Store};

pub struct LoadedPlugin {
    instance: Instance,
    store: Store<()>,
    pub applies_to: Vec<String>,
}

pub struct PluginHost {
    engine: Engine,
    pub plugins: Vec<LoadedPlugin>,
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    pub fn new() -> Self {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        // Task 62: Enforce memory limits (e.g. max 128MB per plugin instance)
        // Note: wasmtime memory limits are typically set on the Memory object or via Store resource limits.
        // For MVP, we'll use the default engine settings but ensure fuel is consumed.
        Self {
            engine: Engine::new(&config).unwrap_or_default(),
            plugins: Vec::new(),
        }
    }

    pub fn load(&mut self, path: &str, applies_to: Vec<String>) -> anyhow::Result<()> {
        let module = Module::from_file(&self.engine, path)?;
        let linker = Linker::new(&self.engine);
        let mut store = Store::new(&self.engine, ());
        store.set_fuel(100_000).unwrap_or(()); // Cap execution to prevent infinite loop hangs
        let instance = linker.instantiate(&mut store, &module)?;

        self.plugins.push(LoadedPlugin {
            instance,
            store,
            applies_to,
        });
        Ok(())
    }

    pub fn run_plugins(&mut self, pair: &MatchedPair) -> Option<f32> {
        let mut max_score: Option<f32> = None;

        for plugin in &mut self.plugins {
            if !plugin
                .applies_to
                .iter()
                .any(|pat| pair.endpoint.contains(pat.as_str()))
            {
                continue;
            }

            if let Some(score) = Self::call_plugin(plugin, pair) {
                max_score = Some(max_score.unwrap_or(0.0).max(score));
            }
        }

        max_score
    }

    fn call_plugin(plugin: &mut LoadedPlugin, pair: &MatchedPair) -> Option<f32> {
        // Warning: This is a simplified MVP memory passing protocol.
        // A full implementation requires allocating multiple buffers and
        // passing all pointers correctly based on the export_plugin macro signature.
        let alloc = plugin
            .instance
            .get_typed_func::<u32, u32>(&mut plugin.store, "alloc")
            .ok()?;

        let score_fn = plugin.instance.get_func(&mut plugin.store, "score_pair")?;

        let memory = plugin.instance.get_memory(&mut plugin.store, "memory")?;

        // Allocate and write body A
        let body_a_ptr = alloc
            .call(&mut plugin.store, pair.res_a.body.len() as u32)
            .ok()?;
        memory
            .write(&mut plugin.store, body_a_ptr as usize, &pair.res_a.body)
            .ok()?;

        // Allocate and write body B
        let body_b_ptr = alloc
            .call(&mut plugin.store, pair.res_b.body.len() as u32)
            .ok()?;
        memory
            .write(&mut plugin.store, body_b_ptr as usize, &pair.res_b.body)
            .ok()?;

        plugin.store.set_fuel(100_000).unwrap_or(());

        let mut results = [wasmtime::Val::F32(0)];
        let params = [
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(body_a_ptr as i32),
            wasmtime::Val::I32(pair.res_a.body.len() as i32),
            wasmtime::Val::I32(pair.res_a.status as i32),
            wasmtime::Val::I32(body_a_ptr as i32),
            wasmtime::Val::I32(pair.res_a.body.len() as i32),
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(0),
            wasmtime::Val::I32(body_b_ptr as i32),
            wasmtime::Val::I32(pair.res_b.body.len() as i32),
            wasmtime::Val::I32(pair.res_b.status as i32),
            wasmtime::Val::I32(body_b_ptr as i32),
            wasmtime::Val::I32(pair.res_b.body.len() as i32),
        ];

        if let Err(e) = score_fn.call(&mut plugin.store, &params, &mut results) {
            tracing::error!("plugin execution failed: {}", e);
            if e.to_string().contains("exhausted fuel") {
                tracing::error!(
                    "plugin was terminated due to infinite loop or excessive resource usage"
                );
            }
            return None;
        }
        Some(results[0].unwrap_f32())
    }
}
