use smoltcp::socket::TcpSocket;
use log::{error, warn};
use core::fmt::Write;
use heapless::{consts::U1024, Vec};
use super::{
    net,
    command_parser::{
        Ipv4Config, 
        Command, 
        ShowCommand, 
        CenterPoint, 
        PidParameter, 
        PwmPin, 
        ShParameter
    },
    leds::Leds,
    ad7172,
    CHANNEL_CONFIG_KEY,
    channels::{
        Channels, 
        CHANNELS
    },
    config::ChannelConfig,
    dfu,
    flash_store::FlashStore,
    session::Session,
    FanCtrl,
    hw_rev::HWRev,
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

#[derive(Debug, Clone, PartialEq)]
pub enum Handler {
    Handled,
    CloseSocket,
    NewIPV4(Ipv4Config),
    Reset,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    ReportError,
    PostFilterRateError,
    FlashError
}

pub type JsonBuffer = Vec<u8, U1024>;

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

impl Handler {

    fn reporting(socket: &mut TcpSocket) -> Result<Handler, Error> {
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn show_report_mode(socket: &mut TcpSocket, session: &Session) -> Result<Handler, Error> {
        let _ = writeln!(socket, "{{ \"report\": {:?} }}", session.reporting());
        Ok(Handler::Handled)
    }

    fn show_report(socket: &mut TcpSocket, channels: &mut Channels) -> Result<Handler, Error> {
        match channels.reports_json() {
            Ok(buf) => {
                send_line(socket, &buf[..]);
            }
            Err(e) => {
                error!("unable to serialize report: {:?}", e);
                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                return Err(Error::ReportError);
            }
        }
        Ok(Handler::Handled)
    }

    fn show_pid(socket: &mut TcpSocket, channels: &mut Channels) -> Result<Handler, Error> {
        match channels.pid_summaries_json() {
            Ok(buf) => {
                send_line(socket, &buf);
            }
            Err(e) => {
                error!("unable to serialize pid summary: {:?}", e);
                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                return Err(Error::ReportError);
            }
        }
        Ok(Handler::Handled)
    }

    fn show_pwm(socket: &mut TcpSocket, channels: &mut Channels) -> Result<Handler, Error> {
        match channels.pwm_summaries_json() {
            Ok(buf) => {
                send_line(socket, &buf);
            }
            Err(e) => {
                error!("unable to serialize pwm summary: {:?}", e);
                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                return Err(Error::ReportError);
            }
        }
        Ok(Handler::Handled)
    }

    fn show_steinhart_hart(socket: &mut TcpSocket, channels: &mut Channels) -> Result<Handler, Error> {
        match channels.steinhart_hart_summaries_json() {
            Ok(buf) => {
                send_line(socket, &buf);
            }
            Err(e) => {
                error!("unable to serialize steinhart-hart summaries: {:?}", e);
                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                return Err(Error::ReportError);
            }
        }
        Ok(Handler::Handled)
    }

    fn show_post_filter (socket: &mut TcpSocket, channels: &mut Channels) -> Result<Handler, Error> {
        match channels.postfilter_summaries_json() {
            Ok(buf) => {
                send_line(socket, &buf);
            }
            Err(e) => {
                error!("unable to serialize postfilter summary: {:?}", e);
                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                return Err(Error::ReportError);
            }
        }
        Ok(Handler::Handled)
    }

    fn show_ipv4 (socket: &mut TcpSocket, ipv4_config: &mut Ipv4Config) -> Result<Handler, Error> {
        let (cidr, gateway) = net::split_ipv4_config(ipv4_config.clone());
        let _ = write!(socket, "{{\"addr\":\"{}\"", cidr);
        gateway.map(|gateway| write!(socket, ",\"gateway\":\"{}\"", gateway));
        let _ = writeln!(socket, "}}");
        Ok(Handler::Handled)
    }

    fn engage_pid (socket: &mut TcpSocket, channels: &mut Channels, leds: &mut Leds, channel: usize) -> Result<Handler, Error> {
        channels.channel_state(channel).pid_engaged = true;
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn set_pwm (socket: &mut TcpSocket, channels: &mut Channels, leds: &mut Leds, channel: usize, pin: PwmPin, value: f64) -> Result<Handler, Error> {
        match pin {
            PwmPin::ISet => {
                channels.channel_state(channel).pid_engaged = false;
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
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn set_center_point(socket: &mut TcpSocket, channels: &mut Channels, channel: usize, center: CenterPoint) -> Result<Handler, Error> {
        let i_tec = channels.get_i(channel);
        let state = channels.channel_state(channel);
        state.center = center;
        if !state.pid_engaged {
            channels.set_i(channel, i_tec);
        }
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn set_pid (socket: &mut TcpSocket, channels: &mut Channels, channel: usize, parameter: PidParameter, value: f64) -> Result<Handler, Error> {
        let pid = &mut channels.channel_state(channel).pid;
        use super::command_parser::PidParameter::*;
        match parameter {
            Target =>
                pid.target = value,
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
        }
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn set_steinhart_hart (socket: &mut TcpSocket, channels: &mut Channels, channel: usize, parameter: ShParameter, value: f64) -> Result<Handler, Error> {
        let sh = &mut channels.channel_state(channel).sh;
        use super::command_parser::ShParameter::*;
        match parameter {
            T0 => sh.t0 = ThermodynamicTemperature::new::<degree_celsius>(value),
            B => sh.b = value,
            R0 => sh.r0 = ElectricalResistance::new::<ohm>(value),
        }
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn reset_post_filter (socket: &mut TcpSocket, channels: &mut Channels, channel: usize) -> Result<Handler, Error> {
        channels.adc.set_postfilter(channel as u8, None).unwrap();
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn set_post_filter (socket: &mut TcpSocket, channels: &mut Channels, channel: usize, rate: f32) -> Result<Handler, Error> {
        let filter = ad7172::PostFilter::closest(rate);
        match filter {
            Some(filter) => {
                channels.adc.set_postfilter(channel as u8, Some(filter)).unwrap();
                send_line(socket, b"{}");
            }
            None => {
                error!("unable to choose postfilter for rate {:.3}", rate);
                send_line(socket, b"{{\"error\": \"unable to choose postfilter rate\"}}");
                return Err(Error::PostFilterRateError);
            }
        }
        Ok(Handler::Handled)
    }

    fn load_channel (socket: &mut TcpSocket, channels: &mut Channels, store: &mut FlashStore, channel: Option<usize>) -> Result<Handler, Error> {
        for c in 0..CHANNELS {
            if channel.is_none() || channel == Some(c) {
                match store.read_value::<ChannelConfig>(CHANNEL_CONFIG_KEY[c]) {
                    Ok(Some(config)) => {
                        config.apply(channels, c);
                        send_line(socket, b"{}");
                    }
                    Ok(None) => {
                        error!("flash config not found");
                        send_line(socket, b"{{\"error\": \"flash config not found\"}}");
                    }
                    Err(e) => {
                        error!("unable to load config from flash: {:?}", e);
                        let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                        return Err(Error::FlashError);
                    }
                }
            }
        }
        Ok(Handler::Handled)
    }

    fn save_channel (socket: &mut TcpSocket, channels: &mut Channels, channel: Option<usize>, store: &mut FlashStore) -> Result<Handler, Error> {
        for c in 0..CHANNELS {
            let mut store_value_buf = [0u8; 256];
            if channel.is_none() || channel == Some(c) {
                let config = ChannelConfig::new(channels, c);
                match store.write_value(CHANNEL_CONFIG_KEY[c], &config, &mut store_value_buf) {
                    Ok(()) => {
                        send_line(socket, b"{}");
                    }
                    Err(e) => {
                        error!("unable to save channel {} config to flash: {:?}", c, e);
                        let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                        return Err(Error::FlashError);
                    }
                }
            }
        }
        Ok(Handler::Handled)
    }

    fn set_ipv4 (socket: &mut TcpSocket, store: &mut FlashStore, config: Ipv4Config) -> Result<Handler, Error> {
        let _ = store
            .write_value("ipv4", &config, [0; 16])
            .map_err(|e| error!("unable to save ipv4 config to flash: {:?}", e));
        let new_ipv4_config = Some(config);
        send_line(socket, b"{}");
        Ok(Handler::NewIPV4(new_ipv4_config.unwrap()))
    }

    fn reset (channels: &mut Channels) -> Result<Handler, Error> {
        for i in 0..CHANNELS {
            channels.power_down(i);
        }
        // should_reset = true;
        Ok(Handler::Reset)
    }

    fn dfu (channels: &mut Channels) -> Result<Handler, Error> {
        for i in 0..CHANNELS {
            channels.power_down(i);
        }
        unsafe {
            dfu::set_dfu_trigger();
        }
        // should_reset = true;
        Ok(Handler::Reset)
    }

    fn set_fan(socket: &mut TcpSocket, fan_pwm: u32, fan_ctrl: &mut FanCtrl) -> Result<Handler, Error> {
        if !fan_ctrl.fan_available() {
            send_line(socket, b"{ \"warning\": \"this thermostat doesn't have fan!\" }");
            return Ok(Handler::Handled);
        }
        fan_ctrl.set_auto_mode(false);
        fan_ctrl.set_pwm(fan_pwm);
        if fan_ctrl.fan_pwm_recommended() {
            send_line(socket, b"{}");
        } else {
            send_line(socket, b"{ \"warning\": \"this fan doesn't have full PWM support. Use it at your own risk!\" }");
        }
        Ok(Handler::Handled)
    }

    fn show_fan(socket: &mut TcpSocket, fan_ctrl: &mut FanCtrl) -> Result<Handler, Error> {
        match fan_ctrl.summary() {
            Ok(buf) => {
                send_line(socket, &buf);
                Ok(Handler::Handled)
            }
            Err(e) => {
                error!("unable to serialize fan summary: {:?}", e);
                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                Err(Error::ReportError)
            }
        }
    }

    fn fan_auto(socket: &mut TcpSocket, fan_ctrl: &mut FanCtrl) -> Result<Handler, Error> {
        if !fan_ctrl.fan_available() {
            send_line(socket, b"{ \"warning\": \"this thermostat doesn't have fan!\" }");
            return Ok(Handler::Handled);
        }
        fan_ctrl.set_auto_mode(true);
        if fan_ctrl.fan_pwm_recommended() {
            send_line(socket, b"{}");
        } else {
            send_line(socket, b"{ \"warning\": \"this fan doesn't have full PWM support. Use it at your own risk!\" }");
        }
        Ok(Handler::Handled)
    }

    fn fan_curve(socket: &mut TcpSocket, fan_ctrl: &mut FanCtrl, k_a: f32, k_b: f32, k_c: f32) -> Result<Handler, Error> {
        fan_ctrl.set_curve(k_a, k_b, k_c);
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn fan_defaults(socket: &mut TcpSocket, fan_ctrl: &mut FanCtrl) -> Result<Handler, Error> {
        fan_ctrl.restore_defaults();
        send_line(socket, b"{}");
        Ok(Handler::Handled)
    }

    fn show_hwrev(socket: &mut TcpSocket, hwrev: HWRev) -> Result<Handler, Error> {
        match hwrev.summary() {
            Ok(buf) => {
                send_line(socket, &buf);
                Ok(Handler::Handled)
            }
            Err(e) => {
                error!("unable to serialize HWRev summary: {:?}", e);
                let _ = writeln!(socket, "{{\"error\":\"{:?}\"}}", e);
                Err(Error::ReportError)
            }
        }
    }

    pub fn handle_command(command: Command, socket: &mut TcpSocket, channels: &mut Channels, session: &Session, leds: &mut Leds, store: &mut FlashStore, ipv4_config: &mut Ipv4Config, fan_ctrl: &mut FanCtrl, hwrev: HWRev) -> Result<Self, Error> {
        match command {
            Command::Quit => Ok(Handler::CloseSocket),
            Command::Reporting(_reporting) => Handler::reporting(socket),            
            Command::Show(ShowCommand::Reporting) => Handler::show_report_mode(socket, session),            
            Command::Show(ShowCommand::Input) => Handler::show_report(socket, channels),
            Command::Show(ShowCommand::Pid) => Handler::show_pid(socket, channels),
            Command::Show(ShowCommand::Pwm) => Handler::show_pwm(socket, channels),
            Command::Show(ShowCommand::SteinhartHart) => Handler::show_steinhart_hart(socket, channels),
            Command::Show(ShowCommand::PostFilter) => Handler::show_post_filter(socket, channels),
            Command::Show(ShowCommand::Ipv4) => Handler::show_ipv4(socket, ipv4_config),
            Command::PwmPid { channel } => Handler::engage_pid(socket, channels, leds, channel),
            Command::Pwm { channel, pin, value } => Handler::set_pwm(socket, channels, leds, channel, pin, value),
            Command::CenterPoint { channel, center } => Handler::set_center_point(socket, channels, channel, center),
            Command::Pid { channel, parameter, value } => Handler::set_pid(socket, channels, channel, parameter, value),
            Command::SteinhartHart { channel, parameter, value } => Handler::set_steinhart_hart(socket, channels, channel, parameter, value),
            Command::PostFilter { channel, rate: None } => Handler::reset_post_filter(socket, channels, channel),
            Command::PostFilter { channel, rate: Some(rate) } => Handler::set_post_filter(socket, channels, channel, rate),
            Command::Load { channel } => Handler::load_channel(socket, channels, store, channel),
            Command::Save { channel } => Handler::save_channel(socket, channels, channel, store),
            Command::Ipv4(config) => Handler::set_ipv4(socket, store, config),
            Command::Reset => Handler::reset(channels),
            Command::Dfu => Handler::dfu(channels),
            Command::FanSet {fan_pwm} => Handler::set_fan(socket, fan_pwm, fan_ctrl),
            Command::ShowFan => Handler::show_fan(socket, fan_ctrl),
            Command::FanAuto => Handler::fan_auto(socket, fan_ctrl),
            Command::FanCurve { k_a, k_b, k_c } => Handler::fan_curve(socket, fan_ctrl, k_a, k_b, k_c),
            Command::FanCurveDefaults => Handler::fan_defaults(socket, fan_ctrl),
            Command::ShowHWRev => Handler::show_hwrev(socket, hwrev),
        }
    }
}