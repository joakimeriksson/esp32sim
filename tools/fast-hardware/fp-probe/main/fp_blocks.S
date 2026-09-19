/* All timed blocks have the same finite initialization and return sequence.
 * Exactly 256 measured FP instructions follow each nonempty initialization.
 * The lsi_add_gap control also includes 128 integer NOP instructions. */
.section .iram1,"ax",@progbits
.macro begin name
  .p2align 4
  .global \name
  .type \name,@function
\name:
  entry a1, 16
  movi a8, 1
  float.s f0, a8, 0
  float.s f1, a8, 0
  float.s f2, a8, 0
  float.s f3, a8, 0
  float.s f4, a8, 0
  float.s f5, a8, 0
  rfr a9, f1
  s32i a9, a1, 0
.endm
.macro finish name
  rfr a2, f0
  retw.n
  .size \name, .-\name
.endm

begin fp_empty
finish fp_empty
begin fp_add_dep
.rept 256
  add.s f0, f0, f1
.endr
finish fp_add_dep
begin fp_add_ind
.rept 64
  add.s f0, f0, f1
  add.s f3, f3, f1
  add.s f4, f4, f1
  add.s f5, f5, f1
.endr
finish fp_add_ind
begin fp_mul_dep
.rept 256
  mul.s f0, f0, f1
.endr
finish fp_mul_dep
begin fp_mul_ind
.rept 64
  mul.s f0, f0, f1
  mul.s f3, f3, f1
  mul.s f4, f4, f1
  mul.s f5, f5, f1
.endr
finish fp_mul_ind
begin fp_madd_dep
.rept 256
  madd.s f0, f1, f2
.endr
finish fp_madd_dep
begin fp_madd_ind
.rept 64
  madd.s f0, f1, f2
  madd.s f3, f1, f2
  madd.s f4, f1, f2
  madd.s f5, f1, f2
.endr
finish fp_madd_ind
begin fp_float_add_dep
.rept 128
  float.s f0, a8, 0
  add.s f0, f0, f1
.endr
finish fp_float_add_dep
begin fp_float_add_ind
.rept 128
  float.s f0, a8, 0
  add.s f3, f3, f1
.endr
finish fp_float_add_ind
begin fp_mul_trunc_dep
.rept 128
  mul.s f0, f0, f1
  trunc.s a9, f0, 0
.endr
finish fp_mul_trunc_dep
begin fp_mul_trunc_ind
.rept 128
  mul.s f0, f0, f1
  trunc.s a9, f3, 0
.endr
finish fp_mul_trunc_ind
begin fp_lsi_add_dep
.rept 128
  lsi f0, a1, 0
  add.s f0, f0, f1
.endr
finish fp_lsi_add_dep
begin fp_lsi_add_gap
.rept 128
  lsi f0, a1, 0
  nop.n
  add.s f0, f0, f1
.endr
finish fp_lsi_add_gap
begin fp_lsi_add_ind
.rept 128
  lsi f0, a1, 0
  add.s f3, f3, f1
.endr
finish fp_lsi_add_ind
