//! Binary WASM emitter for an Xtensa block, with budgeted hardware-loop prefixes.
use super::*;
#[path = "wasm_float.rs"]
mod float;
#[path = "wasm_region.rs"]
pub(super) mod region;
#[path = "wasm_pie.rs"]
mod pie;
use region::{region_edge, RegionGen};
#[path = "wasm_policy.rs"]
mod policy;
#[path = "wasm_memory.rs"]
mod memory;
#[path = "wasm_instruction.rs"]
mod instruction;
pub(super) use policy::{admitted, supported_insn, loop_safe, terminal_helper, rsr_field};
#[cfg(feature = "wasm-jit-tests")]
pub(super) use policy::supported_opcode;
use policy::coprocessors;

// Parameters: cpu, bus, helpers, budget, entry, TLB, versions.
// Locals: done, windowbase*4, scratch, guest address, TLB entry, relative offset.
const DONE: u8 = 7;
const WB: u8 = 8;
const TMP: u8 = 9;
const ADDR: u8 = 10;
const TLB: u8 = 11;
const REL: u8 = 12;
const WINDOWS: u8 = 29;
/// Region locals: a helper or code-page store happened (leave at the next head); next chunk.
const DIRTY: u8 = 30;
const NEXT: u8 = 31;
#[cfg(feature = "wasm-cache-inline")]
const CACHE: u8 = 32;
#[cfg(feature = "wasm-cache-inline")]
const CACHE_TAG: u8 = 33;
#[cfg(feature = "wasm-cache-inline")]
const CACHE_SET: u8 = 34;
#[cfg(feature = "wasm-cache-inline")]
const CACHE_LINE: u8 = 35;
/// Typed scratch locals declared after the i32 locals: a vector and two 64-bit integers
/// (PIE lane sums, the 40-bit ACCX scratch and EX178's held accumulator). `module` must
/// declare them in this order.
const V128: u8 = if cfg!(feature = "wasm-cache-inline") { 36 } else { 32 };
const WIDE: u8 = V128 + 1;
/// EX178: the 40-bit ACCX itself, held across a run of accumulates that cannot saturate.
const ACC: u8 = WIDE + 1;
/// EX156 guarded body: the first static index that must not run (entry + credit).
const STOP: u8 = ACC + 1;
/// EX178 s1: the host pointer a coalesced run of PIE vector loads reads through.
const HOSTP: u8 = STOP + 1;
const PC: usize = offset_of!(Cpu, pc);
const AR: usize = offset_of!(Cpu, ar);
const WINDOWBASE: usize = offset_of!(Cpu, windowbase);
const LCOUNT: usize = offset_of!(Cpu, lcount);
const LEND: usize = offset_of!(Cpu, lend);
const LBEG: usize = offset_of!(Cpu, lbeg);
const SAR: usize = offset_of!(Cpu, sar);
const ACCX: usize = offset_of!(Cpu, accx);

/// Structured control nesting the emitter is inside of. Each construct remembers the
/// statically pending retirement count at its start; the code after its `end` resumes
/// that count. A path inside that retires more must therefore flush it into DONE before
/// falling out, or leave (return or branch) instead.
enum Ctl { If(u32), Block(u32), Loop(u32) }

#[derive(Default)]
struct Gen {
    /// The next `leave` is a skipped LOOPNEZ/LOOPGTZ body: its setup price already covers it.
    free_leave: bool,
    /// EX141: the instruction being emitted has a static target that straddles a fetch word
    straddle: bool,
    /// Static wait prepaid for the current instruction, refundable on helper failure.
    wait_price: u32,
    /// module body bytes
    bytes: Vec<u8>,
    /// registers written so far (spilled on exit)
    written: u16,
    /// registers loaded at entry
    loaded: u16,
    /// Retired instructions not yet added to the DONE local. In a static body (whole
    /// path) DONE is never materialized: every exit returns a constant count.
    pending: u32,
    /// DONE is a runtime value (checked body, loops, regions)
    dynamic: bool,
    ctl: Vec<Ctl>,
    /// Highest AR index any instruction touches; below 4 no window collision is possible.
    max_ar: u8,
    /// Region emission state, when compiling several chunks into one function.
    region: Option<RegionGen>,
    /// PC of the most recently emitted guest instruction, for exit-site attribution.
    last_pc: u32,
    /// EX156: emitting the guarded body; with a loop site (instruction count of the
    /// repeated prefix, control depth just inside the repeat loop) when LEND is hinted.
    guarded: bool,
    guard_site: Option<(usize, usize)>,
    /// tails-s1: in a region's guarded copy, the control depth of the block whose end spills
    /// and returns for every cut (EX182 s3), so the spill is emitted once per copy.
    cut_target: Option<usize>,
    /// EX178: this body is straight line from its head, so a run of PIE accumulates can
    /// keep ACCX in a local. False for the guarded body (every index is an entry label)
    /// and the checked body (a cut may land between two instructions of a run).
    accx_ok: bool,
    /// ACC holds the architectural ACCX; the copy in memory is stale until spilled.
    accx_live: bool,
    /// Largest magnitude the held accumulator can have reached since its `ee.zero.accx`.
    accx_head: i64,
    #[cfg(feature = "wasm-jit-profile")]
    last_kind: ExitKind,
}
impl Gen {
    /// Runtime reachability receipt; absent from production modules.
    #[cfg(feature = "wasm-jit-tests")]
    fn test_hit(&mut self, counter: &std::sync::atomic::AtomicU32) {
        self.c(counter.as_ptr() as u32);
        self.c(counter.as_ptr() as u32);
        self.load(0);
        self.c(1);
        self.op(0x6a);
        self.store(0);
    }
    fn op(&mut self, op: u8) {
        self.bytes.push(op);
    }
    fn c(&mut self, v: u32) {
        self.op(0x41);
        sleb(&mut self.bytes, v as i32);
    }
    fn c64(&mut self, mut n: i64) {
        self.op(0x42);
        loop {
            let b = (n as u8) & 127;
            n >>= 7;
            let done = (n == 0 && b & 64 == 0) || (n == -1 && b & 64 != 0);
            self.bytes.push(b | if done { 0 } else { 128 });
            if done {
                break;
            }
        }
    }
    /// EX178: write a held ACCX back exactly as `pie::accx_set` does (the low word, then
    /// bits 32..40). The local stays authoritative: paths that leave emit this and return,
    /// while the emitter keeps writing the fall-through path after them.
    fn accx_spill(&mut self) {
        if !self.accx_live {
            return;
        }
        self.get(0);
        self.get(ACC);
        self.op(0x3e); // i64.store32
        uleb(&mut self.bytes, 2);
        uleb(&mut self.bytes, ACCX);
        self.get(0);
        self.get(ACC);
        self.c64(32);
        self.op(0x87); // i64.shr_s
        self.c64(0xff);
        self.op(0x83); // i64.and
        self.op(0x3e);
        uleb(&mut self.bytes, 2);
        uleb(&mut self.bytes, ACCX + 4);
    }
    /// Memory becomes authoritative again: before anything that may read ACCX, at a join
    /// and at every chunk boundary, where another path could arrive with a stale local.
    fn accx_flush(&mut self) {
        self.accx_spill();
        self.accx_live = false;
    }
    fn get(&mut self, n: u8) {
        self.bytes.extend([0x20, n]);
    }
    fn set(&mut self, n: u8) {
        self.bytes.extend([0x21, n]);
    }
    fn tee(&mut self, n: u8) {
        self.bytes.extend([0x22, n]);
    }
    fn load(&mut self, offset: usize) {
        self.op(0x28);
        uleb(&mut self.bytes, 2);
        uleb(&mut self.bytes, offset);
    }
    fn store(&mut self, offset: usize) {
        self.op(0x36);
        uleb(&mut self.bytes, 2);
        uleb(&mut self.bytes, offset);
    }
    fn cpu(&mut self, off: usize) {
        self.get(0);
        self.load(off);
    }
    fn cpu_const(&mut self, off: usize, v: u32) {
        self.get(0);
        self.c(v);
        self.store(off);
    }
    fn ar_addr(&mut self, r: u8) {
        self.get(0);
        self.get(WB);
        self.c(r as u32);
        self.op(0x6a);
        self.c(63);
        self.op(0x71);
        self.c(2);
        self.op(0x74);
        self.op(0x6a);
    }
    fn fr(&mut self, r: u8) {
        self.cpu(offset_of!(Cpu, fr) + 4 * r as usize);
    }
    fn float(&mut self, r: u8) {
        self.fr(r);
        self.op(0xbe); // f32.reinterpret_i32 preserves register bits.
    }
    fn boolean(&mut self, r: u8) {
        self.cpu(offset_of!(Cpu, br));
        self.c(1 << r);
        self.op(0x71);
    }
    fn ar(&mut self, r: u8) {
        self.get(13 + r);
    }
    fn set_ar(&mut self, r: u8) {
        self.set(13 + r);
        self.written |= 1 << r;
    }
    fn reload(&mut self) {
        self.cpu(WINDOWBASE);
        self.c(2);
        self.op(0x74);
        self.set(WB);
        for r in 0..16 {
            if self.loaded & (1 << r) != 0 {
                self.ar_addr(r);
                self.load(AR);
                self.set(13 + r);
            }
        }
        if self.max_ar < 4 {
            return;
        }
        self.c(0);
        self.set(WINDOWS);
        self.cpu(offset_of!(Cpu, ps));
        self.c(ps::WOE | ps::EXCM);
        self.op(0x71);
        self.c(ps::WOE);
        self.op(0x46);
        self.begin_if();
        self.cpu(WINDOWBASE);
        self.c(1);
        self.op(0x6a);
        self.c(15);
        self.op(0x71);
        self.set(TMP);
        self.cpu(offset_of!(Cpu, windowstart));
        self.get(TMP);
        self.op(0x76);
        self.cpu(offset_of!(Cpu, windowstart));
        self.c(16);
        self.get(TMP);
        self.op(0x6b);
        self.op(0x74);
        self.op(0x72);
        self.set(WINDOWS);
        self.end();
    }
    fn spill(&mut self) {
        self.accx_spill();
        for r in 0..16 {
            if self.written & (1 << r) != 0 {
                self.ar_addr(r);
                self.get(13 + r);
                self.store(AR);
            }
        }
    }
    fn begin_if(&mut self) {
        self.bytes.extend([0x04, 0x40]);
        self.ctl.push(Ctl::If(self.pending));
    }
    fn begin_block(&mut self) {
        self.bytes.extend([0x02, 0x40]);
        self.ctl.push(Ctl::Block(self.pending));
    }
    fn begin_loop(&mut self) {
        self.bytes.extend([0x03, 0x40]);
        self.ctl.push(Ctl::Loop(self.pending));
    }
    fn end(&mut self) {
        self.op(0x0b);
        if let Some(Ctl::If(pending) | Ctl::Block(pending) | Ctl::Loop(pending)) = self.ctl.pop() {
            self.pending = pending;
        }
    }
    fn depth(&self) -> usize {
        self.ctl.len()
    }
    fn ret(&mut self, code: u32) {
        self.spill();
        self.ret_value(code);
    }
    /// Exit tag: the code, and for regions the exit site so the caller can attribute
    /// the last retired instruction without a per-instruction PC store.
    fn tag(&mut self, code: u32) -> u32 {
        let site = match &mut self.region {
            Some(r) => {
                #[cfg(not(feature = "wasm-jit-profile"))]
                r.sites.push((self.last_pc, NONE));
                #[cfg(feature = "wasm-jit-profile")]
                r.sites.push((self.last_pc, self.last_kind, NONE));
                (r.sites.len() - 1) as u32
            }
            None => 0,
        };
        assert!(site < (1 << 13), "region exit site exceeds the result tag");
        (code << 16) | (site << 19)
    }
    /// lane-s1: the region parameter that resumes at the exit PC of the site just tagged.
    fn resume_at(&mut self, param: u32) {
        let site = self.region.as_mut().unwrap().sites.last_mut().unwrap();
        #[cfg(not(feature = "wasm-jit-profile"))]
        { site.1 = param; }
        #[cfg(feature = "wasm-jit-profile")]
        { site.2 = param; }
    }
    fn ret_value(&mut self, code: u32) {
        let tag = self.tag(code);
        if self.dynamic {
            self.get(DONE);
            if self.pending != 0 {
                self.c(self.pending);
                self.op(0x6a);
            }
            self.c(tag);
            self.op(0x72);
        } else {
            self.c(tag | self.pending);
        }
        self.op(0x0f);
    }
    /// One more instruction retired on the current path.
    fn advance(&mut self) {
        self.pending += 1;
    }
    /// Materialize pending retirements into DONE. Only valid at a point that dominates
    /// every later read on the same path: straight-line code, or just before leaving.
    fn flush(&mut self) {
        if self.dynamic && self.pending != 0 {
            self.get(DONE);
            self.c(self.pending);
            self.op(0x6a);
            self.set(DONE);
        }
        self.pending = 0;
    }
    /// EX138: charge `cycles` beyond the instruction's own to `Cpu::timing_extra`.
    fn price(&mut self, cycles: u32) {
        if cycles == 0 || !super::PRICED.load(std::sync::atomic::Ordering::Relaxed) { return; }
        self.get(0);
        self.cpu(offset_of!(Cpu, timing_extra));
        self.c(cycles);
        self.op(0x6a);
        self.store(offset_of!(Cpu, timing_extra));
    }
    fn refund_wait(&mut self) {
        if self.wait_price == 0 || !super::PRICED.load(std::sync::atomic::Ordering::Relaxed) { return; }
        self.get(0);
        self.cpu(offset_of!(Cpu, timing_extra));
        self.c(self.wait_price);
        self.op(0x6b);
        self.store(offset_of!(Cpu, timing_extra));
    }
    /// Retire the current instruction and continue at a statically known `target`.
    fn leave(&mut self, target: u32) {
        if !std::mem::take(&mut self.free_leave) { self.price(2 + self.straddle as u32); }
        self.advance();
        if self.region.is_some() {
            region_edge(self, target, false);
        } else {
            self.cpu_const(PC, target);
            self.ret(CODE_LEFT);
        }
    }
    fn helper(&mut self, offset: usize, ty: u8) {
        self.get(2);
        self.load(offset);
        self.bytes.extend([0x11, ty, 0]);
    }
    /// Push the occupied-window mask touched by this AR operand range.
    fn window_collision(&mut self, max_ar: u8) {
        self.get(WINDOWS);
        self.c((1 << (max_ar / 4)) - 1);
        self.op(0x71);
    }
    /// Take one already-proved hardware backedge. Its caller chooses the target path.
    fn decrement_loop(&mut self) {
        self.get(0);
        self.cpu(LCOUNT);
        self.c(1);
        self.op(0x6b);
        self.store(LCOUNT);
    }
    /// Keep disabled-coprocessor traps at the instruction boundary: a checked body
    /// must retire its prefix and honor budget cuts before executing this fallback.
    fn guard_coprocessor(&mut self, mask: u32, bi: &BlockInsn, pc: u32, next: u32, last: bool) {
        self.cpu(offset_of!(Cpu, cpenable));
        self.c(mask);
        self.op(0x71);
        self.op(0x45);
        self.begin_if();
        self.fallback(bi, pc, next, last, false);
        self.end();
    }
    fn overflow(&mut self, max_ar: u8, pc: u32) {
        if max_ar < 4 {
            return;
        }
        self.window_collision(max_ar);
        self.begin_if();
        self.spill();
        self.get(0);
        self.c(max_ar as u32);
        self.c(pc);
        self.helper(4, 2);
        self.reload();
        self.begin_if();
        self.ret(CODE_TRAP_PRE);
        self.end();
        self.end();
    }
    fn fallback(
        &mut self,
        instruction: *const BlockInsn,
        pc: u32,
        next: u32,
        last: bool,
        continue_block: bool,
    ) {
        self.spill();
        self.get(0);
        self.get(1);
        self.c(instruction as usize as u32);
        self.c(pc);
        self.helper(0, 1);
        // Preserve the helper result across reload, whose window calculation uses TMP.
        self.set(REL);
        if continue_block {
            self.reload();
            if self.region.is_some() {
                // The helper may have written a region code page: leave at the next head.
                self.c(1);
                self.set(DIRTY);
            }
        }
        self.get(REL);
        self.set(TMP);
        self.advance();
        self.get(TMP);
        self.c(1);
        self.op(0x71);
        self.begin_if();
        // Match the interpreter: an instruction that faults or is deferred does
        // not retain its dependency wait. The executed prefix remains priced.
        self.refund_wait();
        if continue_block {
            self.ret(CODE_TRAP);
        } else {
            self.ret_value(CODE_TRAP);
        }
        self.end();
        self.get(TMP);
        self.cpu(PC);
        self.c(next);
        self.op(0x47);
        self.op(0x72);
        self.begin_if();
        if continue_block {
            self.ret(CODE_LEFT);
        } else {
            self.ret_value(CODE_LEFT);
        }
        self.end();
        if last || !continue_block {
            // A region never returns CUT: its continuation is a plain PC, not a head-block index.
            let code = if last { CODE_END } else if self.region.is_some() { CODE_LEFT } else { CODE_CUT };
            if continue_block {
                self.ret(code);
            } else {
                self.ret_value(code);
            }
        } else {
            self.cpu(WINDOWBASE);
            self.c(2);
            self.op(0x74);
            self.set(WB);
        }
    }
    fn fallthrough(&mut self, next: u32, looping: bool) {
        self.accx_flush();
        self.advance();
        self.flush();
        self.cpu(LEND);
        self.c(next);
        self.op(0x46);
        self.begin_if();
        self.cpu(LCOUNT);
        self.begin_if();
        self.decrement_loop();
        self.get(0);
        self.cpu(LBEG);
        self.store(PC);
        if looping {
            // LCOUNT-if, LEND-if, instruction-if, shared-backedge block.
            self.bytes.extend([0x0c, 3]);
        } else {
            self.ret(CODE_LEFT);
        }
        self.end();
        self.end();
    }
    /// EX156: the one static loop end of a guarded body. Same decisions as `fallthrough`
    /// followed by `repeat_guard`, reached only here instead of after every instruction.
    fn guarded_backedge(&mut self, hint: u32, loop_depth: usize) {
        // Read LEND here, not at entry: a LOOP instruction just before may have moved it.
        self.cpu(LEND);
        self.c(hint);
        self.op(0x46);
        self.begin_if();
        self.cpu(LCOUNT);
        self.begin_if();
        self.decrement_loop();
        self.get(0);
        self.cpu(LBEG);
        self.store(PC);
        self.flush();
        self.get(2);
        self.load(offset_of!(Helpers, loop_end));
        self.cpu(LEND);
        self.op(0x46);
        self.get(2);
        self.load(offset_of!(Helpers, version_ptrs));
        self.op(0x45);
        self.op(0x45);
        self.op(0x71);
        self.begin_if();
        for n in 0..2 {
            self.get(2);
            self.load(offset_of!(Helpers, version_ptrs) + n * 4);
            self.load(0);
            self.get(2);
            self.load(offset_of!(Helpers, versions) + n * 4);
            self.op(0x46);
            if n != 0 { self.op(0x71); }
        }
        self.get(DONE);
        self.get(3);
        self.op(0x49);
        self.op(0x71);
        self.begin_if();
        self.c(0);
        self.set(4);
        self.get(3);
        self.get(DONE);
        self.op(0x6b);
        self.set(STOP);
        self.op(0x0c);
        let label = self.depth() - loop_depth;
        uleb(&mut self.bytes, label);
        self.end();
        self.end();
        self.ret(CODE_LEFT);
        self.end();
        self.end();
    }
    fn repeat_guard(&mut self) {
        self.flush();
        self.get(2);
        self.load(offset_of!(Helpers, loop_end));
        self.cpu(LEND);
        self.op(0x46);
        self.get(2);
        self.load(offset_of!(Helpers, version_ptrs));
        self.op(0x45);
        self.op(0x45);
        self.op(0x71);
        self.begin_if();
        // A store into either decoded code page must return to normal validation.
        // This guard is emitted once per block, not once per possible loop-end PC.
        for n in 0..2 {
            self.get(2);
            self.load(offset_of!(Helpers, version_ptrs) + n * 4);
            self.load(0);
            self.get(2);
            self.load(offset_of!(Helpers, versions) + n * 4);
            self.op(0x46);
            if n != 0 { self.op(0x71); }
        }
        self.get(DONE);
        self.get(3);
        self.op(0x49);
        self.op(0x71);
        self.begin_if();
        self.c(0);
        self.set(4);
        // version/budget-if, admission-if, enclosing WASM loop.
        self.bytes.extend([0x0c, 2]);
        self.end();
        self.end();
        self.ret(CODE_LEFT);
    }

}

pub(super) fn generate(block: &Block) -> Vec<u8> {
    // A conservative operand mask avoids loading all sixteen registers for tiny blocks.
    // Interpreter-only opcodes may use implicit registers, so their test/future helper
    // path retains the full register file.
    let registers = block.instructions.iter().enumerate().fold(0u16, |mask, (n, bi)| {
        if n + 1 == block.instructions.len() && terminal_helper(bi.insn.op)
            && !supported_insn(&bi.insn, block.fast) {
            // The helper reads the CPU after dirty locals have been spilled. It exits
            // immediately, so neither its operands nor its new window need loading.
            mask
        } else if !supported_insn(&bi.insn, block.fast) {
            u16::MAX
        } else {
            // Include destinations (also conditional ones), not just reads: entry may
            // resume after an earlier write, and emitted selects read the old destination.
            // ENTRY reloads this same whole-block mask after rotating the register window.
            mask | bi.insn.gpr_effects().touched()
        }
    });
    let max_ar = block.instructions.iter().map(|bi| bi.max_ar).max().unwrap_or(0);
    let mut g = Gen { loaded: registers, max_ar, ..Gen::default() };
    let looping = block.loop_prefix != 0;
    g.reload();
    // The common path has a whole-block budget, no possible window collision and
    // no active loop end in this block. Prove those facts once rather than checking
    // them for each instruction. Cuts/resumes and exceptional states use the checked
    // path. Unsigned subtraction also handles blocks crossing the address wrap.
    // EX156: the same proofs minus "whole block from its head" admit the guarded body,
    // which enters at any index and cuts at any index with one compare per instruction.
    let hint = block.lend_hint.get();
    let site = if hint == 0 { None } else {
        block.instructions.iter().zip(&block.pcs).position(|(i, pc)| pc.wrapping_add(i.insn.len as u32) == hint).map(|n| n + 1)
    };
    // No loop end inside this block.
    g.cpu(LCOUNT);
    g.op(0x45);
    g.cpu(LEND);
    g.c(block.pc);
    g.op(0x6b);
    g.c(block.instructions.iter().map(|bi| bi.insn.len as u32).sum());
    g.op(0x4b);
    g.op(0x72);
    g.set(STOP);
    g.get(STOP);
    if site.is_some() {
        g.cpu(LEND);
        g.c(hint);
        g.op(0x46);
        g.op(0x72);
    }
    if max_ar >= 4 {
        g.window_collision(max_ar);
        g.op(0x45);
        g.op(0x71);
    }
    let cp = coprocessors(&block.instructions, block.fast);
    if cp != 0 {
        // A disabled coprocessor takes the checked path, which completes the
        // integer prefix and traps exactly at the first executed FP/PIE instruction.
        g.cpu(offset_of!(Cpu, cpenable));
        g.c(cp);
        g.op(0x71);
        g.c(cp);
        g.op(0x46);
        g.op(0x71);
    }
    g.begin_if();
    g.get(4);
    g.op(0x45);
    g.get(3);
    g.c(block.instructions.len() as u32);
    g.op(0x4f);
    g.op(0x71);
    g.get(STOP);
    g.op(0x71);
    g.begin_if();
    emit_body(&mut g, block.pc, &block.instructions, block.fast, looping, true, cp);
    g.end();
    {
        #[cfg(feature = "wasm-jit-tests")]
        {
            g.c(super::tests::GUARDED_TAKEN.as_ptr() as u32);
            g.c(1);
            g.store(0);
        }
        let whole_written = g.written;
        g.dynamic = true;
        g.pending = 0;
        g.written = if site.is_some() { whole_written & registers } else { 0 };
        g.c(0);
        g.get(4);
        g.op(0x6b);
        g.set(DONE);
        g.get(3);
        g.get(4);
        g.op(0x6a);
        g.set(STOP);
        if site.is_some() { g.begin_loop(); }
        let loop_depth = g.depth();
        let n = block.instructions.len();
        for _ in 0..n { g.begin_block(); }
        g.get(4);
        g.op(0x0e);
        uleb(&mut g.bytes, n - 1);
        for k in 0..n { uleb(&mut g.bytes, k); }
        g.guarded = true;
        g.guard_site = site.map(|n| (n, loop_depth));
        emit_body(&mut g, block.pc, &block.instructions, block.fast, false, true, cp);
        g.guarded = false;
        g.guard_site = None;
        if site.is_some() {
            g.end();
            g.op(0x00);
        }
        g.dynamic = false;
        g.pending = 0;
        g.written = whole_written;
    }
    g.end();
    // Cuts, resumes and repeats count at run time; the whole body above never wrote DONE.
    g.dynamic = true;
    g.pending = 0;
    // On a backedge, later instructions may have dirtied locals before an early cut.
    // The whole-body pass has already identified exactly those written registers.
    if looping {
        // A suffix CALL can write an implicit return register that was never loaded.
        // It cannot participate in a repeated prefix; do not spill it before it executes.
        g.written &= registers;
        g.begin_loop(); // repeat loop
        g.begin_block(); // shared-backedge block
    } else {
        g.written = 0;
    }
    emit_body(&mut g, block.pc, &block.instructions, block.fast, looping, false, 0);
    if looping {
        g.pending = 0; // the checked body ended with a return
        g.end();
        // Only loaded operands can be live at a fallthrough backedge; suffix calls
        // return directly and must not contribute their uninitialized return locals.
        g.written &= registers;
        g.repeat_guard();
        g.end();
        g.op(0x00); // every iteration returns or branches; no void-loop fallthrough
    }
    g.end();
    finish(g, block.pc)
}

fn finish(g: Gen, pc: u32) -> Vec<u8> {
    #[cfg(not(feature = "wasm-cpu-profile"))]
    { let _ = pc; module(&g.bytes) }
    #[cfg(feature = "wasm-cpu-profile")]
    {
        let mut bytes = module(&g.bytes);
        // Diagnostic names connect host CPU samples to the guest ELF without a debugger.
        let mut names = Vec::new();
        name(&mut names, "name");
        let mut functions = vec![1, 0]; // one function, index zero
        name(&mut functions, &format!("xtensa_{:08x}", pc));
        section(&mut names, 1, &functions);
        section(&mut bytes, 0, &names);
        bytes
    }
}

fn emit_body(
    g: &mut Gen,
    pc0: u32,
    instructions: &[BlockInsn],
    fast: bool,
    looping: bool,
    whole: bool,
    cp: u32,
) {
    let mut pc = pc0;
    let mut window_changed = false;
    let extras = if super::PRICED.load(std::sync::atomic::Ordering::Relaxed) { crate::exec::static_extras(instructions.iter().map(|b| &b.insn)) } else { vec![0; instructions.len()] };
    // EX178: only a body entered exclusively at its head may hold ACCX in a local.
    g.accx_ok = whole && !g.guarded;
    g.accx_live = false;
    let mut skip = 0usize;
    for (index, bi) in instructions.iter().enumerate() {
        let next = pc.wrapping_add(bi.insn.len as u32);
        if skip > 0 {
            // Already emitted as part of a coalesced run.
            skip -= 1;
            pc = next;
            continue;
        }
        // EX178 s1: one range probe for a whole straight-line run of post-increment
        // vector loads. Only a body entered at its head, with no window rotation behind
        // it and CP3 already proved, can contain a run no other path may land inside.
        if whole && !g.guarded && !window_changed && cp & pie::CP3 != 0 {
            if let Some(run) = pie::coalesce(&instructions[index..], fast) {
                let end = index + run.len;
                pie::emit_run(g, &instructions[index..end], pc, &extras[index..end], &run, end == instructions.len());
                skip = run.len - 1;
                pc = next;
                continue;
            }
        }
        g.last_pc = pc;
        // Anything outside the accumulate run may read ACCX, or reach a helper that does.
        if !pie::accx_local_safe(&bi.insn, fast) {
            g.accx_flush();
        }
        #[cfg(feature = "wasm-jit-profile")]
        { g.last_kind = ExitKind::for_op(bi.insn.op); }
        if !whole {
            g.flush();
            g.get(4);
            g.c(index as u32);
            g.op(0x4d);
            g.begin_if();
            g.get(DONE);
            g.get(3);
            g.op(0x4f);
            g.begin_if();
            g.cpu_const(PC, pc);
            g.ret(CODE_CUT);
            g.end();
        }
        if g.guarded {
            // Close this index's entry label. The static retirement count is the index
            // (an unconditional J before it has counted itself on its own dead path).
            g.end();
            g.pending = index as u32;
            match g.cut_target {
                None => {
                    g.get(STOP);
                    g.c(index as u32);
                    g.op(0x4d);
                    g.begin_if();
                    g.cpu_const(PC, pc);
                    g.ret(CODE_CUT);
                    g.end();
                }
                // tails-s1: a region copy is entered with STOP above its entry index, so index 0
                // never cuts. A region returns TAIL, not CUT (whose tag the caller reads as a block
                // index), with the site naming the last retired instruction, as an own-module
                // cut's `offset - 1` does for note_pc/note_sequential; the next site names the head.
                Some(depth) if index > 0 => {
                    g.get(STOP);
                    g.c(index as u32);
                    g.op(0x4d);
                    g.begin_if();
                    g.cpu_const(PC, pc);
                    let site = std::mem::replace(&mut g.last_pc, pc.wrapping_sub(instructions[index - 1].insn.len as u32));
                    #[cfg(feature = "wasm-jit-profile")]
                    let kind = std::mem::replace(&mut g.last_kind, ExitKind::Budget);
                    let tag = g.tag(CODE_TAIL);
                    let r = g.region.as_ref().unwrap();
                    let copy = r.copies.as_ref().and_then(|c| c[r.current]).unwrap();
                    g.resume_at(copy | (index as u32) << 16);
                    g.last_pc = pc0;
                    g.tag(CODE_TAIL);
                    g.last_pc = site;
                    #[cfg(feature = "wasm-jit-profile")]
                    { g.last_kind = kind; }
                    g.get(DONE);
                    g.c(index as u32);
                    g.op(0x6a);
                    g.c(tag);
                    g.op(0x72);
                    g.set(TMP);
                    let label = g.depth() - depth;
                    g.op(0x0c);
                    uleb(&mut g.bytes, label);
                    g.end();
                }
                Some(_) => {}
            }
        }
        if !whole || window_changed {
            // The window helper runs with the CPU visible; do not leave ACCX in a local.
            g.accx_flush();
            g.overflow(bi.max_ar, pc);
        }
        // Record only instructions reached after budget and pre-instruction guards.
        // The dispatcher caps priced calls at the ring capacity, including retained loops.
        if super::FETCH_RING.load(std::sync::atomic::Ordering::Relaxed) {
            g.cpu(offset_of!(Cpu, icache_fill));
            g.begin_if();
            for (offset, address) in [(0, pc), (4, next.wrapping_sub(1))] {
                g.get(0);
                g.cpu(offset_of!(Cpu, fetch_n));
                // Bound the write even if a future emitted path overruns its credit.
                g.c(63);
                g.op(0x71); // i32.and
                g.c(3);
                g.op(0x74);
                g.op(0x6a);
                g.c(address);
                g.store(offset_of!(Cpu, fetch_ring) + offset);
            }
            g.get(0);
            g.cpu(offset_of!(Cpu, fetch_n));
            g.c(1);
            g.op(0x6a);
            g.store(offset_of!(Cpu, fetch_n));
            g.end();
        }
        let last = index + 1 == instructions.len();
        g.wait_price = extras[index] as u32;
        g.price(g.wait_price);
        g.straddle = bi.straddle;
        if instruction::emit(g, bi, fast, pc, next, last, cp) {
            if whole {
                g.advance();
                if let Some((_, loop_depth)) = g.guard_site.filter(|s| s.0 == index + 1) {
                    g.guarded_backedge(next, loop_depth);
                }
            } else {
                g.fallthrough(next, looping);
            }
            if g.region.is_some() && bi.insn.op == crate::Op::Entry && g.max_ar >= 4 {
                // The rotated window must be free for everything the region touches;
                // otherwise continue at the next instruction through ordinary blocks.
                // ENTRY has retired, so a hardware loop ending right here takes its
                // backedge first, as the interpreter's epilogue would.
                g.window_collision(g.max_ar);
                g.begin_if();
                g.spill();
                g.cpu(LEND);
                g.c(next);
                g.op(0x46);
                g.cpu(LCOUNT);
                g.c(0);
                g.op(0x47);
                g.op(0x71);
                g.begin_if();
                g.decrement_loop();
                g.get(0);
                g.cpu(LBEG);
                g.store(PC);
                g.ret_value(CODE_LEFT);
                g.end();
                g.cpu_const(PC, next);
                g.ret_value(CODE_LEFT);
                g.end();
            }
        } else {
            g.fallback(bi, pc, next, last, !last);
        }
        // ENTRY changes which frames can collide; the entry-time whole-block
        // window proof no longer covers subsequent operands.
        window_changed |= bi.insn.op == crate::Op::Entry && g.region.is_none();
        if !whole {
            // Both arms of the entry test must agree on the pending count at the join.
            g.flush();
            g.end();
        }
        pc = next;
    }
    if g.region.is_some() {
        // A J, call, return or JX already left; a trailing edge would be unreachable.
        let last = instructions.last().unwrap();
        if last.insn.op != crate::Op::J && !terminal_helper(last.insn.op) && last.insn.op != crate::Op::Jx {
            if let Some(&lbeg) = g.region.as_ref().unwrap().loops.get(&pc) {
                // Straight-line arrival at LEND with an active count is the backedge. The
                // region may have been entered with some other loop active, so LEND must
                // be this address as well, exactly as the interpreter tests it.
                g.cpu(LEND);
                g.c(pc);
                g.op(0x46);
                g.cpu(LCOUNT);
                g.c(0);
                g.op(0x47);
                g.op(0x71);
                // Emitting the taken arm spills ACCX and clears its compile-time
                // liveness. The untaken arm still holds that value in the local.
                let accx_live = g.accx_live;
                g.begin_if();
                g.decrement_loop();
                region_edge(g, lbeg, false);
                g.end();
                g.accx_live = accx_live;
            }
            region_edge(g, pc, true);
        }
    } else {
        g.cpu_const(PC, pc);
        g.ret(CODE_END);
    }
}

fn uleb(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        out.push(b | if n != 0 { 128 } else { 0 });
        if n == 0 {
            break;
        }
    }
}
fn sleb(out: &mut Vec<u8>, mut n: i32) {
    loop {
        let b = (n as u8) & 127;
        n >>= 7;
        let done = (n == 0 && b & 64 == 0) || (n == -1 && b & 64 != 0);
        out.push(b | if done { 0 } else { 128 });
        if done {
            break;
        }
    }
}
fn name(out: &mut Vec<u8>, s: &str) {
    uleb(out, s.len());
    out.extend(s.as_bytes());
}
fn section(out: &mut Vec<u8>, id: u8, bytes: &[u8]) {
    out.push(id);
    uleb(out, bytes.len());
    out.extend(bytes);
}
fn module(body: &[u8]) -> Vec<u8> {
    let mut out = b"\0asm\x01\0\0\0".to_vec();
    let mut types = vec![3];
    for count in [7, 4, 3] {
        types.extend([0x60, count]);
        types.extend(vec![0x7f; count as usize]);
        types.extend([1, 0x7f]);
    }
    section(&mut out, 1, &types);
    let mut imports = vec![2];
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 0]);
    name(&mut imports, "env");
    name(&mut imports, "table");
    imports.extend([1, 0x70, 0, 0]);
    section(&mut out, 2, &imports);
    section(&mut out, 3, &[1, 0]);
    let mut exports = vec![1];
    name(&mut exports, "run");
    exports.extend([0, 0]);
    section(&mut out, 7, &exports);
    let mut func = vec![4, if cfg!(feature = "wasm-cache-inline") { 29 } else { 25 }, 0x7f, 1, 0x7b, 2, 0x7e, 2, 0x7f];   // i32 locals, then V128, WIDE, ACC, STOP and HOSTP
    func.extend(body);
    let mut code = vec![1];
    uleb(&mut code, func.len());
    code.extend(func);
    section(&mut out, 10, &code);
    out
}
