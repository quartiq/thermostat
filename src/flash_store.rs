use log::{info, error};
use stm32f4xx_hal::{
    flash::{Error, FlashExt},
    stm32::FLASH,
};
use sfkv::{Store, StoreBackend};

/// 16 KiB
pub const FLASH_SECTOR_SIZE: usize = 0x4000;
pub const FLASH_SECTOR: u8 = 12;
static mut BACKUP_SPACE: [u8; FLASH_SECTOR_SIZE] = [0; FLASH_SECTOR_SIZE];

extern "C" {
    // These are from memory.x
    static _config_start: usize;
    static _flash_start: usize;
}

pub struct FlashBackend {
    flash: FLASH,
}

fn get_offset() -> usize {
    unsafe {
        (&_config_start as *const usize as usize) - (&_flash_start as *const usize as usize)
    }    
}

impl StoreBackend for FlashBackend {
    type Data = [u8];

    fn data(&self) -> &Self::Data {
        &self.flash.read()[get_offset()..(get_offset() + FLASH_SECTOR_SIZE)]
    }

    type Error = Error;
    fn erase(&mut self) -> Result<(), Self::Error> {
        info!("erasing store flash");
        self.flash.unlocked().erase(FLASH_SECTOR)
    }

    fn program(&mut self, offset: usize, payload: &[u8]) -> Result<(), Self::Error> {
        self.flash.unlocked()
            .program(get_offset() + offset, payload.iter())
    }

    fn backup_space(&self) -> &'static mut [u8] {
        unsafe { &mut BACKUP_SPACE }
    }
}

pub type FlashStore = Store<FlashBackend>;

pub fn store(flash: FLASH) -> FlashStore {
    let backend = FlashBackend { flash };
    let mut store = FlashStore::new(backend);

    // just try to read the store
    match store.get_bytes_used() {
        Ok(_) => {}
        Err(e) => {
            error!("corrupt store, erasing. error: {:?}", e);
            let _ = store.erase()
                .map_err(|e| error!("flash erase failed: {:?}", e));
        }
    }

    store
}
