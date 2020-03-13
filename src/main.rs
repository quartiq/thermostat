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
    time::{U32Ext, MegaHertz},
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
mod ad5680;
mod net;
mod server;
use server::Server;
mod session;
use session::{CHANNELS, Session, SessionOutput};
mod command_parser;
use command_parser::{Command, ShowCommand, PwmSetup, PwmMode};
mod timer;


#[derive(Clone, Copy, Debug)]
struct ChannelState {
    adc_data: Option<i32>,
    adc_time: Instant,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState {
            adc_data: None,
            adc_time: Instant::from_secs(0),
        }
    }
}


#[cfg(not(feature = "semihosting"))]
const WATCHDOG_INTERVAL: u32 = 100;
#[cfg(feature = "semihosting")]
const WATCHDOG_INTERVAL: u32 = 10_000;

#[cfg(not(feature = "generate-hwaddr"))]
const NET_HWADDR: [u8; 6] = [0x02, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];

const TCP_PORT: u16 = 23;


const HSE: MegaHertz = MegaHertz(8);

/// Initialization and main loop
#[entry]
fn main() -> ! {
    init_log();
    info!("tecpak");

    let mut cp = CorePeripherals::take().unwrap();
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    let dp = Peripherals::take().unwrap();
    stm32_eth::setup(&dp.RCC, &dp.SYSCFG);

    let clocks = dp.RCC.constrain()
        .cfgr
        .use_hse(HSE)
        .sysclk(168.mhz())
        .hclk(168.mhz())
        .pclk1(32.mhz())
        .pclk2(64.mhz())
        .freeze();

    let mut wd = IndependentWatchdog::new(dp.IWDG);
    wd.start(WATCHDOG_INTERVAL.ms());
    wd.feed();

    let pins = Pins::setup(
        clocks, dp.TIM1, dp.TIM3,
        dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOE, dp.GPIOF, dp.GPIOG,
        dp.SPI2, dp.SPI4, dp.SPI5
    );

    let mut adc = ad7172::Adc::new(pins.adc_spi, pins.adc_nss).unwrap();
    let mut dac0 = ad5680::Dac::new(pins.dac0_spi, pins.dac0_sync);
    dac0.set(0);
    let mut dac1 = ad5680::Dac::new(pins.dac1_spi, pins.dac1_sync);
    dac1.set(0);

    timer::setup(cp.SYST, clocks);

    #[cfg(not(feature = "generate-hwaddr"))]
    let hwaddr = EthernetAddress(NET_HWADDR);
    #[cfg(feature = "generate-hwaddr")]
    let hwaddr = {
        let uid = stm32f4xx_hal::signature::Uid::get();
        EthernetAddress(hash2hwaddr::generate_hwaddr(uid))
    };
    info!("Net hwaddr: {}", hwaddr);

    let mut channel_states = [ChannelState::default(); CHANNELS];

    net::run(dp.ETHERNET_MAC, dp.ETHERNET_DMA, hwaddr, |iface| {
        Server::<Session>::run(iface, |server| {
            loop {
                let now = timer::now().0;
                let instant = Instant::from_millis(i64::from(now));
                cortex_m::interrupt::free(net::clear_pending);
                server.poll(instant)
                    .unwrap_or_else(|e| {
                        warn!("poll: {:?}", e);
                    });

                // ADC input
                adc.data_ready().unwrap().map(|channel| {
                    let data = adc.read_data().unwrap();

                    let state = &mut channel_states[usize::from(channel)];
                    state.adc_data = Some(data);
                    state.adc_time = instant;
                    server.for_each(|_, session| session.set_report_pending(channel.into()));
                });

                // TCP protocol handling
                server.for_each(|mut socket, session| {
                    if ! socket.is_open() {
                        let _ = socket.listen(TCP_PORT);
                        session.reset();
                    } else if socket.can_send() && socket.can_recv() && socket.send_capacity() - socket.send_queue() > 128 {
                        match socket.recv(|buf| session.feed(buf)) {
                            Ok(SessionOutput::Nothing) => {}
                            Ok(SessionOutput::Command(command)) => match command {
                                Command::Quit =>
                                    socket.close(),
                                Command::Reporting(reporting) => {
                                    let _ = writeln!(socket, "report={}", if reporting { "on" } else { "off" });
                                }
                                Command::Show(ShowCommand::Reporting) => {
                                    let _ = writeln!(socket, "report={}", if session.reporting() { "on" } else { "off" });
                                }
                                Command::Show(ShowCommand::Input) => {
                                    for (channel, state) in channel_states.iter().enumerate() {
                                        if let Some(adc_data) = state.adc_data {
                                            let _ = writeln!(
                                                socket, "t={} raw{}=0x{:06X}",
                                                state.adc_time, channel, adc_data
                                            );
                                            // TODO: show pwm status et al
                                        }
                                    }
                                }
                                Command::Show(ShowCommand::Pid) => {
                                    // for (channel, state) in states.iter().enumerate() {
                                    //     let _ = writeln!(socket, "PID settings for channel {}", channel);
                                    //     let pid = &state.pid;
                                    //     let _ = writeln!(socket, "- target={:.4}", pid.get_target());
                                    //     let p = pid.get_parameters();
                                    //     macro_rules! out {
                                    //         ($p: tt) => {
                                    //             let _ = writeln!(socket, "- {}={:.4}", stringify!($p), p.$p);
                                    //         };
                                    //     }
                                    //     out!(kp);
                                    //     out!(ki);
                                    //     out!(kd);
                                    //     out!(output_min);
                                    //     out!(output_max);
                                    //     out!(integral_min);
                                    //     out!(integral_max);
                                    //     let _ = writeln!(socket, "");
                                    // }
                                }
                                Command::Show(ShowCommand::Pwm) => {
                                    // for (channel, state) in states.iter().enumerate() {
                                    //     let _ = writeln!(
                                    //         socket, "channel {}: PID={}",
                                    //         channel,
                                    //         if state.pid_enabled { "engaged" } else { "disengaged" }
                                    //     );
                                    //     for pin in TecPin::VALID_VALUES {
                                    //         let (width, total) = match channel {
                                    //             0 => tec0.get(*pin),
                                    //             1 => tec1.get(*pin),
                                    //             _ => unreachable!(),
                                    //         };
                                    //         let _ = writeln!(socket, "- {}={}/{}", pin, width, total);
                                    //     }
                                    //     let _ = writeln!(socket, "");
                                    // }
                                }
                                Command::Show(ShowCommand::SteinhartHart) => {
                                    // for (channel, state) in states.iter().enumerate() {
                                    //     let _ = writeln!(
                                    //         socket, "channel {}: Steinhart-Hart equation parameters",
                                    //         channel,
                                    //     );
                                    //     let _ = writeln!(socket, "- a={}", state.sh.a);
                                    //     let _ = writeln!(socket, "- b={}", state.sh.b);
                                    //     let _ = writeln!(socket, "- c={}", state.sh.c);
                                    //     let _ = writeln!(socket, "- parallel_r={}", state.sh.parallel_r);
                                    //     let _ = writeln!(socket, "");
                                    // }
                                }
                                Command::Show(ShowCommand::PostFilter) => {
                                    // for (channel, _) in states.iter().enumerate() {
                                    //     match adc.get_postfilter(channel as u8).unwrap() {
                                    //         Some(filter) => {
                                    //             let _ = writeln!(
                                    //                 socket, "channel {}: postfilter={:.2} SPS",
                                    //                 channel, filter.output_rate().unwrap()
                                    //             );
                                    //         }
                                    //         None => {
                                    //             let _ = writeln!(
                                    //                 socket, "channel {}: no postfilter",
                                    //                 channel
                                    //             );
                                    //         }
                                    //     }
                                    // }
                                }
                                Command::Pwm { channel, setup: PwmSetup::ISet(PwmMode::Pid) } => {
                                    // states[channel].pid_enabled = true;
                                    // let _ = writeln!(socket, "channel {}: PID enabled to control PWM", channel
                                    // );
                                }
                                Command::Pwm { channel, setup: PwmSetup::ISet(PwmMode::Manual(config))} => {
                                    // states[channel].pid_enabled = false;
                                    // let PwmConfig { width, total } = config;
                                    // match channel {
                                    //     0 => tec0.set(TecPin::ISet, width, total),
                                    //     1 => tec1.set(TecPin::ISet, width, total),
                                    //     _ => unreachable!(),
                                    // }
                                    // let _ = writeln!(
                                    //     socket, "channel {}: PWM duty cycle manually set to {}/{}",
                                    //     channel, config.width, config.total
                                    // );
                                }
                                Command::Pwm { channel, setup } => {
                                    // let (pin, config) = match setup {
                                    //     PwmSetup::ISet(_) =>
                                    //     // Handled above
                                    //         unreachable!(),
                                    //     PwmSetup::MaxIPos(config) =>
                                    //         (TecPin::MaxIPos, config),
                                    //     PwmSetup::MaxINeg(config) =>
                                    //         (TecPin::MaxINeg, config),
                                    //     PwmSetup::MaxV(config) =>
                                    //         (TecPin::MaxV, config),
                                    // };
                                    // let PwmConfig { width, total } = config;
                                    // match channel {
                                    //     0 => tec0.set(pin, width, total),
                                    //     1 => tec1.set(pin, width, total),
                                    //     _ => unreachable!(),
                                    // }
                                    // let _ = writeln!(
                                    //     socket, "channel {}: PWM {} reconfigured to {}/{}",
                                    //     channel, pin, width, total
                                    // );
                                }
                                Command::Pid { channel, parameter, value } => {
                                    // let pid = &mut states[channel].pid;
                                    // use command_parser::PidParameter::*;
                                    // match parameter {
                                    //     Target =>
                                    //         pid.set_target(value),
                                    //     KP =>
                                    //         pid.update_parameters(|parameters| parameters.kp = value),
                                    //     KI =>
                                    //         pid.update_parameters(|parameters| parameters.ki = value),
                                    //     KD =>
                                    //         pid.update_parameters(|parameters| parameters.kd = value),
                                    //     OutputMin =>
                                    //         pid.update_parameters(|parameters| parameters.output_min = value),
                                    //     OutputMax =>
                                    //         pid.update_parameters(|parameters| parameters.output_max = value),
                                    //     IntegralMin =>
                                    //         pid.update_parameters(|parameters| parameters.integral_min = value),
                                    //     IntegralMax =>
                                    //         pid.update_parameters(|parameters| parameters.integral_max = value),
                                    // }
                                    // pid.reset();
                                    // let _ = writeln!(socket, "PID parameter updated");
                                }
                                Command::SteinhartHart { channel, parameter, value } => {
                                    // let sh = &mut states[channel].sh;
                                    // use command_parser::ShParameter::*;
                                    // match parameter {
                                    //     A => sh.a = value,
                                    //     B => sh.b = value,
                                    //     C => sh.c = value,
                                    //     ParallelR => sh.parallel_r = value,
                                    // }
                                    // let _ = writeln!(socket, "Steinhart-Hart equation parameter updated");
                                }
                                Command::PostFilter { channel, rate } => {
                                    // let filter = ad7172::PostFilter::closest(rate);
                                    // match filter {
                                    //     Some(filter) => {
                                    //         adc.set_postfilter(channel as u8, Some(filter)).unwrap();
                                    //         let _ = writeln!(
                                    //             socket, "channel {}: postfilter set to {:.2} SPS",
                                    //             channel, filter.output_rate().unwrap()
                                    //         );
                                    //     }
                                    //     None => {
                                    //         let _ = writeln!(socket, "Unable to choose postfilter");
                                    //     }
                                    // }
                                }
                            }
                            Ok(SessionOutput::Error(e)) => {
                                let _ = writeln!(socket, "Command error: {:?}", e);
                            }
                            Err(_) =>
                                socket.close(),
                        }
                    } else if socket.can_send() && socket.send_capacity() - socket.send_queue() > 256 {
                        while let Some(channel) = session.is_report_pending() {
                            let state = &mut channel_states[usize::from(channel)];
                            let _ = writeln!(
                                socket, "t={} raw{}=0x{:04X}",
                                state.adc_time, channel, state.adc_data.unwrap_or(0)
                            ).map(|_| {
                                session.mark_report_sent(channel);
                            });
                        }
                    }
                });

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
