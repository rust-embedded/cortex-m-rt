//! Core peripheral support when using `init_array`
//!
//! Allows passing on any subset of peripherals normally `take`n from
//! `cortex_m::peripheral::Peripherals`. Once in `main` `InitArrayPeripherals`
//! cannot be given back.
//!
//! ``` no_run
//! extern crate cortex_m;
//!
//! use cortex_m_rt::init_array::InitArrayPeripherals;
//!
//! init_array!(before_main, {
//!     let mut peripherals = InitArrayPeripherals::take().unwrap();
//!     peripherals.DWT.enable_cycle_counter();
//!     InitArrayPeripherals::give(peripherals);
//! });
//!
//! fn main() {
//!     let ok = InitArrayPeripherals::take().unwrap();
//!     let panics = InitArrayPeripherals::take().unwrap();
//! }
//! ```
//!
//! Select peripherals can be dropped by creating a new `InitArrayPeripherals`
//! with `None` in place of the peripheral.
//!
//! ``` no_run
//! extern crate cortex_m;
//!
//! use cortex_m::peripheral::Peripherals;
//! use cortex_m_rt::init_array::InitArrayPeripherals;
//!
//! init_array!(before_main, {
//!     let mut Peripherals {
//!         CBP,
//!         CPUID,
//!         DCB,
//!         DWT,
//!         FPB,
//!         FPU,
//!         ITM,
//!         MPU,
//!         NVIC,
//!         SCB,
//!         SYST,
//!         TPIU,
//!     } = Peripherals::take().unwrap();
//!     DWT.enable_cycle_counter();
//!     let pass_on_peripherals = InitArrayPeripherals {
//!         Some(CBP),
//!         Some(CPUID),
//!         Some(DCB),
//!         None,       // Don't pass on DWT
//!         Some(FPB),
//!         Some(FPU),
//!         Some(ITM),
//!         Some(MPU),
//!         Some(NVIC),
//!         Some(SCB),
//!         Some(SYST),
//!         Some(TPIU),
//!     }
//!     InitArrayPeripherals::give(pass_on_peripherals);
//! });
//!
//! fn main() {
//!     let peripherals = InitArrayPeripherals::take().unwrap();
//!     assert!(peripherals.DWT.is_none());
//! }
//! ```

#![allow(private_no_mangle_statics)]

use cortex_m::peripheral::{self, Peripherals};
use cortex_m::interrupt;

/// Core peripherals handed back from `init_array`
#[allow(non_snake_case)]
pub struct InitArrayPeripherals {
    /// Cache and branch predictor maintenance operations
    #[cfg(any(armv7m, target_arch = "x86_64"))]
    pub CBP: Option<peripheral::CBP>,
    /// CPUID
    pub CPUID: Option<peripheral::CPUID>,
    /// Debug Control Block
    pub DCB: Option<peripheral::DCB>,
    /// Data Watchpoint and Trace unit
    pub DWT: Option<peripheral::DWT>,
    /// Flash Patch and Breakpoint unit
    #[cfg(any(armv7m, target_arch = "x86_64"))]
    pub FPB: Option<peripheral::FPB>,
    /// Floating Point Unit
    #[cfg(any(has_fpu, target_arch = "x86_64"))]
    pub FPU: Option<peripheral::FPU>,
    /// Instrumentation Trace Macrocell
    #[cfg(any(armv7m, target_arch = "x86_64"))]
    pub ITM: Option<peripheral::ITM>,
    /// Memory Protection Unit
    pub MPU: Option<peripheral::MPU>,
    /// Nested Vector Interrupt Controller
    pub NVIC: Option<peripheral::NVIC>,
    /// System Control Block
    pub SCB: Option<peripheral::SCB>,
    /// SysTick: System Timer
    pub SYST: Option<peripheral::SYST>,
    /// Trace Port Interface Unit;
    #[cfg(any(armv7m, target_arch = "x86_64"))]
    pub TPIU: Option<peripheral::TPIU>,
}

#[no_mangle]
static mut INIT_ARRAY_PERIPHERALS: Option<InitArrayPeripherals> = None;
#[no_mangle]
static mut INIT_ARRAY_DONE: bool = false;

impl InitArrayPeripherals {
    /// Returns all the core peripherals. Can be given back in an `init_array`
    /// function by calling `give`.
    pub fn take() -> Option<Self> {
        interrupt::free(|_| { unsafe { INIT_ARRAY_PERIPHERALS.take() } }).or_else(|| {
            Peripherals::take().map(InitArrayPeripherals::from)
        })
    }

    /// Give the peripherals back for use in `main` after using them in
    /// `init_array`. Does nothing if called after `init_array`.
    pub fn give<P: Into<InitArrayPeripherals>>(p: P) {
        interrupt::free(|_| {
            if unsafe { !INIT_ARRAY_DONE } {
                unsafe { INIT_ARRAY_PERIPHERALS = Some(p.into()) }
            }
        })
    }

    pub(crate) fn done() {
        interrupt::free(|_| {
            unsafe { INIT_ARRAY_DONE = true };
        })
    }
}

#[allow(non_snake_case)]
impl From<Peripherals> for InitArrayPeripherals {
    fn from(p: Peripherals) -> Self {
        let Peripherals {
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            CBP,
            CPUID,
            DCB,
            DWT,
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            FPB,
            #[cfg(any(has_fpu, target_arch = "x86_64"))]
            FPU,
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            ITM,
            MPU,
            NVIC,
            SCB,
            SYST,
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            TPIU,
            ..
        } = p;

        InitArrayPeripherals {
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            CBP: Some(CBP),
            CPUID: Some(CPUID),
            DCB: Some(DCB),
            DWT: Some(DWT),
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            FPB: Some(FPB),
            #[cfg(any(has_fpu, target_arch = "x86_64"))]
            FPU: Some(FPU),
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            ITM: Some(ITM),
            MPU: Some(MPU),
            NVIC: Some(NVIC),
            SCB: Some(SCB),
            SYST: Some(SYST),
            #[cfg(any(armv7m, target_arch = "x86_64"))]
            TPIU: Some(TPIU),
        }
    }
}
