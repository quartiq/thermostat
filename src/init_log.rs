use crate::usb;

#[cfg(not(feature = "semihosting"))]
static USB_LOGGER: usb::Logger = usb::Logger;

#[cfg(not(feature = "semihosting"))]
pub fn init_log() {
    let _ = log::set_logger(&USB_LOGGER);
}

#[cfg(feature = "semihosting")]
pub fn init_log() {
    use log::LevelFilter;
    use cortex_m_log::log::{Logger, init};
    use cortex_m_log::printer::semihosting::{InterruptOk, hio::HStdout};
    static mut LOGGER: Option<Logger<InterruptOk<HStdout>>> = None;
    let logger = Logger {
        inner: InterruptOk::<_>::stdout().expect("semihosting stdout"),
        level: LevelFilter::Info,
    };
    let logger = unsafe {
        LOGGER.get_or_insert(logger)
    };

    init(logger).expect("set logger");
}
