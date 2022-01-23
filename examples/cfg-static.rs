//! using `#[cfg]` on `static` shouldn't cause compile errors

#![deny(unsafe_code)]
#![deny(warnings)]
#![no_main]
#![no_std]

extern crate cortex_m_rt as rt;
extern crate panic_halt;

use rt::{entry, exception};

#[entry]
fn main(
    #[cfg(never)]
    #[init(0)]
    count: &mut u32,
) -> ! {
    loop {}
}

#[exception(SysTick)]
fn on_systick(
    #[cfg(never)]
    #[init(0)]
    foo: &mut u32,
) {
}
