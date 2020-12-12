use stm32f4xx_hal::{
    flash::{Error, FlashExt},
    stm32::FLASH,
};
use sfkv::{Store, StoreBackend};

/// 16 KiB
pub const FLASH_SECTOR_SIZE: usize = 0x4000;
pub const FLASH_SECTOR: u8 = 12;
pub const FLASH_SECTOR_OFFSET: usize = 0x10_0000;
static mut BACKUP_SPACE: [u8; FLASH_SECTOR_SIZE] = [0; FLASH_SECTOR_SIZE];

pub struct FlashBackend {
    flash: FLASH,
}

impl StoreBackend for FlashBackend {
    type Data = [u8];

    fn data(&self) -> &Self::Data {
        self.flash.read()
    }

    type Error = Error;
    fn erase(&mut self) -> Result<(), Self::Error> {
        self.flash.unlocked().erase(FLASH_SECTOR)
    }

    fn program(&mut self, offset: usize, payload: &[u8]) -> Result<(), Self::Error> {
        self.flash.unlocked()
            .program(FLASH_SECTOR_OFFSET + offset, payload.iter().cloned())
    }


    fn backup_space(&self) -> &'static mut [u8] {
        unsafe { &mut BACKUP_SPACE }
    }
}

pub type FlashStore = Store<FlashBackend>;

pub fn store(flash: FLASH) -> FlashStore {
    let backend = FlashBackend { flash };
    FlashStore::new(backend)
}
