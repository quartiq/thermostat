use cortex_m_rt::pre_init;
use stm32f4xx_hal::stm32::{RCC, SYSCFG};

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
        let rcc = &*RCC::ptr();
        rcc.apb2enr.modify(|_, w| w.syscfgen().set_bit());

        // Bypass BOOT pins and remap bootloader to 0x00000000
        let syscfg = &*SYSCFG::ptr() ;
        syscfg.memrm.write(|w| w.mem_mode().bits(0b01));  

        // Impose instruction and memory barriers
        cortex_m::asm::isb();
        cortex_m::asm::dsb();
        
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
