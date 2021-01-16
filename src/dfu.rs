use cortex_m_rt::{pre_init};

const DFU_TRIG_MSG: u32 = 0xDECAFBAD;

extern "C" {
    // This symbol comes from memory.x
    static mut _dfu_msg: u32;
}

pub unsafe fn set_dfu_trigger() {
    _dfu_msg = DFU_TRIG_MSG;
}

/// Called by reset handler in lib.rs immediately after reset.
/// This function should not be called outside of reset handler as 
/// bootloader expects MCU to be in reset state when called.
#[cfg(target_arch = "arm")]
#[pre_init]
unsafe fn __pre_init() {
    if _dfu_msg == DFU_TRIG_MSG {
        _dfu_msg = 0x00000000;

        // Enable system config controller clock
        const RCC_APB2ENR: *mut u32 = 0xE000_ED88 as *mut u32;
        const RCC_APB2ENR_ENABLE_SYSCFG_CLOCK: u32 = 0x00004000;

        core::ptr::write_volatile(
            RCC_APB2ENR,
            *RCC_APB2ENR | RCC_APB2ENR_ENABLE_SYSCFG_CLOCK,
        );

        // Bypass BOOT pins and remap bootloader to 0x00000000
        const SYSCFG_MEMRMP: *mut u32 = 0x40013800 as *mut u32;
        const SYSCFG_MEMRMP_MAP_ROM: u32 = 0x00000001;

        core::ptr::write_volatile(
            SYSCFG_MEMRMP,
            *SYSCFG_MEMRMP | SYSCFG_MEMRMP_MAP_ROM,
        );
        
        asm!(
            // Set stack pointer to bootloader location
            "LDR R0, =0x1FFF0000",
            "LDR SP,[R0, #0]",
            // Jump to bootloader
            "LDR R0,[R0, #4]",
            "BX R0",
        );
    }
}