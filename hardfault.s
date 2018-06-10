  .global HardFault
  .thumb_func
HardFault:
  movs r0, lr
  lsl  r0, r0, #29     // Test bit[2] of EXC_RETURN to determine thread mode
  bmi  HardFault.PSP

HardFault.MSP:
  mrs r0, MSP          // Use Main Stack Pointer (if in privileged mode)
  bl  UserHardFault    // Jump to hard fault handler

HardFault.PSP:
  mrs r0, PSP          // Use Process Stack Pointer (if in user mode)
  bl  UserHardFault    // Jump to hard fault handler
