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
use session::{Session, SessionOutput};
mod command_parser;
use command_parser::{Command, ShowCommand, PwmPin};
mod timer;
mod units;
use units::{Amps, Ohms, Volts};
mod pid;
mod steinhart_hart;
mod channels;
use channels::{CHANNELS, Channels};
mod channel;
mod channel_state;


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
        dp.SPI2, dp.SPI4, dp.SPI5,
        dp.ADC1, dp.ADC2,
    );
    let mut channels = Channels::new(pins);
    timer::setup(cp.SYST, clocks);

    #[cfg(not(feature = "generate-hwaddr"))]
    let hwaddr = EthernetAddress(NET_HWADDR);
    #[cfg(feature = "generate-hwaddr")]
    let hwaddr = {
        let uid = stm32f4xx_hal::signature::Uid::get();
        EthernetAddress(hash2hwaddr::generate_hwaddr(uid))
    };
    info!("Net hwaddr: {}", hwaddr);

    net::run(dp.ETHERNET_MAC, dp.ETHERNET_DMA, hwaddr, |iface| {
        Server::<Session>::run(iface, |server| {
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
                                        if let Some(adc_data) = channels.channel_state(channel).adc_data {
                                            let dac_feedback = channels.read_dac_feedback(channel);
                                            let dac_i = dac_feedback / Ohms(5.0);

                                            let itec = channels.read_itec(channel);
                                            let tec_i = Amps((itec.0 - 1.5) / 8.0);

                                            let state = channels.channel_state(channel);
                                            let _ = writeln!(
                                                socket, "t={} raw{}=0x{:06X} dac_feedback={}/{} itec={} tec={}",
                                                state.adc_time, channel, adc_data,
                                                dac_feedback, dac_i,
                                                itec, tec_i,
                                            );
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
                                        let _ = writeln!(socket, "- i_set={}", state.dac_value);
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
                                                show_pwm_channel(socket.deref_mut(), "max_v", &channels.pwm.max_v0);
                                                show_pwm_channel(socket.deref_mut(), "max_i_pos", &channels.pwm.max_i_pos0);
                                                show_pwm_channel(socket.deref_mut(), "max_i_neg", &channels.pwm.max_i_neg0);
                                            }
                                            1 => {
                                                show_pwm_channel(socket.deref_mut(), "max_v", &channels.pwm.max_v1);
                                                show_pwm_channel(socket.deref_mut(), "max_i_pos", &channels.pwm.max_i_pos1);
                                                show_pwm_channel(socket.deref_mut(), "max_i_neg", &channels.pwm.max_i_neg1);
                                            }
                                            _ => unreachable!(),
                                        }
                                        let _ = writeln!(socket, "");
                                    }
                                }
                                Command::Show(ShowCommand::SteinhartHart) => {
                                    for channel in 0..CHANNELS {
                                        let state = channels.channel_state(channel);
                                        let _ = writeln!(
                                            socket, "channel {}: Steinhart-Hart equation parameters",
                                            channel,
                                        );
                                        let _ = writeln!(socket, "- t0={}", state.sh.t0);
                                        let _ = writeln!(socket, "- b={}", state.sh.b);
                                        let _ = writeln!(socket, "- r0={}", state.sh.r0);
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
                                    let _ = writeln!(socket, "channel {}: PID enabled to control PWM", channel
                                    );
                                }
                                Command::Pwm { channel, pin: PwmPin::ISet, duty } => {
                                    channels.channel_state(channel).pid_engaged = false;
                                    let voltage = Volts(duty);
                                    channels.set_dac(channel, voltage);
                                    let _ = writeln!(
                                        socket, "channel {}: PWM duty cycle manually set to {}",
                                        channel, voltage
                                    );
                                }
                                Command::Pwm { channel, pin, duty } => {
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
                                            set_pwm_channel(&mut channels.pwm.max_i_pos0, duty),
                                        (0, PwmPin::MaxINeg) =>
                                            set_pwm_channel(&mut channels.pwm.max_i_neg0, duty),
                                        (0, PwmPin::MaxV) =>
                                            set_pwm_channel(&mut channels.pwm.max_v0, duty),
                                        (1, PwmPin::MaxIPos) =>
                                            set_pwm_channel(&mut channels.pwm.max_i_pos1, duty),
                                        (1, PwmPin::MaxINeg) =>
                                            set_pwm_channel(&mut channels.pwm.max_i_neg1, duty),
                                        (1, PwmPin::MaxV) =>
                                            set_pwm_channel(&mut channels.pwm.max_v1, duty),
                                        _ =>
                                            unreachable!(),
                                    };
                                    let _ = writeln!(
                                        socket, "channel {}: PWM {} reconfigured to {}/{}",
                                        channel, pin.name(), duty, max
                                    );
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
                                        T0 => sh.t0 = value,
                                        B => sh.b = value,
                                        R0 => sh.r0 = value,
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
                            let _ = writeln!(
                                socket, "t={} raw{}=0x{:06X}",
                                state.adc_time, channel, state.adc_data.unwrap_or(0)
                            ).map(|_| {
                                session.mark_report_sent(channel);
                            });
                        }
                    }
                });

                // Update watchdog
                wd.feed();

                cortex_m::interrupt::free(|cs| {
                    if !net::is_pending(cs) {
                        // Wait for interrupts
                        // (Ethernet or SysTick)
                        wfi();
                    }
                });
            }
        });
    });

    unreachable!()
}
