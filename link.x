INCLUDE memory.x

/* With multiple codegen units the rlib produced for this crate has several object files in it. */
/* Because the linker is Smart it may not look into all the object files and not pick up the */
/* .vector_table.exceptions section. But we want it to! To workaround the problem we create an */
/* undefined reference to the EXCEPTIONS symbol (located in .vector_table.exceptions); this way the */
/* linker will look at all the object of the rlib and pick up our EXCEPTIONS symbol */
EXTERN(EXCEPTIONS);

/* Create an undefined reference to the INTERRUPTS symbol. This is required to
   force the linker to *not* drop the INTERRUPTS symbol if it comes from an
   object file that's passed to the linker *before* this crate */
EXTERN(INTERRUPTS);

/* Here we create a weak aliases for exception handlers. But since DEFAULT_HANDLER is
   also a weak symbol, we have a weak aliases to a weak symbol. Such configuration is
   not supported well through language features, so we have to do this in the linker
   script. See cortex-m-rtfm/issues/39 and cortex-m-rt/issues/58 */
EXTERN(DEFAULT_HANDLER);
PROVIDE(NMI = DEFAULT_HANDLER);
PROVIDE(HARD_FAULT = DEFAULT_HANDLER);
PROVIDE(MEM_MANAGE = DEFAULT_HANDLER);
PROVIDE(BUS_FAULT = DEFAULT_HANDLER);
PROVIDE(USAGE_FAULT = DEFAULT_HANDLER);
PROVIDE(SVCALL = DEFAULT_HANDLER);
PROVIDE(PENDSV = DEFAULT_HANDLER);
PROVIDE(SYS_TICK = DEFAULT_HANDLER);
/* This handler is not needed on armv6m, but it shouldn't hurt to keed it here anyway */
PROVIDE(DEBUG_MONITOR = DEFAULT_HANDLER);

PROVIDE(_stack_start = ORIGIN(RAM) + LENGTH(RAM));

SECTIONS
{
  .vector_table ORIGIN(FLASH) : ALIGN(4)
  {
    /* Vector table */
    _svector_table = .;
    LONG(_stack_start);

    KEEP(*(.vector_table.reset_vector));

    KEEP(*(.vector_table.exceptions));
    _eexceptions = .;

    KEEP(*(.vector_table.interrupts));
    _einterrupts = .;
  } > FLASH

  PROVIDE(_stext = _einterrupts);

  .text _stext : ALIGN(4)
  {
    /* Put reset handler first in .text section so it ends up as the entry */
    /* point of the program. */
    KEEP(*(.reset_handler));

    *(.text .text.*);
  } > FLASH

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*);
    . = ALIGN(4);
  } > FLASH

  PROVIDE(_sbss = ORIGIN(RAM));
  .bss _sbss : ALIGN(4)
  {
    *(.bss .bss.*);
    . = ALIGN(4);
    _ebss = .;
  } > RAM AT > FLASH
  /* NOTE(AT > FLASH) without this LLD v6 produces a binary that crashes OpenOCD whereas LLD v7
     emits a ".rodata and .bss sections overlap" error ... This hacky workaround doesn't increase
     the binary size AFAICT */

  .data : ALIGN(4)
  {
    _sidata = LOADADDR(.data);
    _sdata = .;
    *(.data .data.*);
    . = ALIGN(4);
    _edata = .;
  } > RAM AT > FLASH

  /* The heap starts right after the .bss + .data section ends */
  _sheap = _edata;

  /* fake output .got section */
  /* Dynamic relocations are unsupported. This section is only used to detect
     relocatable code in the input files and raise an error if relocatable code
     is found */
  .got :
  {
    _sgot = .;
    KEEP(*(.got .got.*));
    _egot = .;
  } > RAM AT > FLASH

  /DISCARD/ :
  {
    *(.ARM.exidx.*);
  }
}

/* Do not exceed this mark in the error messages below                | */
ASSERT(_eexceptions - ORIGIN(FLASH) > 8, "
The exception handlers are missing. This is likely a cortex-m-rt bug.
Please file a bug report at:
https://github.com/japaric/cortex-m-rt/issues");

ASSERT(_eexceptions - ORIGIN(FLASH) == 0x40, "
Invalid '.vector_table.exceptions' section. This is likely a
cortex-m-rt bug. Please file a bug report at:
https://github.com/japaric/cortex-m-rt/issues");

ASSERT(_einterrupts - _eexceptions > 0, "
The interrupt handlers are missing. If you are not linking to a device
crate then you supply the interrupt handlers yourself. Check the
documentation.");

ASSERT(_einterrupts - _eexceptions <= 0x3c0, "
There can't be more than 240 interrupt handlers. This may be a bug in
your device crate, or you may have registered more than 240 interrupt
handlers.");

ASSERT(_einterrupts <= _stext, "
The '.text' section can't be placed inside '.vector_table' section.
Set '_stext' to an address greater than '_einterrupts'");

ASSERT(_stext < ORIGIN(FLASH) + LENGTH(FLASH), "
The '.text' section must be placed inside the FLASH memory
Set '_stext' to an address smaller than 'ORIGIN(FLASH) + LENGTH(FLASH)");

ASSERT(_sgot == _egot, "
.got section detected in the input files. Dynamic relocations are not
supported. If you are linking to C code compiled using the `gcc` crate
then modify your build script to compile the C code _without_ the
-fPIC flag. See the documentation of the `gcc::Config.fpic` method for
details.");
