#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(maybe_uninit_extra, maybe_uninit_ref, asm)]
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
    stm32::{CorePeripherals, Peripherals, SCB},
    time::{U32Ext, MegaHertz},
    watchdog::IndependentWatchdog,
};
use smoltcp::{
    time::Instant,
    socket::TcpSocket,
    wire::EthernetAddress,
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
use command_parser::{Command, Ipv4Config, PwmPin, ShowCommand};
mod timer;
mod pid;
mod steinhart_hart;
mod channels;
use channels::{CHANNELS, Channels};
mod channel;
mod channel_state;
mod config;
use config::ChannelConfig;
mod flash_store;
mod dfu;
mod iir_float;
mod iir_array;

const HSE: MegaHertz = MegaHertz(8);
#[cfg(not(feature = "semihosting"))]
const WATCHDOG_INTERVAL: u32 = 1_000;
#[cfg(feature = "semihosting")]
const WATCHDOG_INTERVAL: u32 = 30_000;

const CHANNEL_CONFIG_KEY: [&str; 2] = ["ch0", "ch1"];

const TCP_PORT: u16 = 23;

fn send_line(socket: &mut TcpSocket, data: &[u8]) -> bool {
    let send_free = socket.send_capacity() - socket.send_queue();
    if data.len() > send_free + 1 {
        // Not enough buffer space, skip report for now,
        // instead of sending incomplete line
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

    let mut store = flash_store::store(dp.FLASH);
    let mut store_value_buf = [0u8; 256];

    let mut channels = Channels::new(pins);
    for c in 0..CHANNELS {
        match store.read_value::<ChannelConfig>(CHANNEL_CONFIG_KEY[c]) {
            Ok(Some(config)) =>
                config.apply(&mut channels, c),
            Ok(None) =>
                error!("flash config not found for channel {}", c),
            Err(e) =>
                error!("unable to load config {} from flash: {:?}", c, e),
        }
    }

    // default net config:
    let mut ipv4_config = Ipv4Config {
        address: [192, 168, 1, 26],
        mask_len: 24,
        gateway: None,
    };
    match store.read_value("ipv4") {
        Ok(Some(config)) =>
            ipv4_config = config,
        Ok(None) => {}
        Err(e) =>
            error!("cannot read ipv4 config: {:?}", e),
    }

    // EEPROM ships with a read-only EUI-48 identifier
    let mut eui48 = [0; 6];
    eeprom.read_data(0xFA, &mut eui48).unwrap();
    let hwaddr = EthernetAddress(eui48);
    info!("EEPROM MAC address: {}", hwaddr);



    net::run(clocks, dp.ETHERNET_MAC, dp.ETHERNET_DMA, eth_pins, hwaddr, ipv4_config.clone(), |iface| {
        Server::<Session>::run(iface, |server| {
            leds.r1.off();
            let mut should_reset = false;

            loop {



                let mut new_ipv4_config = None;
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

                if ! should_reset {
                    // TCP protocol handling
                    server.for_each(|mut socket, session| {
                        if ! socket.is_active() {
                            let _ = socket.listen(TCP_PORT);
                            session.reset();
                        } else if socket.may_send() && !socket.may_recv() {
                            socket.close()
                        } else if socket.can_send() && socket.can_recv() {

                            match socket.recv(|buf| session.feed(buf)) {
                                // SessionInput::Nothing happens when the line reader parses a string of characters that is not
                                // followed by a newline character. Could be due to partial commands not terminated with newline,
                                // socket RX ring buffer wraps around, or when the command is sent as seperate TCP packets etc.
                                // Do nothing and feed more data to the line reader in the next loop cycle.
                                Ok(SessionInput::Nothing) => {}
                                Ok(SessionInput::Command(command)) => match command {
                                    Command::Quit =>
                                        socket.close(),
                                    Command::Reporting(_reporting) => {
                                        // handled by session
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::Show(ShowCommand::Reporting) => {
                                        let _ = writeln!(socket, "{{ \"report\": {:?} }}", session.reporting());
                                    }
                                    Command::Show(ShowCommand::Input) => {
                                        match channels.reports_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf[..]);
                                            }
                                            Err(e) => {
                                                error!("unable to serialize report: {:?}", e);
                                                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);

                                            }
                                        }
                                    }
                                    Command::Show(ShowCommand::Pid) => {
                                        match channels.pid_summaries_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) => {
                                                error!("unable to serialize pid summary: {:?}", e);
                                                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                                            }
                                        }
                                    }
                                    Command::Show(ShowCommand::Pwm) => {
                                        match channels.pwm_summaries_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) => {
                                                error!("unable to serialize pwm summary: {:?}", e);
                                                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                                            }
                                        }
                                    }
                                    Command::Show(ShowCommand::SteinhartHart) => {
                                        match channels.steinhart_hart_summaries_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) => {
                                                error!("unable to serialize steinhart-hart summaries: {:?}", e);
                                                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                                            }
                                        }
                                    }
                                    Command::Show(ShowCommand::PostFilter) => {
                                        match channels.postfilter_summaries_json() {
                                            Ok(buf) => {
                                                send_line(&mut socket, &buf);
                                            }
                                            Err(e) => {
                                                error!("unable to serialize postfilter summary: {:?}", e);
                                                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                                            }
                                        }
                                    }
                                    Command::Show(ShowCommand::Ipv4) => {
                                        let (cidr, gateway) = net::split_ipv4_config(ipv4_config.clone());
                                        let _ = write!(socket, "{{\"addr\":\"{}\"", cidr);
                                        gateway.map(|gateway| write!(socket, ",\"gateway\":\"{}\"", gateway));
                                        let _ = writeln!(socket, "}}");
                                    }
                                    Command::PwmPid { channel } => {
                                        channels.channel_state(channel).pid_engaged = true;
                                        leds.g3.on();
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::Pwm { channel, pin, value } => {
                                        match pin {
                                            PwmPin::ISet => {
                                                channels.channel_state(channel).pid_engaged = false;
                                                channels.channel_state(channel).iir_engaged = false;
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
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::CenterPoint { channel, center } => {
                                        let i_tec = channels.get_i(channel);
                                        let state = channels.channel_state(channel);
                                        state.center = center;
                                        if !state.pid_engaged {
                                            channels.set_i(channel, i_tec);
                                        }
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::Pid { channel, parameter, value } => {
                                        let pid = &mut channels.channel_state(channel).pid;
                                        use command_parser::PidParameter::*;
                                        match parameter {
                                            Target => {
                                                pid.target = value;
                                            },
                                            KP =>
                                                pid.parameters.kp = value as f32,
                                            KI =>
                                                pid.update_ki(value as f32),
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
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::SteinhartHart { channel, parameter, value } => {
                                        let sh = &mut channels.channel_state(channel).sh;
                                        use command_parser::ShParameter::*;
                                        match parameter {
                                            T0 => sh.t0 = ThermodynamicTemperature::new::<degree_celsius>(value),
                                            B => sh.b = value,
                                            R0 => sh.r0 = ElectricalResistance::new::<ohm>(value),
                                        }
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::PostFilter { channel, rate: None } => {
                                        channels.adc.set_postfilter(channel as u8, None).unwrap();
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::PostFilter { channel, rate: Some(rate) } => {
                                        let filter = ad7172::PostFilter::closest(rate);
                                        match filter {
                                            Some(filter) => {
                                                channels.adc.set_postfilter(channel as u8, Some(filter)).unwrap();
                                                send_line(&mut socket, b"{}");
                                            }
                                            None => {
                                                error!("unable to choose postfilter for rate {:.3}", rate);
                                                send_line(&mut socket, b"{{\"error\": \"unable to choose postfilter rate\"}}");
                                            }
                                        }
                                    }
                                    Command::Load { channel } => {
                                        for c in 0..CHANNELS {
                                            if channel.is_none() || channel == Some(c) {
                                                match store.read_value::<ChannelConfig>(CHANNEL_CONFIG_KEY[c]) {
                                                    Ok(Some(config)) => {
                                                        config.apply(&mut channels, c);
                                                        send_line(&mut socket, b"{}");
                                                    }
                                                    Ok(None) => {
                                                        error!("flash config not found");
                                                        send_line(&mut socket, b"{{\"error\": \"flash config not found\"}}");
                                                    }
                                                    Err(e) => {
                                                        error!("unable to load config from flash: {:?}", e);
                                                        let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Command::Save { channel } => {
                                        for c in 0..CHANNELS {
                                            if channel.is_none() || channel == Some(c) {
                                                let config = ChannelConfig::new(&mut channels, c);
                                                match store.write_value(CHANNEL_CONFIG_KEY[c], &config, &mut store_value_buf) {
                                                    Ok(()) => {
                                                        send_line(&mut socket, b"{}");
                                                    }
                                                    Err(e) => {
                                                        error!("unable to save channel {} config to flash: {:?}", c, e);
                                                        let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Command::Ipv4(config) => {
                                        let _ = store
                                            .write_value("ipv4", &config, [0; 16])
                                            .map_err(|e| error!("unable to save ipv4 config to flash: {:?}", e));
                                        new_ipv4_config = Some(config);
                                        send_line(&mut socket, b"{}");
                                    }
                                    Command::Reset => {
                                        for i in 0..CHANNELS {
                                            channels.power_down(i);
                                        }
                                        should_reset = true;
                                    }
                                    Command::Dfu => {
                                        for i in 0..CHANNELS {
                                            channels.power_down(i);
                                        }
                                        unsafe {
                                            dfu::set_dfu_trigger();
                                        }
                                        should_reset = true;
                                    }
                                    Command::Iir {channel, values} => {
                                        let iir = &mut channels.channel_state(channel).iir;
                                        iir.ba = values;
                                        send_line(&mut socket, b"Coefficients set [b0,b1,b2,a1,a2]:");
                                        let _ = writeln!(socket, "{:?}", iir.ba);
                                    }
                                    Command::Iirtarget {channel, target} => {
                                        let iir = &mut channels.channel_state(channel).iir;
                                        iir.target = target;
                                        send_line(&mut socket, b"test");
                                        let _ = writeln!(socket, "{:?}", target);
                                    }
                                    Command::Show(ShowCommand::Iir) => {
                                        let iir = &mut channels.channel_state(0 as usize).iir;
                                        send_line(&mut socket, b"Channel 1 ----------------------------------");
                                        send_line(&mut socket, b"Coefficients set [b0,b1,b2,a1,a2]:");
                                        let _ = writeln!(socket, "{:?}", iir.ba);
                                        send_line(&mut socket, b"target:");
                                        let _ = writeln!(socket, "{:?}", iir.target);
                                        send_line(&mut socket, b"engaged:");
                                        let _ = writeln!(socket, "{:?}", channels.channel_state(0 as usize).iir_engaged);
                                        send_line(&mut socket, b"Channel 2 ----------------------------------");
                                        let iir = &mut channels.channel_state(1 as usize).iir;
                                        send_line(&mut socket, b"Coefficients set [b0,b1,b2,a1,a2]:");
                                        let _ = writeln!(socket, "{:?}", iir.ba);
                                        send_line(&mut socket, b"target:");
                                        let _ = writeln!(socket, "{:?}", iir.target);
                                        send_line(&mut socket, b"engaged:");
                                        let _ = writeln!(socket, "{:?}", channels.channel_state(1 as usize).iir_engaged);
                                    }
                                    Command::PwmIir { channel } => {
                                        channels.channel_state(channel).iir_engaged = true;
                                        leds.g3.on();
                                        send_line(&mut socket, b"{}");
                                    }

                                    Command::PwmMatrix { channel, iirout} => {
                                        channels.channel_state(channel).matrix_engaged = iirout;
                                        leds.g3.on();
                                        send_line(&mut socket, b"{matrix iir engaged:}");
                                        let _ = writeln!(socket, "{:?}", iirout);
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
                                match channels.reports_json() {
                                    Ok(buf) => {
                                        send_line(&mut socket, &buf[..]);
                                        session.mark_report_sent(channel);
                                    }
                                    Err(e) => {
                                        error!("unable to serialize report: {:?}", e);

                                    }
                                }
                            }
                        }
                    });
                } else {
                    // Should reset, close all TCP sockets.
                    let mut any_socket_alive = false;
                    server.for_each(|mut socket, _| {
                        if socket.is_active() {
                            socket.abort();
                            any_socket_alive = true;
                        }
                    });
                    // Must let loop run for one more cycle to poll server for RST to be sent,
                    // this makes sure system does not reset right after socket.abort() is called.
                    if !any_socket_alive {
                        SCB::sys_reset();
                    }
                }

                // Apply new IPv4 address/gateway
                new_ipv4_config.take()
                    .map(|config| {
                        server.set_ipv4_config(config.clone());
                        ipv4_config = config;
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
