.text
.global operand_regressions
operand_regressions:
addi.n a2, a3, 15
nsau a2, a3
srli a2, a3, 12
lsx f15, a2, a3
moveqz.s f0, f15, a2
nsa a2, a3
extui a2, a3, 15, 16
srai a2, a3, 31
l32e a2, a3, -4
s32e a2, a3, -4
s32nb a2, a3, 60
ssx f15, a2, a3
lsxp f15, a2, a3
ssxp f15, a2, a3
movnez.s f0, f15, a2
movltz.s f0, f15, a2
movgez.s f0, f15, a2
rer a2, a3
wer a2, a3
ritlb0 a2, a3
ritlb1 a2, a3
rdtlb0 a2, a3
rdtlb1 a2, a3
pitlb a2, a3
pdtlb a2, a3
witlb a2, a3
wdtlb a2, a3
mul.dd.ll m1, m3
mul.ad.ll a2, m3
mul.da.ll m1, a3
ldinc m0, a3
add a2, a3, a15
addi.n a15, a3, 15
ssx f2, a15, a3
moveqz.s f0, f1, a15
movnez a15, a2, a3
movf a2, a3, b15
xsr a2, windowbase
wsr a2, windowbase
rotw 1
entry a1, 16
retw
