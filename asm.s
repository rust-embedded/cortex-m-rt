  .section .text.HardFault
  .global HardFault
  .thumb_func
HardFault:
  mrs r0, MSP
  bl UserHardFault

  .section .text.__zero_bss
  .global __zero_bss
  .thumb_func
  .syntax unified
__zero_bss:
  movs	r2, #0
  b 2f
1:
  stmia	r0!, {r2}
2:
  cmp	r0, r1
  bcc.n	1b
  bx lr

  .section .text.__init_data
  .global __init_data
  .thumb_func
  .syntax unified
__init_data:
  b 2f
1:
  ldmia   r1!, {r3}
  stmia   r0!, {r3}
2:
  cmp     r0, r2
  bcc.n   1b
  bx lr
