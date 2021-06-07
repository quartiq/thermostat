use core::fmt;
use core::num::ParseIntError;
use core::str::{from_utf8, Utf8Error};
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{is_a, tag, take_while1},
    character::{is_digit, complete::{char, one_of}},
    combinator::{complete, map, opt, value},
    sequence::preceded,
    multi::{fold_many0, fold_many1},
    error::ErrorKind,
};
use num_traits::{Num, ParseFloatError};
use serde::{Serialize, Deserialize};


#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Parser(ErrorKind),
    Incomplete,
    UnexpectedInput(u8),
    Utf8(Utf8Error),
    ParseInt(ParseIntError),
    // `num_traits::ParseFloatError` does not impl Clone
    ParseFloat,
}

impl<'t> From<nom::Err<(&'t [u8], ErrorKind)>> for Error {
    fn from(e: nom::Err<(&'t [u8], ErrorKind)>) -> Self {
        match e {
            nom::Err::Incomplete(_) =>
                Error::Incomplete,
            nom::Err::Error((_, e)) =>
                Error::Parser(e),
            nom::Err::Failure((_, e)) =>
                Error::Parser(e),
        }
    }
}

impl From<Utf8Error> for Error {
    fn from(e: Utf8Error) -> Self {
        Error::Utf8(e)
    }
}

impl From<ParseIntError> for Error {
    fn from(e: ParseIntError) -> Self {
        Error::ParseInt(e)
    }
}

impl From<ParseFloatError> for Error {
    fn from(_: ParseFloatError) -> Self {
        Error::ParseFloat
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Error::Incomplete =>
                "incomplete input".fmt(fmt),
            Error::UnexpectedInput(c) => {
                "unexpected input: ".fmt(fmt)?;
                c.fmt(fmt)
            }
            Error::Parser(e) => {
                "parser: ".fmt(fmt)?;
                (e as &dyn core::fmt::Debug).fmt(fmt)
            }
            Error::Utf8(e) => {
                "utf8: ".fmt(fmt)?;
                (e as &dyn core::fmt::Debug).fmt(fmt)
            }
            Error::ParseInt(e) => {
                "parsing int: ".fmt(fmt)?;
                (e as &dyn core::fmt::Debug).fmt(fmt)
            }
            Error::ParseFloat => {
                "parsing float".fmt(fmt)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ipv4Config {
    pub address: [u8; 4],
    pub mask_len: u8,
    pub gateway: Option<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShowCommand {
    Input,
    Reporting,
    Pwm,
    Pid,
    SteinhartHart,
    PostFilter,
    Ipv4,
    Iir,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PidParameter {
    Target,
    KP,
    KI,
    KD,
    OutputMin,
    OutputMax,
    IntegralMin,
    IntegralMax,
}

/// Steinhart-Hart equation parameter
#[derive(Debug, Clone, PartialEq)]
pub enum ShParameter {
    T0,
    B,
    R0,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PwmPin {
    ISet,
    MaxIPos,
    MaxINeg,
    MaxV,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CenterPoint {
    Vref,
    Override(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Quit,
    Load {
        channel: Option<usize>,
    },
    Save {
        channel: Option<usize>,
    },
    Reset,
    Ipv4(Ipv4Config),
    Show(ShowCommand),
    Reporting(bool),
    /// PWM parameter setting
    Pwm {
        channel: usize,
        pin: PwmPin,
        value: f64,
    },
    /// Enable PID control for `i_set`
    PwmPid {
        channel: usize,
    },
    CenterPoint {
        channel: usize,
        center: CenterPoint,
    },
    /// PID parameter setting
    Pid {
        channel: usize,
        parameter: PidParameter,
        value: f64,
    },
    SteinhartHart {
        channel: usize,
        parameter: ShParameter,
        value: f64,
    },
    PostFilter {
        channel: usize,
        rate: Option<f32>,
    },
    Dfu,
    Iir {
        channel: usize,
        values: [f64; 5],
    },
    Iirtarget{
        channel: usize,
        target: f64,
    },
    PwmIir{
        channel: usize,
    },
    PwmMatrix{
        channel: usize,
        iirout: usize
    },
    MatrixTemp{
        target: bool,
        nr: u32,
        temp: u32,
    },
    MatrixMatrix{
        target: bool,
        nr: u32,
        matrix: u32,
    },
    MatrixVal{
        target: bool,
        nr: u32,
        val: f64,
    },
}

fn end(input: &[u8]) -> IResult<&[u8], ()> {
    complete(
        fold_many0(
            one_of("\r\n\t "),
            (), |(), _| ()
        )
    )(input)
}

fn whitespace(input: &[u8]) -> IResult<&[u8], ()> {
    fold_many1(char(' '), (), |(), _| ())(input)
}

fn unsigned(input: &[u8]) -> IResult<&[u8], Result<u32, Error>> {
    take_while1(is_digit)(input)
        .map(|(input, digits)| {
            let result =
                from_utf8(digits)
                .map_err(|e| e.into())
                .and_then(|digits| u32::from_str_radix(digits, 10)
                     .map_err(|e| e.into())
                );
            (input, result)
        })
}

fn float(input: &[u8]) -> IResult<&[u8], Result<f64, Error>> {
    let (input, sign) = opt(is_a("-"))(input)?;
    let negative = sign.is_some();
    let (input, digits) = take_while1(|c| is_digit(c) || c == '.' as u8)(input)?;
    let result =
        from_utf8(digits)
        .map_err(|e| e.into())
        .and_then(|digits| f64::from_str_radix(digits, 10)
                  .map_err(|e| e.into())
        )
        .map(|result: f64| if negative { -result } else { result });
    Ok((input, result))
}

fn off_on(input: &[u8]) -> IResult<&[u8], bool> {
    alt((value(false, tag("off")),
         value(true, tag("on"))
    ))(input)
}

fn channel(input: &[u8]) -> IResult<&[u8], usize> {
    map(one_of("01"), |c| (c as usize) - ('0' as usize))(input)
}

fn report(input: &[u8]) -> IResult<&[u8], Command> {
    preceded(
        tag("report"),
        alt((
            preceded(
                whitespace,
                preceded(
                    tag("mode"),
                    alt((
                        preceded(
                            whitespace,
                            // `report mode <on | off>` - Switch repoting mode
                            map(off_on, Command::Reporting)
                        ),
                        // `report mode` - Show current reporting state
                        value(Command::Show(ShowCommand::Reporting), end)
                    ))
                )),
            // `report` - Report once
            value(Command::Show(ShowCommand::Input), end)
        ))
    )(input)
}

fn pwm_setup(input: &[u8]) -> IResult<&[u8], Result<(PwmPin, f64), Error>> {
    let result_with_pin = |pin: PwmPin|
        move |result: Result<f64, Error>|
        result.map(|value| (pin, value));

    alt((
        map(
            preceded(
                tag("i_set"),
                preceded(
                    whitespace,
                    float
                )
            ),
            result_with_pin(PwmPin::ISet)
        ),
        map(
            preceded(
                tag("max_i_pos"),
                preceded(
                    whitespace,
                    float
                )
            ),
            result_with_pin(PwmPin::MaxIPos)
        ),
        map(
            preceded(
                tag("max_i_neg"),
                preceded(
                    whitespace,
                    float
                )
            ),
            result_with_pin(PwmPin::MaxINeg)
        ),
        map(
            preceded(
                tag("max_v"),
                preceded(
                    whitespace,
                    float
                )
            ),
            result_with_pin(PwmPin::MaxV)
        ))
    )(input)
}

/// `pwm <0-1> pid` - Set PWM to be controlled by PID
fn pwm_pid(input: &[u8]) -> IResult<&[u8], ()> {
    value((), tag("pid"))(input)
}

/// `pwm <0-1> iir` - Set PWM to be controlled by IIR
fn pwm_iir(input: &[u8]) -> IResult<&[u8], ()> {
    value((), tag("iir"))(input)
}

/// `pwm <0-1> matrix` - Set PWM to be controlled by IIR
fn pwm_matrix(input: &[u8]) -> IResult<&[u8], ()> {
    value((), tag("matrix"))(input)

}

fn pwm(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("pwm")(input)?;
    alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, channel) = channel(input)?;
            let (input, _) = whitespace(input)?;
            let (input, result) = alt((
                |input| {
                    let (input, ()) = pwm_pid(input)?;
                    Ok((input, Ok(Command::PwmPid { channel })))
                },
                |input| {
                    let (input, ()) = pwm_iir(input)?;
                    Ok((input, Ok(Command::PwmIir { channel })))
                },
                |input| {
                    let (input, ()) = pwm_matrix(input)?;
                    let (input, _) = whitespace(input)?;
                    let (input, iirout_w) = unsigned(input)?;
                    let iirout = iirout_w.unwrap() as usize;
                    Ok((input, Ok(Command::PwmMatrix { channel, iirout })))
                },
                |input| {
                    let (input, config) = pwm_setup(input)?;
                    match config {
                        Ok((pin, value)) =>
                            Ok((input, Ok(Command::Pwm { channel, pin, value }))),
                        Err(e) =>
                            Ok((input, Err(e))),
                    }
                },
            ))(input)?;
            end(input)?;
            Ok((input, result))
        },
        value(Ok(Command::Show(ShowCommand::Pwm)), end)
    ))(input)
}

fn center_point(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("center")(input)?;
    let (input, _) = whitespace(input)?;
    let (input, channel) = channel(input)?;
    let (input, _) = whitespace(input)?;
    let (input, center) = alt((
        value(Ok(CenterPoint::Vref), tag("vref")),
        |input| {
            let (input, value) = float(input)?;
            Ok((input, value.map(|value| CenterPoint::Override(value as f32))))
        }
    ))(input)?;
    end(input)?;
    Ok((input, center.map(|center| Command::CenterPoint {
        channel,
        center,
    })))
}

/// `pid <0-1> <parameter> <value>`
fn pid_parameter(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, channel) = channel(input)?;
    let (input, _) = whitespace(input)?;
    let (input, parameter) =
        alt((value(PidParameter::Target, tag("target")),
             value(PidParameter::KP, tag("kp")),
             value(PidParameter::KI, tag("ki")),
             value(PidParameter::KD, tag("kd")),
             value(PidParameter::OutputMin, tag("output_min")),
             value(PidParameter::OutputMax, tag("output_max")),
             value(PidParameter::IntegralMin, tag("integral_min")),
             value(PidParameter::IntegralMax, tag("integral_max"))
        ))(input)?;
    let (input, _) = whitespace(input)?;
    let (input, value) = float(input)?;
    let result = value
        .map(|value| Command::Pid { channel, parameter, value });
    Ok((input, result))
}

/// `pid` | `pid <pid_parameter>`
fn pid(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("pid")(input)?;
    alt((
        preceded(
            whitespace,
            pid_parameter
        ),
        value(Ok(Command::Show(ShowCommand::Pid)), end)
    ))(input)
}

/// `s-h <0-1> <parameter> <value>`
fn steinhart_hart_parameter(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, channel) = channel(input)?;
    let (input, _) = whitespace(input)?;
    let (input, parameter) =
        alt((value(ShParameter::T0, tag("t0")),
             value(ShParameter::B, tag("b")),
             value(ShParameter::R0, tag("r0"))
        ))(input)?;
    let (input, _) = whitespace(input)?;
    let (input, value) = float(input)?;
    let result = value
        .map(|value| Command::SteinhartHart { channel, parameter, value });
    Ok((input, result))
}

/// `s-h` | `s-h <steinhart_hart_parameter>`
fn steinhart_hart(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("s-h")(input)?;
    alt((
        preceded(
            whitespace,
            steinhart_hart_parameter
        ),
        value(Ok(Command::Show(ShowCommand::SteinhartHart)), end)
    ))(input)
}

fn postfilter(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("postfilter")(input)?;
    alt((
        preceded(
            whitespace,
            |input| {
                let (input, channel) = channel(input)?;
                let (input, _) = whitespace(input)?;
                alt((
                    value(Ok(Command::PostFilter {
                        channel,
                        rate: None,
                    }), tag("off")),
                    move |input| {
                        let (input, _) = tag("rate")(input)?;
                        let (input, _) = whitespace(input)?;
                        let (input, rate) = float(input)?;
                        let result = rate
                            .map(|rate| Command::PostFilter {
                                channel,
                                rate: Some(rate as f32),
                            });
                        Ok((input, result))
                    }
                ))(input)
            }
        ),
        value(Ok(Command::Show(ShowCommand::PostFilter)), end)
    ))(input)
}

fn load(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("load")(input)?;
    let (input, channel) = alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, channel) = channel(input)?;
            let (input, _) = end(input)?;
            Ok((input, Some(channel)))
        },
        value(None, end)
    ))(input)?;

    let result = Ok(Command::Load { channel });
    Ok((input, result))
}

fn save(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("save")(input)?;
    let (input, channel) = alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, channel) = channel(input)?;
            let (input, _) = end(input)?;
            Ok((input, Some(channel)))
        },
        value(None, end)
    ))(input)?;

    let result = Ok(Command::Save { channel });
    Ok((input, result))
}

fn ipv4_addr(input: &[u8]) -> IResult<&[u8], Result<[u8; 4], Error>> {
    let (input, a) = unsigned(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, b) = unsigned(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, c) = unsigned(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, d) = unsigned(input)?;
    let address = move || Ok([a? as u8, b? as u8, c? as u8, d? as u8]);
    Ok((input, address()))
}

fn ipv4(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("ipv4")(input)?;
    alt((
        |input| {
            let (input, _) = whitespace(input)?;
            let (input, address) = ipv4_addr(input)?;
            let (input, _) = tag("/")(input)?;
            let (input, mask_len) = unsigned(input)?;
            let (input, gateway) = alt((
                |input| {
                    let (input, _) = whitespace(input)?;
                    let (input, gateway) = ipv4_addr(input)?;
                    Ok((input, gateway.map(Some)))
                },
                value(Ok(None), end),
            ))(input)?;

            let result = move || {
                Ok(Command::Ipv4(Ipv4Config {
                    address: address?,
                    mask_len: mask_len? as u8,
                    gateway: gateway?,
                }))
            };
            Ok((input, result()))
        },
        value(Ok(Command::Show(ShowCommand::Ipv4)), end),
    ))(input)
}

/// `iir` | `iir <iir_parameter>`
fn iir(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("iir")(input)?;
    alt((
        preceded(
            whitespace,
            iir_parameter
        ),
        value(Ok(Command::Show(ShowCommand::Iir)), end)
    ))(input)
}

/// `iir` | `iir <iir_parameter>`
fn iirtarget(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("target_iir")(input)?;
    let (input, _) = whitespace(input)?;
    let (input, channel) = channel(input)?;
    let (input, _) = whitespace(input)?;
    let (input, tg) = float(input)?;
    let target = tg.unwrap();
    let result = Ok(Command::Iirtarget { channel, target });
    Ok((input, result))

}



/// `pid <0-1> <values>`
fn iir_parameter(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, channel_w) = unsigned(input)?;
    let channel = channel_w.unwrap() as usize;
    let (input, _) = whitespace(input)?;
    let (input, val0) = float(input)?;
    let (input, _) = whitespace(input)?;
    let (input, val1) = float(input)?;
    let (input, _) = whitespace(input)?;
    let (input, val2) = float(input)?;
    let (input, _) = whitespace(input)?;
    let (input, val3) = float(input)?;
    let (input, _) = whitespace(input)?;
    let (input, val4) = float(input)?;

    // let values = [val0, val1, val2, val3, val4].iter().map(|val| val.unwrap()).collect();
    let values = [val0.unwrap(),val1.unwrap(),val2.unwrap(),val3.unwrap(),val4.unwrap()];

    let result = Ok(Command::Iir { channel, values });
    Ok((input, result))
}


fn matrix(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    let (input, _) = tag("matrix")(input)?;
    let (input, _) = whitespace(input)?;

    let (input, target) =
        alt((value(true, tag("target")),
             value(false, tag("in"))
        ))(input)?;

    let (input, _) = whitespace(input)?;
    let (input, in_w) = unsigned(input)?;
    let nr = in_w.unwrap();
    let (input, _) = whitespace(input)?;
    let (input, result) = alt((
        |input| {
            let (input, _) = tag("temp")(input)?;
            let (input, _) = whitespace(input)?;
            let (input, temp_w) = unsigned(input)?;
            let temp = temp_w.unwrap();
            Ok((input, Ok(Command::MatrixTemp { target, nr, temp })))
        },
        |input| {
            let (input, _) = tag("matrix")(input)?;
            let (input, _) = whitespace(input)?;
            let (input, matrix_w) = unsigned(input)?;
            let matrix = matrix_w.unwrap();
            Ok((input, Ok(Command::MatrixMatrix { target, nr, matrix })))
        },
        |input| {
            let (input, _) = tag("val")(input)?;
            let (input, _) = whitespace(input)?;
            let (input, val_w) = float(input)?;
            let val = val_w.unwrap();
            Ok((input, Ok(Command::MatrixVal { target, nr, val })))
        }
    ))(input)?;

    // let (input, result) = alt((
    //     |input| {
    //         let (input, _) = tag("in")(input)?;
    //         let (input, in_w) = unsigned(input)?;
    //         let nr = in_w.unwrap();
    //         let (input, _) = whitespace(input)?;
    //         let (input, result) = alt((
    //             |input| {
    //                 let (input, _) = tag("temp")(input)?;
    //                 let (input, temp_w) = unsigned(input)?;
    //                 let temp = temp_w.unwrap();
    //                 let target = false;
    //                 Ok((input, Ok(Command::MatrixTemp { target, nr, temp })))
    //             },
    //             |input| {
    //                 let (input, _) = tag("matrix")(input)?;
    //                 let (input, matrix_w) = unsigned(input)?;
    //                 let matrix = matrix_w.unwrap();
    //                 let target = false;
    //                 Ok((input, Ok(Command::MatrixMatrix { target, nr, matrix })))
    //             },
    //             |input| {
    //                 let (input, _) = tag("val")(input)?;
    //                 let (input, val_w) = float(input)?;
    //                 let val = val_w.unwrap();
    //                 let target = false;
    //                 Ok((input, Ok(Command::MatrixVal { target, nr, val })))
    //             }
    //         ))(input)?;
    //     },
    //     |input| {
    //         let (input, _) = tag("target")(input)?;
    //         let (input, in_w) = unsigned(input)?;
    //         let nr = in_w.unwrap();
    //         let (input, _) = whitespace(input)?;
    //         let (input, result) = alt((
    //             |input| {
    //                 let (input, _) = tag("temp")(input)?;
    //                 let (input, temp_w) = unsigned(input)?;
    //                 let temp = temp_w.unwrap();
    //                 let target = true;
    //                 Ok((input, Ok(Command::MatrixTemp { target, nr, temp })))
    //             },
    //             |input| {
    //                 let (input, _) = tag("matrix")(input)?;
    //                 let (input, matrix_w) = unsigned(input)?;
    //                 let matrix = matrix_w.unwrap();
    //                 let target = true;
    //                 Ok((input, Ok(Command::MatrixMatrix { target, nr, matrix })))
    //             },
    //             |input| {
    //                 let (input, _) = tag("val")(input)?;
    //                 let (input, val_w) = float(input)?;
    //                 let val = val_w.unwrap();
    //                 let target = true;
    //                 Ok((input, Ok(Command::MatrixVal { target, nr, val })))
    //             }
    //         ))(input)?;
    //     },
    // ))(input)?;
    end(input)?;
    Ok((input, result))
}



fn command(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    alt((value(Ok(Command::Quit), tag("quit")),
         load,
         save,
         value(Ok(Command::Reset), tag("reset")),
         ipv4,
         map(report, Ok),
         pwm,
         center_point,
         pid,
         steinhart_hart,
         postfilter,
         value(Ok(Command::Dfu), tag("dfu")),
         iir,
         iirtarget,
         matrix,
    ))(input)
}

impl Command {
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        match command(input) {
            Ok((input_remain, result)) if input_remain.len() == 0 =>
                result,
            Ok((input_remain, _)) =>
                Err(Error::UnexpectedInput(input_remain[0])),
            Err(e) =>
                Err(e.into()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_quit() {
        let command = Command::parse(b"quit");
        assert_eq!(command, Ok(Command::Quit));
    }

    #[test]
    fn parse_load() {
        let command = Command::parse(b"load");
        assert_eq!(command, Ok(Command::Load { channel: None }));
    }

    #[test]
    fn parse_load_channel() {
        let command = Command::parse(b"load 0");
        assert_eq!(command, Ok(Command::Load { channel: Some(0) }));
    }

    #[test]
    fn parse_save() {
        let command = Command::parse(b"save");
        assert_eq!(command, Ok(Command::Save { channel: None }));
    }

    #[test]
    fn parse_save_channel() {
        let command = Command::parse(b"save 0");
        assert_eq!(command, Ok(Command::Save { channel: Some(0) }));
    }

    #[test]
    fn parse_show_ipv4() {
        let command = Command::parse(b"ipv4");
        assert_eq!(command, Ok(Command::Show(ShowCommand::Ipv4)));
    }

    #[test]
    fn parse_ipv4() {
        let command = Command::parse(b"ipv4 192.168.1.26/24");
        assert_eq!(command, Ok(Command::Ipv4(Ipv4Config {
            address: [192, 168, 1, 26],
            mask_len: 24,
            gateway: None,
        })));
    }

    #[test]
    fn parse_ipv4_and_gateway() {
        let command = Command::parse(b"ipv4 10.42.0.126/8 10.1.0.1");
        assert_eq!(command, Ok(Command::Ipv4(Ipv4Config {
            address: [10, 42, 0, 126],
            mask_len: 8,
            gateway: Some([10, 1, 0, 1]),
        })));
    }

    #[test]
    fn parse_report() {
        let command = Command::parse(b"report");
        assert_eq!(command, Ok(Command::Show(ShowCommand::Input)));
    }

    #[test]
    fn parse_report_mode() {
        let command = Command::parse(b"report mode");
        assert_eq!(command, Ok(Command::Show(ShowCommand::Reporting)));
    }

    #[test]
    fn parse_report_mode_on() {
        let command = Command::parse(b"report mode on");
        assert_eq!(command, Ok(Command::Reporting(true)));
    }

    #[test]
    fn parse_report_mode_off() {
        let command = Command::parse(b"report mode off");
        assert_eq!(command, Ok(Command::Reporting(false)));
    }

    #[test]
    fn parse_pwm_i_set() {
        let command = Command::parse(b"pwm 1 i_set 16383");
        assert_eq!(command, Ok(Command::Pwm {
            channel: 1,
            pin: PwmPin::ISet,
            value: 16383.0,
        }));
    }

    #[test]
    fn parse_pwm_pid() {
        let command = Command::parse(b"pwm 0 pid");
        assert_eq!(command, Ok(Command::PwmPid {
            channel: 0,
        }));
    }

    #[test]
    fn parse_pwm_max_i_pos() {
        let command = Command::parse(b"pwm 0 max_i_pos 7");
        assert_eq!(command, Ok(Command::Pwm {
            channel: 0,
            pin: PwmPin::MaxIPos,
            value: 7.0,
        }));
    }

    #[test]
    fn parse_pwm_max_i_neg() {
        let command = Command::parse(b"pwm 0 max_i_neg 128");
        assert_eq!(command, Ok(Command::Pwm {
            channel: 0,
            pin: PwmPin::MaxINeg,
            value: 128.0,
        }));
    }

    #[test]
    fn parse_pwm_max_v() {
        let command = Command::parse(b"pwm 0 max_v 32768");
        assert_eq!(command, Ok(Command::Pwm {
            channel: 0,
            pin: PwmPin::MaxV,
            value: 32768.0,
        }));
    }

    #[test]
    fn parse_pid() {
        let command = Command::parse(b"pid");
        assert_eq!(command, Ok(Command::Show(ShowCommand::Pid)));
    }

    #[test]
    fn parse_pid_target() {
        let command = Command::parse(b"pid 0 target 36.5");
        assert_eq!(command, Ok(Command::Pid {
            channel: 0,
            parameter: PidParameter::Target,
            value: 36.5,
        }));
    }

    #[test]
    fn parse_pid_integral_max() {
        let command = Command::parse(b"pid 1 integral_max 2000");
        assert_eq!(command, Ok(Command::Pid {
            channel: 1,
            parameter: PidParameter::IntegralMax,
            value: 2000.0,
        }));
    }

    #[test]
    fn parse_steinhart_hart() {
        let command = Command::parse(b"s-h");
        assert_eq!(command, Ok(Command::Show(ShowCommand::SteinhartHart)));
    }

    #[test]
    fn parse_steinhart_hart_set() {
        let command = Command::parse(b"s-h 1 t0 23.05");
        assert_eq!(command, Ok(Command::SteinhartHart {
            channel: 1,
            parameter: ShParameter::T0,
            value: 23.05,
        }));
    }

    #[test]
    fn parse_postfilter() {
        let command = Command::parse(b"postfilter");
        assert_eq!(command, Ok(Command::Show(ShowCommand::PostFilter)));
    }

    #[test]
    fn parse_postfilter_off() {
        let command = Command::parse(b"postfilter 1 off");
        assert_eq!(command, Ok(Command::PostFilter {
            channel: 1,
            rate: None,
        }));
    }

    #[test]
    fn parse_postfilter_rate() {
        let command = Command::parse(b"postfilter 0 rate 21");
        assert_eq!(command, Ok(Command::PostFilter {
            channel: 0,
            rate: Some(21.0),
        }));
    }

    #[test]
    fn parse_center_point() {
        let command = Command::parse(b"center 0 1.5");
        assert_eq!(command, Ok(Command::CenterPoint {
            channel: 0,
            center: CenterPoint::Override(1.5),
        }));
    }

    #[test]
    fn parse_center_point_vref() {
        let command = Command::parse(b"center 1 vref");
        assert_eq!(command, Ok(Command::CenterPoint {
            channel: 1,
            center: CenterPoint::Vref,
        }));
    }
}
