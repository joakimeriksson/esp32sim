//! Opt-in statistical block profiling. No instrumentation is present in production builds.
//! Uniform pseudorandom samples avoid aliasing with periodic guest loops. Timings include
//! block lookup/decoding and dispatch, but not the outer SoC scheduler. Browser clock
//! quantization and timer-call overhead make these estimates, not a CPU flame graph.
use super::{emitter, BlockInsn};
use std::collections::HashMap;
use std::fmt::Write;

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_profile_now() -> f64;
}

pub fn now() -> f64 {
    // SAFETY: the profiling host provides a monotonic clock in milliseconds.
    unsafe { host_profile_now() }
}

#[derive(Default)]
struct Row {
    samples: u64,
    instructions: u64,
    ms: f64,
    ops: String,
    missing: String,
}

pub struct Profile {
    rng: u32,
    calls: u64,
    rows: HashMap<(u32, bool), Row>,
    loops: HashMap<u32, (u64, u64)>,
}

impl Default for Profile {
    fn default() -> Self {
        Self { rng: 0x914f_7ab3, calls: 0, rows: HashMap::new(), loops: HashMap::new() }
    }
}

impl Profile {
    pub fn sample(&mut self) -> bool {
        self.calls += 1;
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng & 4095 == 0
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(&mut self, pc: u32, jit: bool, done: u32, ms: f64, ops: &[BlockInsn], fast: bool) {
        let row = self.rows.entry((pc, jit)).or_insert_with(|| Row {
            ops: ops.iter().map(|i| format!("{:?}", i.insn.op)).collect::<Vec<_>>().join(","),
            missing: ops.iter().filter(|i| !emitter::supported_insn(&i.insn, fast))
                .map(|i| format!("{:?}", i.insn.op)).collect::<Vec<_>>().join(","),
            ..Row::default()
        });
        row.samples += 1;
        row.instructions += done as u64;
        row.ms += ms;
    }

    /// Count completed retained backedges once per dispatch, never in generated loops.
    pub fn record_loop(&mut self, pc: u32, retained: u32) {
        if retained == 0 { return; }
        let row = self.loops.entry(pc).or_default();
        row.0 += 1;
        row.1 += retained as u64;
    }

    pub fn report(&self) -> String {
        let mut text = format!("[wasm-profile] calls={} sample_probability=1/4096\n", self.calls);
        let mut loops: Vec<_> = self.loops.iter().collect();
        loops.sort_by_key(|(pc, _)| **pc);
        for (pc, (calls, backedges)) in loops {
            writeln!(text, "[wasm-loop] pc={pc:08x} calls={calls} retained_backedges={backedges}").unwrap();
        }
        for jit in [false, true] {
            let rows: Vec<_> = self.rows.iter().filter(|((_, j), _)| *j == jit).collect();
            let samples: u64 = rows.iter().map(|(_, r)| r.samples).sum();
            let instructions: u64 = rows.iter().map(|(_, r)| r.instructions).sum();
            let ms: f64 = rows.iter().map(|(_, r)| r.ms).sum();
            writeln!(text, "jit={jit} samples={samples} sampled_instructions={instructions} sampled_ms={ms:.6}").unwrap();
        }
        let mut rows: Vec<_> = self.rows.iter().collect();
        rows.sort_by(|a, b| b.1.instructions.cmp(&a.1.instructions).then(a.0.cmp(b.0)));
        writeln!(text, "pc\tjit\tsamples\tinstructions\tms\tmissing\tops").unwrap();
        for ((pc, jit), r) in rows {
            writeln!(text, "{pc:08x}\t{jit}\t{}\t{}\t{:.6}\t{}\t{}", r.samples, r.instructions, r.ms, r.missing, r.ops).unwrap();
        }
        text
    }
}
