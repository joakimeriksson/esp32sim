use std::mem::{align_of, offset_of, size_of};
use xtensa_lx7::Cpu;
fn main() {
    println!("size={} align={}", size_of::<Cpu>(), align_of::<Cpu>());
    macro_rules! fields { ($($field:ident),*) => { $(println!("{}={}", stringify!($field), offset_of!(Cpu, $field));)* }; }
    fields!(pc, ar, windowbase, windowstart, ps, sar, lbeg, lend, lcount, br, ccount, approximate_cpi, ccompare, qr, waiting, insn_count, blocks, boundary_bloom, jit_trap, timing_extra, price_control, icache_fill, icache_misses, fetch_cache);
}
