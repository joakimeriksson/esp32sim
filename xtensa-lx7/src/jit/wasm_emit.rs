//! Binary WASM emitter for an Xtensa block, with budgeted hardware-loop prefixes.
use super::*;
#[path = "wasm_float.rs"]
mod float;

// Most unsupported operations keep their block interpreted. Calls/returns at the
// end may use a helper after the compiled prefix; memory misses also use helpers.
pub(super) fn supported(op: crate::Op, fast: bool) -> bool {
    use crate::Op::*;
    matches!(
        op,
        Nop | NopN
            | Memw
            | Extw
            | Movi
            | MoviN
            | Mov
            | MovN
            | Add
            | AddN
            | Sub
            | And
            | Or
            | Xor
            | Mull
            | Muluh
            | Mulsh
            | Salt
            | Saltu
            | Addi
            | AddiN
            | Addmi
            | Addx2
            | Addx4
            | Addx8
            | Subx2
            | Subx4
            | Subx8
            | Neg
            | Abs
            | Slli
            | Srli
            | Srai
            | Sll
            | Srl
            | Sra
            | Src
            | Entry
            | Extui
            | Sext
            | Ssr
            | Ssl
            | Ssa8l
            | Ssa8b
            | Ssai
            | Nsau
            | Moveqz
            | Movnez
            | Movltz
            | Movgez
            | Min
            | Max
            | Minu
            | Maxu
            | J
            | Jx
            | Call0 | Call4 | Call8 | Call12 | Callx0 | Callx4 | Callx8 | Callx12
            | Beqz
            | BeqzN
            | Bnez
            | BnezN
            | Bltz
            | Bgez
            | Beqi
            | Bnei
            | Blti
            | Bgei
            | Bltui
            | Bgeui
            | Beq
            | Bne
            | Blt
            | Bge
            | Bltu
            | Bgeu
            | Bbci
            | Bbsi
            | Bbc
            | Bbs
    ) || float::supported(op) || (fast
        && matches!(
            op,
            L8ui | L16ui | L16si | L32i | L32iN | L32r | S8i | S16i | S32i | S32iN | Lsi | Ssi
        ))
}

// Initially admit only straight-line integer/memory loops. Slow memory paths leave
// generated execution; no helper can change mappings or interrupt state and continue.
pub(super) fn loop_safe(op: crate::Op, fast: bool) -> bool {
    use crate::Op::*;
    matches!(op, Nop | NopN | Movi | MoviN | Mov | MovN | Add | AddN | Sub
        | And | Or | Xor | Addi | AddiN | Addmi | Addx2 | Addx4 | Addx8
        | Subx2 | Subx4 | Subx8 | Neg | Slli | Srli | Srai | Extui | Sext)
        || (fast && matches!(op, L8ui | L16ui | L16si | L32i | L32iN | L32r
            | S8i | S16i | S32i | S32iN))
}

// Calls and returns must end decoder blocks. Normal calls are emitted directly;
// returns and exceptional calls retain exec_insn's window and exception handling.
pub(super) fn terminal_helper(op: crate::Op) -> bool {
    use crate::Op::*;
    matches!(op, Call0 | Call4 | Call8 | Call12 | Callx0 | Callx4 | Callx8 | Callx12
        | Ret | RetN | Retw | RetwN)
}

// Parameters: cpu, bus, helpers, budget, entry, TLB, versions.
// Locals: done, windowbase*4, scratch, guest address, TLB entry, relative offset.
const DONE: u8 = 7;
const WB: u8 = 8;
const TMP: u8 = 9;
const ADDR: u8 = 10;
const TLB: u8 = 11;
const REL: u8 = 12;
const WINDOWS: u8 = 29;
const PC: usize = offset_of!(Cpu, pc);
const AR: usize = offset_of!(Cpu, ar);
const WINDOWBASE: usize = offset_of!(Cpu, windowbase);
const LCOUNT: usize = offset_of!(Cpu, lcount);
const LEND: usize = offset_of!(Cpu, lend);
const LBEG: usize = offset_of!(Cpu, lbeg);
const SAR: usize = offset_of!(Cpu, sar);

#[derive(Default)]
struct Gen(Vec<u8>, u16, u16);
impl Gen {
    fn op(&mut self, op: u8) {
        self.0.push(op);
    }
    fn c(&mut self, v: u32) {
        self.op(0x41);
        sleb(&mut self.0, v as i32);
    }
    fn get(&mut self, n: u8) {
        self.0.extend([0x20, n]);
    }
    fn set(&mut self, n: u8) {
        self.0.extend([0x21, n]);
    }
    fn tee(&mut self, n: u8) {
        self.0.extend([0x22, n]);
    }
    fn load(&mut self, offset: usize) {
        self.op(0x28);
        uleb(&mut self.0, 2);
        uleb(&mut self.0, offset);
    }
    fn store(&mut self, offset: usize) {
        self.op(0x36);
        uleb(&mut self.0, 2);
        uleb(&mut self.0, offset);
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
        self.1 |= 1 << r;
    }
    fn reload(&mut self) {
        self.cpu(WINDOWBASE);
        self.c(2);
        self.op(0x74);
        self.set(WB);
        for r in 0..16 {
            if self.2 & (1 << r) != 0 {
                self.ar_addr(r);
                self.load(AR);
                self.set(13 + r);
            }
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
        for r in 0..16 {
            if self.1 & (1 << r) != 0 {
                self.ar_addr(r);
                self.get(13 + r);
                self.store(AR);
            }
        }
    }
    fn begin_if(&mut self) {
        self.0.extend([0x04, 0x40]);
    }
    fn end(&mut self) {
        self.op(0x0b);
    }
    fn ret(&mut self, code: u32) {
        self.spill();
        self.ret_value(code);
    }
    fn ret_value(&mut self, code: u32) {
        self.get(DONE);
        self.c(code << 16);
        self.op(0x72);
        self.op(0x0f);
    }
    fn advance(&mut self) {
        self.get(DONE);
        self.c(1);
        self.op(0x6a);
        self.set(DONE);
    }
    fn helper(&mut self, offset: usize, ty: u8) {
        self.get(2);
        self.load(offset);
        self.0.extend([0x11, ty, 0]);
    }
    fn overflow(&mut self, max_ar: u8, pc: u32) {
        if max_ar < 4 {
            return;
        }
        self.get(WINDOWS);
        self.c((1 << (max_ar / 4)) - 1);
        self.op(0x71);
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
        }
        self.get(REL);
        self.set(TMP);
        self.advance();
        self.get(TMP);
        self.c(1);
        self.op(0x71);
        self.begin_if();
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
            if continue_block {
                self.ret(if last { CODE_END } else { CODE_CUT });
            } else {
                self.ret_value(if last { CODE_END } else { CODE_CUT });
            }
        } else {
            self.cpu(WINDOWBASE);
            self.c(2);
            self.op(0x74);
            self.set(WB);
        }
    }
    fn fallthrough(&mut self, next: u32, looping: bool) {
        self.advance();
        self.cpu(LEND);
        self.c(next);
        self.op(0x46);
        self.begin_if();
        self.cpu(LCOUNT);
        self.begin_if();
        self.get(0);
        self.cpu(LCOUNT);
        self.c(1);
        self.op(0x6b);
        self.store(LCOUNT);
        self.get(0);
        self.cpu(LBEG);
        self.store(PC);
        if looping {
            // LCOUNT-if, LEND-if, instruction-if, shared-backedge block.
            self.0.extend([0x0c, 3]);
        } else {
            self.ret(CODE_LEFT);
        }
        self.end();
        self.end();
    }
    fn repeat_guard(&mut self) {
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
        self.0.extend([0x0c, 2]);
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
            && !supported(bi.insn.op, block.fast) {
            // The helper reads the CPU after dirty locals have been spilled. It exits
            // immediately, so neither its operands nor its new window need loading.
            mask
        } else if !supported(bi.insn.op, block.fast) {
            u16::MAX
        } else {
            // Include destinations (also conditional ones), not just reads: entry may
            // resume after an earlier write, and emitted selects read the old destination.
            // ENTRY reloads this same whole-block mask after rotating the register window.
            mask | bi.insn.gpr_effects().touched()
        }
    });
    let mut g = Gen(Vec::new(), 0, registers);
    g.reload();
    // The common path has a whole-block budget, no possible window collision and
    // no active loop end in this block. Prove those facts once rather than checking
    // them for each instruction. Cuts/resumes and exceptional states use the checked
    // path. Unsigned subtraction also handles blocks crossing the address wrap.
    g.get(4);
    g.op(0x45);
    g.get(3);
    g.c(block.instructions.len() as u32);
    g.op(0x4f);
    g.op(0x71);
    let max_ar = block.instructions.iter().map(|bi| bi.max_ar).max().unwrap_or(0);
    if max_ar >= 4 {
        g.get(WINDOWS);
        g.c((1 << (max_ar / 4)) - 1);
        g.op(0x71);
        g.op(0x45);
        g.op(0x71);
    }
    g.cpu(LCOUNT);
    g.op(0x45);
    g.cpu(LEND);
    g.c(block.pc);
    g.op(0x6b);
    g.c(block.instructions.iter().map(|bi| bi.insn.len as u32).sum());
    g.op(0x4b);
    g.op(0x72);
    g.op(0x71);
    if float::can_hoist_guard(block) {
        // A disabled coprocessor takes the checked path, which completes the
        // integer prefix and traps exactly at the first executed FP instruction.
        g.cpu(offset_of!(Cpu, cpenable));
        g.c(1);
        g.op(0x71);
        g.op(0x71);
    }
    g.begin_if();
    emit_body(&mut g, block, true);
    g.end();
    let looping = block.loop_prefix != 0;
    // On a backedge, later instructions may have dirtied locals before an early cut.
    // The whole-body pass has already identified exactly those written registers.
    if looping {
        // A suffix CALL can write an implicit return register that was never loaded.
        // It cannot participate in a repeated prefix; do not spill it before it executes.
        g.1 &= registers;
        g.0.extend([0x03, 0x40, 0x02, 0x40]); // repeat loop, shared-backedge block
    } else {
        g.1 = 0;
    }
    emit_body(&mut g, block, false);
    if looping {
        g.end();
        // Only loaded operands can be live at a fallthrough backedge; suffix calls
        // return directly and must not contribute their uninitialized return locals.
        g.1 &= registers;
        g.repeat_guard();
        g.end();
        g.op(0x00); // every iteration returns or branches; no void-loop fallthrough
    }
    g.end();
    #[cfg(not(feature = "wasm-cpu-profile"))]
    { module(&g.0) }
    #[cfg(feature = "wasm-cpu-profile")]
    {
        let mut bytes = module(&g.0);
        // Diagnostic names connect host CPU samples to the guest ELF without a debugger.
        let mut names = Vec::new();
        name(&mut names, "name");
        let mut functions = vec![1, 0]; // one function, index zero
        name(&mut functions, &format!("xtensa_{:08x}", block.pc));
        section(&mut names, 1, &functions);
        section(&mut bytes, 0, &names);
        bytes
    }
}

fn emit_body(g: &mut Gen, block: &Block, whole: bool) {
    let mut pc = block.pc;
    let mut window_changed = false;
    let cp_enabled = whole && float::can_hoist_guard(block);
    for (index, bi) in block.instructions.iter().enumerate() {
        let next = pc.wrapping_add(bi.insn.len as u32);
        if !whole {
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
        if !whole || window_changed {
            g.overflow(bi.max_ar, pc);
        }
        let last = index + 1 == block.instructions.len();
        if emit_instruction(g, bi, block.fast, pc, next, last, cp_enabled) {
            if whole {
                g.advance();
            } else {
                g.fallthrough(next, block.loop_prefix != 0);
            }
        } else {
            g.fallback(bi, pc, next, last, !last);
        }
        // ENTRY changes which frames can collide; the entry-time whole-block
        // window proof no longer covers subsequent operands.
        window_changed |= bi.insn.op == crate::Op::Entry;
        if !whole {
            g.end();
        }
        pc = next;
    }
    g.cpu_const(PC, pc);
    g.ret(CODE_END);
}

fn emit_instruction(
    g: &mut Gen,
    bi: &BlockInsn,
    fast: bool,
    pc: u32,
    next: u32,
    last: bool,
    cp_enabled: bool,
) -> bool {
    use crate::Op::*;
    let i = &bi.insn;
    let (r, s, t) = (i.r, i.s, i.t);
    let imm = i.imm as u32;
    if float::supported(i.op) {
        float::emit(g, bi, pc, next, last, cp_enabled);
        return true;
    }
    match i.op {
        Nop | NopN | Memw | Extw => {}
        Movi | MoviN => {
            g.c(imm);
            g.set_ar(if i.op == Movi { t } else { s });
        }
        Mov | MovN => {
            g.ar(s);
            g.set_ar(t);
        }
        Add | AddN | Sub | And | Or | Xor | Mull | Salt | Saltu => {
            g.ar(s);
            g.ar(t);
            g.op(match i.op {
                Add | AddN => 0x6a,
                Sub => 0x6b,
                And => 0x71,
                Or => 0x72,
                Xor => 0x73,
                Mull => 0x6c,
                Salt => 0x48,
                _ => 0x49,
            });
            g.set_ar(r);
        }
        Muluh | Mulsh => {
            let extend = if i.op == Mulsh { 0xac } else { 0xad }; // i64.extend_i32_s/u
            g.ar(s);
            g.op(extend);
            g.ar(t);
            g.op(extend);
            g.op(0x7e); // i64.mul
            g.c(32);
            g.op(0xad);
            g.op(if i.op == Mulsh { 0x87 } else { 0x88 }); // i64.shr_s/u
            g.op(0xa7); // i32.wrap_i64
            g.set_ar(r);
        }
        Addi | AddiN | Addmi => {
            g.ar(s);
            g.c(imm);
            g.op(0x6a);
            g.set_ar(if i.op == AddiN { r } else { t });
        }
        Addx2 | Addx4 | Addx8 | Subx2 | Subx4 | Subx8 => {
            g.ar(s);
            g.c(match i.op {
                Addx2 | Subx2 => 1,
                Addx4 | Subx4 => 2,
                _ => 3,
            });
            g.op(0x74);
            g.ar(t);
            g.op(if matches!(i.op, Addx2 | Addx4 | Addx8) {
                0x6a
            } else {
                0x6b
            });
            g.set_ar(r);
        }
        Neg => {
            g.c(0);
            g.ar(t);
            g.op(0x6b);
            g.set_ar(r);
        }
        Abs => {
            g.c(0);
            g.ar(t);
            g.op(0x6b); // Wrapping negation preserves INT_MIN.
            g.ar(t);
            g.ar(t);
            g.c(0);
            g.op(0x48); // i32.lt_s
            g.op(0x1b);
            g.set_ar(r);
        }
        Slli | Srli | Srai => {
            g.ar(if i.op == Slli { s } else { t });
            g.c(imm & 31);
            g.op(match i.op {
                Slli => 0x74,
                Srai => 0x75,
                _ => 0x76,
            });
            g.set_ar(r);
        }
        Sll | Srl => {
            if i.op == Sll {
                g.c(32);
                g.cpu(SAR);
                g.op(0x6b);
                g.c(63);
                g.op(0x71);
            } else {
                g.cpu(SAR);
            }
            g.set(TMP);
            g.ar(if i.op == Sll { s } else { t });
            g.get(TMP);
            g.op(if i.op == Sll { 0x74 } else { 0x76 });
            g.c(0);
            g.get(TMP);
            g.c(32);
            g.op(0x49); // Counts >= 32 produce zero, unlike WASM's masked shifts.
            g.op(0x1b);
            g.set_ar(r);
        }
        Sra => {
            g.ar(t);
            g.cpu(SAR);
            g.c(31);
            g.cpu(SAR);
            g.c(32);
            g.op(0x49); // Clamp the unsigned count; WASM shifts otherwise wrap at 32.
            g.op(0x1b);
            g.op(0x75); // i32.shr_s
            g.set_ar(r);
        }
        Src => {
            g.ar(s);
            g.op(0xad); // i64.extend_i32_u
            g.c(32);
            g.op(0xad);
            g.op(0x86); // i64.shl
            g.ar(t);
            g.op(0xad);
            g.op(0x84); // i64.or
            g.cpu(SAR);
            g.op(0xad);
            g.op(0x88); // i64.shr_u masks the count to six bits, as Xtensa does.
            g.op(0xa7); // i32.wrap_i64
            g.set_ar(r);
        }
        Entry => {
            if s > 3 {
                return false;
            }
            g.cpu(offset_of!(Cpu, ps));
            g.c(ps::WOE);
            g.op(0x71);
            g.op(0x45);
            g.begin_if();
            g.fallback(bi, pc, next, last, false);
            g.end();
            // Commit the old window before rotating, then refresh all cached
            // operands and collision bits before writing the new stack pointer.
            g.ar(s);
            g.c(imm);
            g.op(0x6b);
            g.set(REL);
            g.spill();
            g.get(0);
            g.cpu(WINDOWBASE);
            g.cpu(offset_of!(Cpu, ps));
            g.c(ps::CALLINC_MASK);
            g.op(0x71);
            g.c(ps::CALLINC_SHIFT);
            g.op(0x76);
            g.op(0x6a);
            g.c(15);
            g.op(0x71);
            g.store(WINDOWBASE);
            g.get(0);
            g.cpu(offset_of!(Cpu, windowstart));
            g.c(1);
            g.cpu(WINDOWBASE);
            g.op(0x74);
            g.op(0x72);
            g.store(offset_of!(Cpu, windowstart));
            g.reload();
            g.get(REL);
            g.set_ar(s);
        }
        Extui => {
            g.ar(t);
            g.c(imm);
            g.op(0x76);
            g.c(if i.imm2 >= 32 {
                u32::MAX
            } else {
                (1u32 << i.imm2) - 1
            });
            g.op(0x71);
            g.set_ar(r);
        }
        Sext => {
            g.ar(s);
            g.c(31 - imm);
            g.op(0x74);
            g.c(31 - imm);
            g.op(0x75);
            g.set_ar(r);
        }
        Ssr | Ssl | Ssa8l | Ssa8b => {
            g.get(0);
            if matches!(i.op, Ssl | Ssa8b) {
                g.c(32);
            }
            g.ar(s);
            g.c(if matches!(i.op, Ssa8l | Ssa8b) { 3 } else { 31 });
            g.op(0x71);
            if matches!(i.op, Ssa8l | Ssa8b) {
                g.c(3);
                g.op(0x74);
            }
            if matches!(i.op, Ssl | Ssa8b) {
                g.op(0x6b);
            }
            g.store(SAR);
        }
        Ssai => g.cpu_const(SAR, imm & 31),
        Nsau => {
            g.ar(s);
            g.op(0x67);
            g.set_ar(t);
        }
        Moveqz | Movnez | Movltz | Movgez => {
            g.ar(s);
            g.ar(r);
            g.ar(t);
            g.c(0);
            g.op(match i.op {
                Moveqz => 0x46,
                Movnez => 0x47,
                Movltz => 0x48,
                _ => 0x4e,
            });
            g.op(0x1b);
            g.set_ar(r);
        }
        Min | Max | Minu | Maxu => {
            g.ar(s);
            g.ar(t);
            g.ar(s);
            g.ar(t);
            g.op(match i.op {
                Min => 0x48,
                Max => 0x4a,
                Minu => 0x49,
                _ => 0x4b,
            });
            g.op(0x1b);
            g.set_ar(r);
        }
        J => {
            g.advance();
            g.cpu_const(PC, imm);
            g.ret(CODE_LEFT);
        }
        Jx => {
            g.advance();
            g.get(0);
            g.ar(s);
            g.store(PC);
            g.ret(CODE_LEFT);
        }
        Call0 | Call4 | Call8 | Call12 | Callx0 | Callx4 | Callx8 | Callx12 => {
            let inc = match i.op {
                Call0 | Callx0 => 0,
                Call4 | Callx4 => 1,
                Call8 | Callx8 => 2,
                _ => 3,
            };
            if inc != 0 {
                // The ordinary overflow guard already ran. Keep the illegal WOE=0
                // case in the interpreter so its exception state remains identical.
                g.cpu(offset_of!(Cpu, ps));
                g.c(ps::WOE);
                g.op(0x71);
                g.op(0x45);
                g.begin_if();
                g.fallback(bi, pc, next, true, false);
                g.end();
            }
            let indirect = matches!(i.op, Callx0 | Callx4 | Callx8 | Callx12);
            if indirect {
                // The target may alias the return-address destination.
                g.ar(s);
                g.set(TMP);
            }
            if inc != 0 {
                g.get(0);
                g.cpu(offset_of!(Cpu, ps));
                g.c(!ps::CALLINC_MASK);
                g.op(0x71);
                g.c(inc << ps::CALLINC_SHIFT);
                g.op(0x72);
                g.store(offset_of!(Cpu, ps));
            }
            g.c(if inc == 0 { next } else { (inc << 30) | (next & 0x3fff_ffff) });
            g.set_ar((inc * 4) as u8);
            g.advance();
            if indirect {
                g.get(0);
                g.get(TMP);
                g.store(PC);
            } else {
                g.cpu_const(PC, imm);
            }
            g.ret(CODE_LEFT);
        }
        Beqz | BeqzN | Bnez | BnezN | Bltz | Bgez | Beqi | Bnei | Blti | Bgei | Bltui | Bgeui
        | Beq | Bne | Blt | Bge | Bltu | Bgeu => {
            g.ar(s);
            match i.op {
                Beqz | BeqzN | Bnez | BnezN | Bltz | Bgez => g.c(0),
                Beqi | Bnei | Blti | Bgei | Bltui | Bgeui => g.c(i.imm2 as u32),
                _ => g.ar(t),
            }
            g.op(match i.op {
                Beqz | BeqzN | Beqi | Beq => 0x46,
                Bnez | BnezN | Bnei | Bne => 0x47,
                Bltz | Blti | Blt => 0x48,
                Bgez | Bgei | Bge => 0x4e,
                Bltui | Bltu => 0x49,
                _ => 0x4f,
            });
            g.begin_if();
            g.advance();
            g.cpu_const(PC, imm);
            g.ret(CODE_LEFT);
            g.end();
        }
        Bbci | Bbsi | Bbc | Bbs => {
            g.ar(s);
            g.c(1);
            if matches!(i.op, Bbci | Bbsi) {
                g.c(i.imm2 as u32);
            } else {
                g.ar(t);
            }
            g.op(0x74);
            g.op(0x71);
            g.c(0);
            g.op(if matches!(i.op, Bbci | Bbc) {
                0x46
            } else {
                0x47
            });
            g.begin_if();
            g.advance();
            g.cpu_const(PC, imm);
            g.ret(CODE_LEFT);
            g.end();
        }
        L8ui | L16ui | L16si | L32i | L32iN | L32r | S8i | S16i | S32i | S32iN | Lsi | Ssi if fast => {
            if !cp_enabled && matches!(i.op, Lsi | Ssi) { float::guard(g, bi, pc, next, last); }
            emit_memory(g, bi, pc, next, last);
        }
        _ => return false,
    }
    true
}

fn emit_memory(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool) {
    use crate::Op::*;
    let i = &bi.insn;
    let store = matches!(i.op, S8i | S16i | S32i | S32iN | Ssi);
    let width = match i.op {
        L8ui | S8i => 1,
        L16ui | L16si | S16i => 2,
        _ => 4,
    };
    if i.op == L32r {
        g.c(i.imm as u32);
    } else {
        g.ar(i.s);
        g.c(i.imm as u32);
        g.op(0x6a);
    }
    g.set(ADDR);
    // This block jumps to the slow instruction before making any memory changes.
    g.0.extend([0x02, 0x40, 0x02, 0x40]);
    g.get(5);
    g.op(0x45);
    g.0.extend([0x0d, 0]);
    g.get(ADDR);
    g.c(width - 1);
    g.op(0x71);
    g.0.extend([0x0d, 0]);
    g.get(5);
    g.get(ADDR);
    g.c(16);
    g.op(0x76);
    g.get(ADDR);
    g.c(24);
    g.op(0x76);
    g.op(0x73);
    g.c(511);
    g.op(0x71);
    g.c(size_of::<TlbEntry>() as u32);
    g.op(0x6c);
    g.op(0x6a);
    g.set(TLB);
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, lo));
    g.op(0x49);
    g.0.extend([0x0d, 0]);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, hi));
    g.get(ADDR);
    g.op(0x6b);
    g.c(width);
    g.op(0x49);
    g.0.extend([0x0d, 0]);
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, hi));
    g.op(0x4f);
    g.0.extend([0x0d, 0]);
    if store {
        g.get(TLB);
        g.load(offset_of!(TlbEntry, writable));
        g.op(0x45);
        g.0.extend([0x0d, 0]);
    }
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, lo));
    g.op(0x6b);
    g.set(REL);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, base));
    g.get(REL);
    g.op(0x6a);
    if store {
        if i.op == Ssi { g.fr(i.t); } else { g.ar(i.t); }
        g.op(match width {
            1 => 0x3a,
            2 => 0x3b,
            _ => 0x36,
        });
        g.0.extend([0, 0]);
        g.get(6);
        g.get(TLB);
        g.load(offset_of!(TlbEntry, vbase));
        g.get(REL);
        g.c(8);
        g.op(0x76);
        g.op(0x6a);
        g.c(2);
        g.op(0x74);
        g.op(0x6a);
        g.tee(TMP);
        g.get(TMP);
        g.load(0);
        g.c(1);
        g.op(0x6a);
        g.store(0);
    } else {
        if i.op == Lsi { g.set(TMP); g.get(0); g.get(TMP); }
        g.op(match i.op {
            L8ui => 0x2d,
            L16ui => 0x2f,
            L16si => 0x2e,
            _ => 0x28,
        });
        g.0.extend([0, 0]);
        if i.op == Lsi { g.store(offset_of!(Cpu, fr) + 4 * i.t as usize); } else { g.set_ar(i.t); }
    }
    g.0.extend([0x0c, 1]);
    g.end();
    g.fallback(bi, pc, next, last, false);
    g.end();
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
    let mut func = vec![1, 23, 0x7f];
    func.extend(body);
    let mut code = vec![1];
    uleb(&mut code, func.len());
    code.extend(func);
    section(&mut out, 10, &code);
    out
}
