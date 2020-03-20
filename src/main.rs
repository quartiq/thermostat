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
        digital::v2::OutputPin,
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
mod channel_state;
use channel_state::ChannelState;


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
    // Feature not used
    adc.set_sync_enable(false).unwrap();
    // Setup channels
    adc.setup_channel(0, ad7172::Input::Ain0, ad7172::Input::Ain1).unwrap();
    adc.setup_channel(1, ad7172::Input::Ain2, ad7172::Input::Ain3).unwrap();
    adc.calibrate_offset().unwrap();
    let mut dac0 = ad5680::Dac::new(pins.dac0_spi, pins.dac0_sync);
    dac0.set(0).unwrap();
    let mut dac1 = ad5680::Dac::new(pins.dac1_spi, pins.dac1_sync);
    dac1.set(0).unwrap();
    let mut pwm = pins.pwm;
    let mut shdn0 = pins.shdn0;
    let mut shdn1 = pins.shdn1;

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
                let instant = Instant::from_millis(i64::from(timer::now()));
                cortex_m::interrupt::free(net::clear_pending);
                server.poll(instant)
                    .unwrap_or_else(|e| {
                        warn!("poll: {:?}", e);
                    });

                let instant = Instant::from_millis(i64::from(timer::now()));
                // ADC input
                adc.data_ready().unwrap().map(|channel| {
                    let data = adc.read_data().unwrap();

                    let state = &mut channel_states[usize::from(channel)];
                    state.update_adc(instant, data);

                    if state.pid_enabled {
                        // Forward PID output to i_set DAC
                        match channel {
                            0 => {
                                dac0.set(state.dac_value).unwrap();
                                shdn0.set_high().unwrap();
                            }
                            1 => {
                                dac1.set(state.dac_value).unwrap();
                                shdn1.set_high().unwrap();
                            }
                            _ =>
                                unreachable!(),
                        }
                    }

                    server.for_each(|_, session| session.set_report_pending(channel.into()));
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
                                        show_pid_parameter!(output_min);
                                        show_pid_parameter!(output_max);
                                        show_pid_parameter!(integral_min);
                                        show_pid_parameter!(integral_max);
                                        if let Some(last_output) = pid.last_output {
                                            let _ = writeln!(socket, "- output={:.4}", last_output);
                                        }
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
                                        let _ = writeln!(socket, "- b={}", state.sh.b);
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
                                        0 => {
                                            dac0.set(duty).unwrap();
                                            shdn0.set_high().unwrap();
                                        }
                                        1 => {
                                            dac1.set(duty).unwrap();
                                            shdn1.set_high().unwrap();
                                        }
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
                                    let sh = &mut channel_states[channel].sh;
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
                            Err(_) =>
                                socket.close(),
                        }
                    } else if socket.can_send() && socket.send_capacity() - socket.send_queue() > 256 {
                        while let Some(channel) = session.is_report_pending() {
                            let state = &mut channel_states[usize::from(channel)];
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
