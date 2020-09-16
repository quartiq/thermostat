#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(maybe_uninit_extra, maybe_uninit_ref)]
// TODO: #![deny(warnings, unused)]

#[cfg(not(any(feature = "semihosting", test)))]
use panic_abort as _;
#[cfg(any(feature = "semihosting", not(test)))]
use panic_semihosting as _;

use log::{info, warn};

use core::ops::DerefMut;
use core::fmt::Write;
use cortex_m::asm::wfi;
use cortex_m_rt::entry;
use stm32f4xx_hal::{
    hal::{
        self,
        watchdog::{WatchdogEnable, Watchdog},
    },
    rcc::RccExt,
    watchdog::IndependentWatchdog,
    time::{U32Ext, MegaHertz},
    stm32::{CorePeripherals, Peripherals},
};
use smoltcp::{
    time::Instant,
    wire::EthernetAddress,
};
use uom::{
    fmt::DisplayStyle::Abbreviation,
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
use session::{Session, SessionOutput};
mod command_parser;
use command_parser::{Command, ShowCommand, PwmPin};
mod timer;
mod pid;
mod steinhart_hart;
mod channels;
use channels::{CHANNELS, Channels};
mod channel;
mod channel_state;


const HSE: MegaHertz = MegaHertz(8);
#[cfg(not(feature = "semihosting"))]
const WATCHDOG_INTERVAL: u32 = 1_000;
#[cfg(feature = "semihosting")]
const WATCHDOG_INTERVAL: u32 = 30_000;

#[cfg(not(feature = "generate-hwaddr"))]
const NET_HWADDR: [u8; 6] = [0x02, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
const TCP_PORT: u16 = 23;


/// Initialization and main loop
#[cfg(not(test))]
#[entry]
fn main() -> ! {
    init_log();
    info!("tecpak");

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

    let (pins, mut leds, eth_pins, usb) = Pins::setup(
        clocks, dp.TIM1, dp.TIM3,
        dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOE, dp.GPIOF, dp.GPIOG,
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

    let mut channels = Channels::new(pins);

    #[cfg(not(feature = "generate-hwaddr"))]
    let hwaddr = EthernetAddress(NET_HWADDR);
    #[cfg(feature = "generate-hwaddr")]
    let hwaddr = {
        let uid = stm32f4xx_hal::signature::Uid::get();
        EthernetAddress(hash2hwaddr::generate_hwaddr(uid))
    };
    info!("Net hwaddr: {}", hwaddr);

    net::run(clocks, dp.ETHERNET_MAC, dp.ETHERNET_DMA, eth_pins, hwaddr, |iface| {
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
                    } else if socket.can_send() && socket.can_recv() && socket.send_capacity() - socket.send_queue() > 1024 {
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
                                    for channel in 0..CHANNELS {
                                        if let Some(adc_input) = channels.channel_state(channel).get_adc() {
                                            let vref = channels.channel_state(channel).vref;
                                            let dac_feedback = channels.read_dac_feedback(channel);

                                            let itec = channels.read_itec(channel);
                                            let tec_i = (itec - vref) / ElectricalResistance::new::<ohm>(0.4);

                                            let tec_u_meas = channels.read_tec_u_meas(channel);

                                            let state = channels.channel_state(channel);
                                            let _ = writeln!(
                                                socket, "channel {}: t={:.0} adc{}={:.3} adc_r={:.3} vref={:.3} dac_feedback={:.3} itec={:.3} tec={:.3} tec_u_meas={:.3} r={:.3}",
                                                channel, state.adc_time,
                                                channel, adc_input.into_format_args(volt, Abbreviation),
                                                state.get_sens().unwrap().into_format_args(ohm, Abbreviation),
                                                vref.into_format_args(volt, Abbreviation), dac_feedback.into_format_args(volt, Abbreviation),
                                                itec.into_format_args(volt, Abbreviation), tec_i.into_format_args(ampere, Abbreviation),
                                                tec_u_meas.into_format_args(volt, Abbreviation),
                                                ((tec_u_meas - vref) / tec_i).into_format_args(ohm, Abbreviation),
                                            );
                                        } else {
                                            let _ = writeln!(socket, "channel {}: no adc input", channel);
                                        }
                                    }
                                }
                                Command::Show(ShowCommand::Pid) => {
                                    for channel in 0..CHANNELS {
                                        let state = channels.channel_state(channel);
                                        let _ = writeln!(socket, "PID settings for channel {}", channel);
                                        let pid = &state.pid;
                                        let _ = writeln!(socket, "- target={:.4}", pid.target);
                                        macro_rules! show_pid_parameter {
                                            ($p: tt) => {
                                                let _ = writeln!(
                                                    socket, "- {}={:.4}",
                                                    stringify!($p), pid.parameters.$p
                                                );
                                            };
                                        }
                                        show_pid_parameter!(kp);
                                        show_pid_parameter!(ki);
                                        show_pid_parameter!(kd);
                                        show_pid_parameter!(integral_min);
                                        show_pid_parameter!(integral_max);
                                        show_pid_parameter!(output_min);
                                        show_pid_parameter!(output_max);
                                        if let Some(last_output) = pid.last_output {
                                            let _ = writeln!(socket, "- last_output={:.4}", last_output);
                                        }
                                        let _ = writeln!(socket, "");
                                    }
                                }
                                Command::Show(ShowCommand::Pwm) => {
                                    for channel in 0..CHANNELS {
                                        let state = channels.channel_state(channel);
                                        let _ = writeln!(
                                            socket, "channel {}: PID={}",
                                            channel,
                                            if state.pid_engaged { "engaged" } else { "disengaged" }
                                        );
                                        let i_set = channels.get_i(channel);
                                        let _ = writeln!(
                                            socket, "- i_set={:.3} / {:.3}",
                                            i_set.0.into_format_args(ampere, Abbreviation),
                                            i_set.1.into_format_args(ampere, Abbreviation),
                                        );
                                        let max_v = channels.get_max_v(channel);
                                        let _ = writeln!(
                                            socket, "- max_v={:.3} / {:.3}",
                                            max_v.0.into_format_args(volt, Abbreviation),
                                            max_v.1.into_format_args(volt, Abbreviation),
                                        );
                                        let max_i_pos = channels.get_max_i_pos(channel);
                                        let _ = writeln!(
                                            socket, "- max_i_pos={:.3} / {:.3}",
                                            max_i_pos.0.into_format_args(ampere, Abbreviation),
                                            max_i_pos.1.into_format_args(ampere, Abbreviation),
                                        );
                                        let max_i_neg = channels.get_max_i_neg(channel);
                                        let _ = writeln!(
                                            socket, "- max_i_neg={:.3} / {:.3}",
                                            max_i_neg.0.into_format_args(ampere, Abbreviation),
                                            max_i_neg.1.into_format_args(ampere, Abbreviation),
                                        );
                                    }
                                    let _ = writeln!(socket, "");
                                }
                                Command::Show(ShowCommand::SteinhartHart) => {
                                    for channel in 0..CHANNELS {
                                        let state = channels.channel_state(channel);
                                        let _ = writeln!(
                                            socket, "channel {}: Steinhart-Hart equation parameters",
                                            channel,
                                        );
                                        let _ = writeln!(socket, "- t0={}", state.sh.t0.into_format_args(degree_celsius, Abbreviation));
                                        let _ = writeln!(socket, "- b={}", state.sh.b);
                                        let _ = writeln!(socket, "- r0={}", state.sh.r0.into_format_args(ohm, Abbreviation));
                                        match (state.get_adc(), state.get_sens(), state.get_temperature()) {
                                            (Some(adc), Some(sens), Some(temp)) => {
                                                let _ = writeln!(
                                                    socket, "- adc={:.6} r={:.0} temp{}={:.3}",
                                                    adc.into_format_args(volt, Abbreviation),
                                                    sens.into_format_args(ohm, Abbreviation),
                                                    channel,
                                                    temp.into_format_args(degree_celsius, Abbreviation),
                                                );
                                            }
                                            _ => {}
                                        }
                                        let _ = writeln!(socket, "");
                                    }
                                }
                                Command::Show(ShowCommand::PostFilter) => {
                                    for channel in 0..CHANNELS {
                                        match channels.adc.get_postfilter(channel as u8).unwrap() {
                                            Some(filter) => {
                                                let _ = writeln!(
                                                    socket, "channel {}: postfilter={:.2} SPS",
                                                    channel, filter.output_rate().unwrap()
                                                );
                                            }
                                            None => {
                                                let _ = writeln!(
                                                    socket, "channel {}: no postfilter",
                                                    channel
                                                );
                                            }
                                        }
                                    }
                                }
                                Command::PwmPid { channel } => {
                                    channels.channel_state(channel).pid_engaged = true;
                                    leds.g3.on();
                                    let _ = writeln!(socket, "channel {}: PID enabled to control PWM", channel
                                    );
                                }
                                Command::Pwm { channel, pin: PwmPin::ISet, value } => {
                                    channels.channel_state(channel).pid_engaged = false;
                                    leds.g3.off();
                                    let current = ElectricCurrent::new::<ampere>(value);
                                    let (current, max) = channels.set_i(channel, current);
                                    let _ = writeln!(
                                        socket, "channel {}: i_set DAC output set to {:.3} / {:.3}",
                                        channel,
                                        current.into_format_args(ampere, Abbreviation),
                                        max.into_format_args(ampere, Abbreviation),
                                    );
                                }
                                Command::Pwm { channel, pin, value } => {
                                    match pin {
                                        PwmPin::ISet =>
                                            // Handled above
                                            unreachable!(),
                                        PwmPin::MaxV => {
                                            let voltage = ElectricPotential::new::<volt>(value);
                                            let (voltage, max) = channels.set_max_v(channel, voltage);
                                            let _ = writeln!(
                                                socket, "channel {:.3}: max_v set to {:.3} / {:.3}",
                                                channel,
                                                voltage.into_format_args(volt, Abbreviation),
                                                max.into_format_args(volt, Abbreviation),
                                            );
                                        }
                                        PwmPin::MaxIPos => {
                                            let current = ElectricCurrent::new::<ampere>(value);
                                            let (current, max) = channels.set_max_i_pos(channel, current);
                                            let _ = writeln!(
                                                socket, "channel {:.3}: max_i_pos set to {:.3} / {:.3}",
                                                channel,
                                                current.into_format_args(ampere, Abbreviation),
                                                max.into_format_args(ampere, Abbreviation),
                                            );
                                        }
                                        PwmPin::MaxINeg => {
                                            let current = ElectricCurrent::new::<ampere>(value);
                                            let (current, max) = channels.set_max_i_neg(channel, current);
                                            let _ = writeln!(
                                                socket, "channel {:.3}: max_i_neg set to {:.3} / {:.3}",
                                                channel,
                                                current.into_format_args(ampere, Abbreviation),
                                                max.into_format_args(ampere, Abbreviation),
                                            );
                                        }
                                        _ =>
                                            unreachable!(),
                                    }
                                }
                                Command::Pid { channel, parameter, value } => {
                                    let pid = &mut channels.channel_state(channel).pid;
                                    use command_parser::PidParameter::*;
                                    match parameter {
                                        Target =>
                                            pid.target = value,
                                        KP =>
                                            pid.parameters.kp = value,
                                        KI =>
                                            pid.parameters.ki = value,
                                        KD =>
                                            pid.parameters.kd = value,
                                        OutputMin =>
                                            pid.parameters.output_min = value,
                                        OutputMax =>
                                            pid.parameters.output_max = value,
                                        IntegralMin =>
                                            pid.parameters.integral_min = value,
                                        IntegralMax =>
                                            pid.parameters.integral_max = value,
                                    }
                                    // TODO: really reset PID state
                                    // after each parameter change?
                                    pid.reset();
                                    let _ = writeln!(socket, "PID parameter updated");
                                }
                                Command::SteinhartHart { channel, parameter, value } => {
                                    let sh = &mut channels.channel_state(channel).sh;
                                    use command_parser::ShParameter::*;
                                    match parameter {
                                        T0 => sh.t0 = ThermodynamicTemperature::new::<degree_celsius>(value),
                                        B => sh.b = value,
                                        R0 => sh.r0 = ElectricalResistance::new::<ohm>(value),
                                    }
                                    let _ = writeln!(socket, "Steinhart-Hart equation parameter updated");
                                }
                                Command::PostFilter { channel, rate } => {
                                    let filter = ad7172::PostFilter::closest(rate);
                                    match filter {
                                        Some(filter) => {
                                            channels.adc.set_postfilter(channel as u8, Some(filter)).unwrap();
                                            let _ = writeln!(
                                                socket, "channel {}: postfilter set to {:.2} SPS",
                                                channel, filter.output_rate().unwrap()
                                            );
                                        }
                                        None => {
                                            let _ = writeln!(socket, "Unable to choose postfilter");
                                        }
                                    }
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
                            let state = &mut channels.channel_state(usize::from(channel));
                            let adc_data = state.adc_data.unwrap_or(0);
                            let _ = writeln!(
                                socket, "t={} raw{}=0x{:06X} value={}",
                                state.adc_time, channel, adc_data,
                                state.get_adc().unwrap().into_format_args(volt, Abbreviation),
                            ).map(|_| {
                                session.mark_report_sent(channel);
                            });
                        }
                    }
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
