//! Instruction lowering: scalar operations plus dispatch to the FP and PIE emitters.
use super::*;

pub(super) fn emit(
    g: &mut Gen,
    bi: &BlockInsn,
    fast: bool,
    pc: u32,
    next: u32,
    last: bool,
    cp: u32,
) -> bool {
    use crate::Op::*;
    let i = &bi.insn;
    let (r, s, t) = (i.r, i.s, i.t);
    let imm = i.imm as u32;
    if policy::floating_point(i.op) {
        float::emit(g, bi, pc, next, last, cp & 1 != 0);
        return true;
    }
    if i.op == Pie {
        if !policy::pie(i, fast) {
            return false;
        }
        pie::emit(g, bi, pc, next, last, cp & pie::CP3 != 0);
        return true;
    }
    if i.op == Rur {
        if !matches!(imm, 0 | 1) {
            return false;
        }
        // RUR ACCX_0 / ACCX_1: `Cpu::read_ur` returns the word as stored and, unlike FCR and
        // FSR, checks no coprocessor enable.
        g.cpu(offset_of!(Cpu, accx) + 4 * imm as usize);
        g.set_ar(r);
        return true;
    }
    if i.op == Rsr {
        let Some(field) = policy::rsr_field(imm) else { return false };
        g.cpu(field);
        g.set_ar(t);
        return true;
    }
    match i.op {
        Nop | NopN | Memw | Extw | Rsync | Esync | Dsync => {}
        Movi | MoviN => {
            g.c(imm);
            g.set_ar(if i.op == Movi { t } else { s });
        }
        Mov | MovN => {
            g.ar(s);
            g.set_ar(t);
        }
        Quou | Quos | Remu | Rems => emit_divide(g, bi, pc, next, last),
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
            g.op(0x42); // i64.const 32
            g.op(32);
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
            g.tee(TMP);
            g.c(31);
            g.get(TMP);
            g.c(32);
            g.op(0x49); // Clamp the unsigned count; WASM shifts otherwise wrap at 32.
            g.op(0x1b);
            g.op(0x75); // i32.shr_s
            g.set_ar(r);
        }
        Src => {
            g.ar(s);
            g.op(0xad); // i64.extend_i32_u
            g.op(0x42); // i64.const 32
            g.op(32);
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
        J => g.leave(imm),
        Jx => {
            g.price(5);
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
            g.price(if indirect { 5 } else { 2 + g.straddle as u32 });
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
            g.leave(imm);
            g.end();
        }
        Loop | Loopnez | Loopgtz => {
            g.price(4);
            // Review spike. Mirrors exec.rs: LCOUNT = AR[s] - 1, LBEG = next, LEND = target;
            // LOOPNEZ/LOOPGTZ skip the body when the count is zero / non-positive. Blocks
            // containing these never receive a retained loop prefix (see queue), so the
            // LCOUNT-delta accounting in run() is unaffected.
            g.get(0);
            g.ar(s);
            g.c(1);
            g.op(0x6b);
            g.store(LCOUNT);
            g.cpu_const(LBEG, next);
            g.cpu_const(LEND, imm);
            if i.op != Loop {
                g.ar(s);
                if i.op == Loopnez {
                    g.op(0x45); // i32.eqz
                } else {
                    g.c(0);
                    g.op(0x4c); // i32.le_s
                }
                g.begin_if();
                g.free_leave = true;
                g.leave(imm);
                g.end();
            }
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
            g.leave(imm);
            g.end();
        }
        L8ui | L16ui | L16si | L32i | L32iN | L32r | S8i | S16i | S32i | S32iN | Lsi | Ssi if fast => {
            if cp & 1 == 0 && matches!(i.op, Lsi | Ssi) { g.guard_coprocessor(1, bi, pc, next, last); }
            memory::emit(g, bi, pc, next, last);
        }
        _ => return false,
    }
    true
}

/// QUOU/QUOS/REMU/REMS. A zero divisor raises DIVIDE_BY_ZERO and QUOS of INT_MIN by -1 wraps,
/// where wasm's `i32.div_s` would trap, so both re-execute the whole instruction in the
/// interpreter; every other operand pair divides inline. (`i32.rem_s` of INT_MIN by -1 is 0 in
/// wasm, as `wrapping_rem` is, so REMS needs only the zero check.)
fn emit_divide(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool) {
    use crate::Op::*;
    let i = &bi.insn;
    g.begin_block();
    g.begin_block();
    g.ar(i.t);
    g.op(0x45); // i32.eqz
    g.bytes.extend([0x0d, 0]);
    if i.op == Quos {
        g.ar(i.s);
        g.c(0x8000_0000);
        g.op(0x46); // i32.eq
        g.ar(i.t);
        g.c(u32::MAX);
        g.op(0x46);
        g.op(0x71); // i32.and
        g.bytes.extend([0x0d, 0]);
    }
    g.ar(i.s);
    g.ar(i.t);
    g.op(match i.op { Quos => 0x6d, Quou => 0x6e, Rems => 0x6f, _ => 0x70 }); // i32.div_s/div_u/rem_s/rem_u
    g.set_ar(i.r);
    // The helper below prices its own path; only the inline quotient is charged here.
    g.price(if matches!(i.op, Quou | Quos) { 3 } else { 4 });
    g.bytes.extend([0x0c, 1]);
    g.end();
    g.fallback(bi, pc, next, last, false);
    g.end();
}

