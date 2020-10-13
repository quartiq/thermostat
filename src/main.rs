#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(maybe_uninit_extra, maybe_uninit_ref)]
#![cfg_attr(test, allow(unused))]
// TODO: #![deny(warnings, unused)]

#[cfg(not(any(feature = "semihosting", test)))]
use panic_abort as _;
#[cfg(all(feature = "semihosting", not(test)))]
use panic_semihosting as _;

use log::{error, info, warn};

use core::fmt::Write;
use cortex_m::asm::wfi;
use cortex_m_rt::entry;
use stm32f4xx_hal::{
    hal::watchdog::{WatchdogEnable, Watchdog},
    rcc::RccExt,
    watchdog::IndependentWatchdog,
    time::{U32Ext, MegaHertz},
    stm32::{CorePeripherals, Peripherals, SCB},
};
use smoltcp::{
    time::Instant,
    socket::TcpSocket,
    wire::{EthernetAddress, Ipv4Address},
};
use uom::{
    si::{
        f64::{
            ElectricCurrent,
            ElectricPotential,
            ElectricalResistance,
            ThermodynamicTemperature,
        },
        electric_current::ampere,
        electric_potential::volt,
        electrical_resistance::ohm,
        thermodynamic_temperature::degree_celsius,
    },
};

mod init_log;
use init_log::init_log;
mod usb;
mod leds;
mod pins;
use pins::Pins;
mod ad7172;
mod ad5680;
mod net;
mod server;
use server::Server;
mod session;
use session::{Session, SessionInput};
mod command_parser;
use command_parser::{Command, ShowCommand, PwmPin};
mod timer;
mod pid;
mod steinhart_hart;
mod channels;
use channels::{CHANNELS, Channels};
mod channel;
mod channel_state;
mod config;
use config::Config;


const HSE: MegaHertz = MegaHertz(8);
#[cfg(not(feature = "semihosting"))]
const WATCHDOG_INTERVAL: u32 = 1_000;
#[cfg(feature = "semihosting")]
const WATCHDOG_INTERVAL: u32 = 30_000;

pub const EEPROM_PAGE_SIZE: usize = 8;
pub const EEPROM_SIZE: usize = 128;

pub const DEFAULT_IPV4_ADDRESS: Ipv4Address = Ipv4Address([192, 168, 1, 26]);
const TCP_PORT: u16 = 23;


fn send_line(socket: &mut TcpSocket, data: &[u8]) -> bool {
    let send_free = socket.send_capacity() - socket.send_queue();
    if data.len() > send_free + 1 {
        // Not enough buffer space, skip report for now
        warn!(
            "TCP socket has only {}/{} needed {}",
            send_free + 1, socket.send_capacity(), data.len(),
        );
    } else {
        match socket.send_slice(&data) {
            Ok(sent) if sent == data.len() => {
                let _ = socket.send_slice(b"\n");
                // success
                return true
            }
            Ok(sent) =>
                warn!("sent only {}/{} bytes", sent, data.len()),
            Err(e) =>
                error!("error sending line: {:?}", e),
        }
    }
    // not success
    false
}

fn report_to(channel: usize, channels: &mut Channels, socket: &mut TcpSocket) -> bool {
    match channels.report(channel).to_json() {
        Ok(buf) =>
            send_line(socket, &buf[..]),
        Err(e) => {
            error!("unable to serialize report: {:?}", e);
            false
        }
    }
}

/// Initialization and main loop
#[cfg(not(test))]
#[entry]
fn main() -> ! {
    init_log();
    info!("thermostat");

    let mut cp = CorePeripherals::take().unwrap();
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    let dp = Peripherals::take().unwrap();
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

    timer::setup(cp.SYST, clocks);

    let (pins, mut leds, mut eeprom, eth_pins, usb) = Pins::setup(
        clocks, dp.TIM1, dp.TIM3,
        dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOE, dp.GPIOF, dp.GPIOG,
        dp.I2C1,
        dp.SPI2, dp.SPI4, dp.SPI5,
        dp.ADC1,
        dp.OTG_FS_GLOBAL,
        dp.OTG_FS_DEVICE,
        dp.OTG_FS_PWRCLK,
    );

    leds.r1.on();
    leds.g3.off();
    leds.g4.off();

    usb::State::setup(usb);

    let mut ipv4_address = DEFAULT_IPV4_ADDRESS;
    let mut channels = Channels::new(pins);
    let _ = Config::load(&mut eeprom)
        .map(|config| {
            config.apply(&mut channels);
            ipv4_address = Ipv4Address::from_bytes(&config.ipv4_address);
        })
        .map_err(|e| warn!("error loading config: {:?}", e));
    info!("IPv4 address: {}", ipv4_address);

    // EEPROM ships with a read-only EUI-48 identifier
    let mut eui48 = [0; 6];
    eeprom.read_data(0xFA, &mut eui48).unwrap();
    let hwaddr = EthernetAddress(eui48);
    info!("EEPROM MAC address: {}", hwaddr);

    net::run(clocks, dp.ETHERNET_MAC, dp.ETHERNET_DMA, eth_pins, hwaddr, ipv4_address, |iface| {
        let mut new_ipv4_address = None;
        Server::<Session>::run(iface, |server| {
            leds.r1.off();

            loop {
                let instant = Instant::from_millis(i64::from(timer::now()));
                let updated_channel = channels.poll_adc(instant);
                if let Some(channel) = updated_channel {
                    server.for_each(|_, session| session.set_report_pending(channel.into()));
                }

                let instant = Instant::from_millis(i64::from(timer::now()));
                cortex_m::interrupt::free(net::clear_pending);
                server.poll(instant)
                    .unwrap_or_else(|e| {
                        warn!("poll: {:?}", e);
                    });

                // TCP protocol handling
                server.for_each(|mut socket, session| {
                    if ! socket.is_active() {
                        let _ = socket.listen(TCP_PORT);
                        session.reset();
                    } else if socket.may_send() && !socket.may_recv() {
                        socket.close()
                    } else if socket.can_send() && socket.can_recv() {
                        match socket.recv(|buf| session.feed(buf)) {
                            Ok(SessionInput::Nothing) => {}
                            Ok(SessionInput::Command(command)) => match command {
                                Command::Quit =>
                                    socket.close(),
                                Command::Reporting(_reporting) => {
                                    // handled by session
                                }
                                Command::Show(ShowCommand::Reporting) => {
                                    let _ = writeln!(socket, "{{ \"report\": {:?} }}", session.reporting());
                                }
                                Command::Show(ShowCommand::Input) => {
                                    for channel in 0..CHANNELS {
                                        report_to(channel, &mut channels, &mut socket);
                                    }
                                }
                                Command::Show(ShowCommand::Pid) => {
                                    for channel in 0..CHANNELS {
                                        match channels.channel_state(channel).pid.summary(channel).to_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) =>
                                                error!("unable to serialize pid summary: {:?}", e),
                                        }
                                    }
                                }
                                Command::Show(ShowCommand::Pwm) => {
                                    for channel in 0..CHANNELS {
                                        match channels.pwm_summary(channel).to_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) =>
                                                error!("unable to serialize pwm summary: {:?}", e),
                                        }
                                    }
                                }
                                Command::Show(ShowCommand::SteinhartHart) => {
                                    for channel in 0..CHANNELS {
                                        match channels.steinhart_hart_summary(channel).to_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) =>
                                                error!("unable to serialize steinhart-hart summary: {:?}", e),
                                        }
                                    }
                                }
                                Command::Show(ShowCommand::PostFilter) => {
                                    for channel in 0..CHANNELS {
                                        match channels.postfilter_summary(channel).to_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) =>
                                                error!("unable to serialize postfilter summary: {:?}", e),
                                        }
                                    }
                                }
                                Command::PwmPid { channel } => {
                                    channels.channel_state(channel).pid_engaged = true;
                                    leds.g3.on();
                                }
                                Command::Pwm { channel, pin, value } => {
                                    match pin {
                                        PwmPin::ISet => {
                                            channels.channel_state(channel).pid_engaged = false;
                                            leds.g3.off();
                                            let current = ElectricCurrent::new::<ampere>(value);
                                            channels.set_i(channel, current);
                                            channels.power_up(channel);
                                        }
                                        PwmPin::MaxV => {
                                            let voltage = ElectricPotential::new::<volt>(value);
                                            channels.set_max_v(channel, voltage);
                                        }
                                        PwmPin::MaxIPos => {
                                            let current = ElectricCurrent::new::<ampere>(value);
                                            channels.set_max_i_pos(channel, current);
                                        }
                                        PwmPin::MaxINeg => {
                                            let current = ElectricCurrent::new::<ampere>(value);
                                            channels.set_max_i_neg(channel, current);
                                        }
                                    }
                                }
                                Command::CenterPoint { channel, center } => {
                                    let (i_tec, _) = channels.get_i(channel);
                                    let state = channels.channel_state(channel);
                                    state.center = center;
                                    if !state.pid_engaged {
                                        channels.set_i(channel, i_tec);
                                    }
                                }
                                Command::Pid { channel, parameter, value } => {
                                    let pid = &mut channels.channel_state(channel).pid;
                                    use command_parser::PidParameter::*;
                                    match parameter {
                                        Target =>
                                            pid.target = value,
                                        KP =>
                                            pid.parameters.kp = value as f32,
                                        KI =>
                                            pid.parameters.ki = value as f32,
                                        KD =>
                                            pid.parameters.kd = value as f32,
                                        OutputMin =>
                                            pid.parameters.output_min = value as f32,
                                        OutputMax =>
                                            pid.parameters.output_max = value as f32,
                                        IntegralMin =>
                                            pid.parameters.integral_min = value as f32,
                                        IntegralMax =>
                                            pid.parameters.integral_max = value as f32,
                                    }
                                }
                                Command::SteinhartHart { channel, parameter, value } => {
                                    let sh = &mut channels.channel_state(channel).sh;
                                    use command_parser::ShParameter::*;
                                    match parameter {
                                        T0 => sh.t0 = ThermodynamicTemperature::new::<degree_celsius>(value),
                                        B => sh.b = value,
                                        R0 => sh.r0 = ElectricalResistance::new::<ohm>(value),
                                    }
                                }
                                Command::PostFilter { channel, rate: None } => {
                                    channels.adc.set_postfilter(channel as u8, None).unwrap();
                                }
                                Command::PostFilter { channel, rate: Some(rate) } => {
                                    let filter = ad7172::PostFilter::closest(rate);
                                    match filter {
                                        Some(filter) =>
                                            channels.adc.set_postfilter(channel as u8, Some(filter)).unwrap(),
                                        None =>
                                            error!("unable to choose postfilter for rate {:.3}", rate),
                                    }
                                }
                                Command::Load => {
                                    match Config::load(&mut eeprom) {
                                        Ok(config) => {
                                            config.apply(&mut channels);
                                            new_ipv4_address = Some(Ipv4Address::from_bytes(&config.ipv4_address));
                                        }
                                        Err(e) =>
                                            error!("unable to load eeprom config: {:?}", e),
                                    }
                                }
                                Command::Save => {
                                    let config = Config::new(&mut channels, ipv4_address);
                                    match config.save(&mut eeprom) {
                                        Ok(()) => {},
                                        Err(e) =>
                                            error!("unable to save eeprom config: {:?}", e),
                                    }
                                }
                                Command::Ipv4(address) => {
                                    new_ipv4_address = Some(Ipv4Address::from_bytes(&address));
                                }
                                Command::Reset => {
                                    for i in 0..CHANNELS {
                                        channels.power_down(i);
                                    }

                                    SCB::sys_reset();
                                }
                            }
                            Ok(SessionInput::Error(e)) => {
                                error!("session input: {:?}", e);
                                send_line(&mut socket, b"{ \"error\": \"invalid input\" }");
                            }
                            Err(_) =>
                                socket.close(),
                        }
                    } else if socket.can_send() {
                        if let Some(channel) = session.is_report_pending() {
                            if report_to(channel, &mut channels, &mut socket) {
                                session.mark_report_sent(channel);
                            }
                        }
                    }
                });

                // Apply new IPv4 address
                new_ipv4_address.map(|new_ipv4_address| {
                    server.set_ipv4_address(ipv4_address);
                    ipv4_address = new_ipv4_address;
                });

                // Update watchdog
                wd.feed();

                leds.g4.off();
                cortex_m::interrupt::free(|cs| {
                    if !net::is_pending(cs) {
                        // Wait for interrupts
                        // (Ethernet, SysTick, or USB)
                        wfi();
                    }
                });
                leds.g4.on();
            }
        });
    });

    unreachable!()
}
