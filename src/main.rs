#![no_std]
#![no_main]
// TODO: #![deny(warnings, unused)]

#[cfg(not(feature = "semihosting"))]
use panic_abort as _;
#[cfg(feature = "semihosting")]
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
use command_parser::{Command, ShowCommand, PwmPin};
mod timer;
mod pid;
mod steinhart_hart;
use steinhart_hart as sh;


struct ChannelState {
    adc_data: Option<i32>,
    adc_time: Instant,
    dac_value: u32,
    pid_enabled: bool,
    pid: pid::Controller,
    sh: sh::Parameters,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState {
            adc_data: None,
            adc_time: Instant::from_secs(0),
            dac_value: 0,
            pid_enabled: false,
            pid: pid::Controller::new(pid::Parameters::default()),
            sh: sh::Parameters::default(),
        }
    }
}


const HSE: MegaHertz = MegaHertz(8);
#[cfg(not(feature = "semihosting"))]
const WATCHDOG_INTERVAL: u32 = 100;
#[cfg(feature = "semihosting")]
const WATCHDOG_INTERVAL: u32 = 10_000;

#[cfg(not(feature = "generate-hwaddr"))]
const NET_HWADDR: [u8; 6] = [0x02, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
const TCP_PORT: u16 = 23;


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
    dac0.set(0).unwrap();
    let mut dac1 = ad5680::Dac::new(pins.dac1_spi, pins.dac1_sync);
    dac1.set(0).unwrap();
    let mut pwm = pins.pwm;

    timer::setup(cp.SYST, clocks);

    #[cfg(not(feature = "generate-hwaddr"))]
    let hwaddr = EthernetAddress(NET_HWADDR);
    #[cfg(feature = "generate-hwaddr")]
    let hwaddr = {
        let uid = stm32f4xx_hal::signature::Uid::get();
        EthernetAddress(hash2hwaddr::generate_hwaddr(uid))
    };
    info!("Net hwaddr: {}", hwaddr);

    let mut channel_states: [ChannelState; CHANNELS] = [
        ChannelState::default(), ChannelState::default()
    ];

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
                                        }
                                    }
                                }
                                Command::Show(ShowCommand::Pid) => {
                                    for (channel, state) in channel_states.iter().enumerate() {
                                        let _ = writeln!(socket, "PID settings for channel {}", channel);
                                        let pid = &state.pid;
                                        let _ = writeln!(socket, "- target={:.4}", pid.get_target());
                                        let p = pid.get_parameters();
                                        macro_rules! out {
                                            ($p: tt) => {
                                                let _ = writeln!(socket, "- {}={:.4}", stringify!($p), p.$p);
                                            };
                                        }
                                        out!(kp);
                                        out!(ki);
                                        out!(kd);
                                        out!(output_min);
                                        out!(output_max);
                                        out!(integral_min);
                                        out!(integral_max);
                                        let _ = writeln!(socket, "");
                                    }
                                }
                                Command::Show(ShowCommand::Pwm) => {
                                    for (channel, state) in channel_states.iter().enumerate() {
                                        let _ = writeln!(
                                            socket, "channel {}: PID={}",
                                            channel,
                                            if state.pid_enabled { "engaged" } else { "disengaged" }
                                        );
                                        let _ = writeln!(socket, "- i_set={}/{}", state.dac_value, ad5680::MAX_VALUE);
                                        fn show_pwm_channel<S, P>(mut socket: S, name: &str, pin: &P)
                                        where
                                            S: core::fmt::Write,
                                            P: hal::PwmPin<Duty=u16>,
                                        {
                                            let _ = writeln!(
                                                socket,
                                                "- {}={}/{}",
                                                name, pin.get_duty(), pin.get_max_duty()
                                            );
                                        }
                                        match channel {
                                            0 => {
                                                show_pwm_channel(socket.deref_mut(), "max_v", &pwm.max_v0);
                                                show_pwm_channel(socket.deref_mut(), "max_i_pos", &pwm.max_i_pos0);
                                                show_pwm_channel(socket.deref_mut(), "max_i_neg", &pwm.max_i_neg0);
                                            }
                                            1 => {
                                                show_pwm_channel(socket.deref_mut(), "max_v", &pwm.max_v1);
                                                show_pwm_channel(socket.deref_mut(), "max_i_pos", &pwm.max_i_pos1);
                                                show_pwm_channel(socket.deref_mut(), "max_i_neg", &pwm.max_i_neg1);
                                            }
                                            _ => unreachable!(),
                                        }
                                        let _ = writeln!(socket, "");
                                    }
                                }
                                Command::Show(ShowCommand::SteinhartHart) => {
                                    for (channel, state) in channel_states.iter().enumerate() {
                                        let _ = writeln!(
                                            socket, "channel {}: Steinhart-Hart equation parameters",
                                            channel,
                                        );
                                        let _ = writeln!(socket, "- t0={}", state.sh.t0);
                                        let _ = writeln!(socket, "- r={}", state.sh.r);
                                        let _ = writeln!(socket, "- r0={}", state.sh.r0);
                                        let _ = writeln!(socket, "");
                                    }
                                }
                                Command::Show(ShowCommand::PostFilter) => {
                                    for (channel, _) in channel_states.iter().enumerate() {
                                        match adc.get_postfilter(channel as u8).unwrap() {
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
                                    channel_states[channel].pid_enabled = true;
                                    let _ = writeln!(socket, "channel {}: PID enabled to control PWM", channel
                                    );
                                }
                                Command::Pwm { channel, pin: PwmPin::ISet, duty } if duty <= ad5680::MAX_VALUE => {
                                    channel_states[channel].pid_enabled = false;
                                    match channel {
                                        0 => dac0.set(duty).unwrap(),
                                        1 => dac1.set(duty).unwrap(),
                                        _ => unreachable!(),
                                    }
                                    channel_states[channel].dac_value = duty;
                                    let _ = writeln!(
                                        socket, "channel {}: PWM duty cycle manually set to {}/{}",
                                        channel, duty, ad5680::MAX_VALUE
                                    );
                                }
                                Command::Pwm { pin: PwmPin::ISet, duty, .. } if duty > ad5680::MAX_VALUE => {
                                    let _ = writeln!(
                                        socket, "error: PWM duty range must not exceed {}",
                                        ad5680::MAX_VALUE
                                    );
                                }
                                Command::Pwm { channel, pin, duty } if duty <= 0xFFFF => {
                                    let duty = duty as u16;

                                    fn set_pwm_channel<P: hal::PwmPin<Duty=u16>>(pin: &mut P, duty: u16) -> u16 {
                                        pin.set_duty(duty);
                                        pin.get_max_duty()
                                    }
                                    let max = match (channel, pin) {
                                        (_, PwmPin::ISet) =>
                                            // Handled above
                                            unreachable!(),
                                        (0, PwmPin::MaxIPos) =>
                                            set_pwm_channel(&mut pwm.max_i_pos0, duty),
                                        (0, PwmPin::MaxINeg) =>
                                            set_pwm_channel(&mut pwm.max_i_neg0, duty),
                                        (0, PwmPin::MaxV) =>
                                            set_pwm_channel(&mut pwm.max_v0, duty),
                                        (1, PwmPin::MaxIPos) =>
                                            set_pwm_channel(&mut pwm.max_i_pos1, duty),
                                        (1, PwmPin::MaxINeg) =>
                                            set_pwm_channel(&mut pwm.max_i_neg1, duty),
                                        (1, PwmPin::MaxV) =>
                                            set_pwm_channel(&mut pwm.max_v1, duty),
                                        _ =>
                                            unreachable!(),
                                    };
                                    let _ = writeln!(
                                        socket, "channel {}: PWM {} reconfigured to {}/{}",
                                        channel, pin.name(), duty, max
                                    );
                                }
                                Command::Pwm { duty, .. } if duty > 0xFFFF => {
                                    let _ = writeln!(socket, "error: PWM duty range must fit 16 bits");
                                }
                                Command::Pid { channel, parameter, value } => {
                                    let pid = &mut channel_states[channel].pid;
                                    use command_parser::PidParameter::*;
                                    match parameter {
                                        Target =>
                                            pid.set_target(value),
                                        KP =>
                                            pid.update_parameters(|parameters| parameters.kp = value),
                                        KI =>
                                            pid.update_parameters(|parameters| parameters.ki = value),
                                        KD =>
                                            pid.update_parameters(|parameters| parameters.kd = value),
                                        OutputMin =>
                                            pid.update_parameters(|parameters| parameters.output_min = value),
                                        OutputMax =>
                                            pid.update_parameters(|parameters| parameters.output_max = value),
                                        IntegralMin =>
                                            pid.update_parameters(|parameters| parameters.integral_min = value),
                                        IntegralMax =>
                                            pid.update_parameters(|parameters| parameters.integral_max = value),
                                    }
                                    pid.reset();
                                    let _ = writeln!(socket, "PID parameter updated");
                                }
                                Command::SteinhartHart { channel, parameter, value } => {
                                    let sh = &mut channel_states[channel].sh;
                                    use command_parser::ShParameter::*;
                                    match parameter {
                                        T0 => sh.t0 = value,
                                        R => sh.r = value,
                                        R0 => sh.r0 = value,
                                    }
                                    sh.update();
                                    let _ = writeln!(socket, "Steinhart-Hart equation parameter updated");
                                }
                                Command::PostFilter { channel, rate } => {
                                    let filter = ad7172::PostFilter::closest(rate);
                                    match filter {
                                        Some(filter) => {
                                            adc.set_postfilter(channel as u8, Some(filter)).unwrap();
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
                                cmd => {
                                    let _ = writeln!(socket, "Not yet implemented: {:?}", cmd);
                                }
                            }
                            Ok(SessionOutput::Error(e)) => {
                                let _ = writeln!(socket, "Command error: {:?}", e);
                            }
                            Ok(o) => {
                                let _ = writeln!(socket, "Not yet implemented");
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
