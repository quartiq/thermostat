#![no_std]
#![no_main]
// TODO: #![deny(warnings, unused)]

#[cfg(not(feature = "semihosting"))]
use panic_abort as _;
#[cfg(feature = "semihosting")]
use panic_semihosting as _;

use log::{info, warn};

use core::fmt::Write;
use cortex_m::asm::wfi;
use cortex_m_rt::entry;
use embedded_hal::watchdog::{WatchdogEnable, Watchdog};
use stm32f4xx_hal::{
    rcc::RccExt,
    watchdog::IndependentWatchdog,
    time::U32Ext,
    stm32::{CorePeripherals, Peripherals},
};
use smoltcp::{
    time::Instant,
    wire::EthernetAddress,
};

mod init_log;
use init_log::init_log;
mod pins;
use pins::Pins;
mod ad7172;
mod net;
mod server;
use server::Server;
mod timer;

/// Interval at which to sample the ADC input and broadcast to all
/// clients.
///
/// This should be a multiple of the `TIMER_RATE`.
const OUTPUT_INTERVAL: u32 = 1000;

#[cfg(not(feature = "semihosting"))]
const WATCHDOG_INTERVAL: u32 = 100;
#[cfg(feature = "semihosting")]
const WATCHDOG_INTERVAL: u32 = 10_000;

#[cfg(not(feature = "generate-hwaddr"))]
const NET_HWADDR: [u8; 6] = [0x02, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];


/// Initialization and main loop
#[entry]
fn main() -> ! {
    init_log();
    info!("Thermostat firmware");

    let mut cp = CorePeripherals::take().unwrap();
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    let dp = Peripherals::take().unwrap();
    stm32_eth::setup(&dp.RCC, &dp.SYSCFG);
    let clocks = dp.RCC.constrain()
        .cfgr
        .sysclk(168.mhz())
        .hclk(168.mhz())
        .pclk1(32.mhz())
        .pclk2(64.mhz())
        .freeze();

    let mut wd = IndependentWatchdog::new(dp.IWDG);
    wd.start(WATCHDOG_INTERVAL.ms());
    wd.feed();

    let pins = Pins::setup(clocks, dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOG, dp.SPI2);

    let mut adc = ad7172::Adc::new(pins.adc_spi, pins.adc_nss).unwrap();

    timer::setup(cp.SYST, clocks);

    #[cfg(not(feature = "generate-hwaddr"))]
    let hwaddr = EthernetAddress(NET_HWADDR);
    #[cfg(feature = "generate-hwaddr")]
    let hwaddr = {
        let uid = stm32f4xx_hal::signature::Uid::get();
        EthernetAddress(hash2hwaddr::generate_hwaddr(uid))
    };
    info!("Net hwaddr: {}", hwaddr);

    info!("Net startup");
    net::run(dp.ETHERNET_MAC, dp.ETHERNET_DMA, hwaddr, |iface| {
        Server::run(iface, |server| {
            let mut last_output = 0_u32;
            loop {
                let now = timer::now().0;
                let instant = Instant::from_millis(i64::from(now));
                cortex_m::interrupt::free(net::clear_pending);
                server.poll(instant)
                    .unwrap_or_else(|e| {
                        warn!("poll: {:?}", e);
                    });

                let now = timer::now().0;
                if now - last_output >= OUTPUT_INTERVAL {
                    // let adc_value = adc_input.read();
                    writeln!(server, "t={},pa3={}\r", now, 0.0 /*adc_value*/).unwrap();
                    last_output = now;
                }

                if let Some(channel) = adc.data_ready().unwrap() {
                    let data = adc.read_data().unwrap();
                    info!("ADC {}: {:08X}", channel, data);
                }

                // Update watchdog
                wd.feed();

                // cortex_m::interrupt::free(|cs| {
                //     if !net::is_pending(cs) {
                //         // Wait for interrupts
                //         wfi();
                //     }
                // });
            }
        });
    });

    unreachable!()
}
