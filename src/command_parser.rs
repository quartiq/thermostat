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

#[derive(Debug, Clone, PartialEq)]
pub enum ShowCommand {
    Input,
    Reporting,
    Pwm,
    Pid,
    SteinhartHart,
    PostFilter,
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

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Quit,
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
        rate: f32,
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
        ),
        map(float, result_with_pin(PwmPin::ISet)
        ))
    )(input)
}

/// `pwm <0-1> pid` - Set PWM to be controlled by PID
fn pwm_pid(input: &[u8]) -> IResult<&[u8], ()> {
    value((), tag("pid"))(input)
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
                let (input, _) = tag("rate")(input)?;
                let (input, _) = whitespace(input)?;
                let (input, rate) = float(input)?;
                let result = rate
                    .map(|rate| Command::PostFilter {
                        channel,
                        rate: rate as f32,
                    });
                Ok((input, result))
            }
        ),
        value(Ok(Command::Show(ShowCommand::PostFilter)), end)
    ))(input)
}

fn command(input: &[u8]) -> IResult<&[u8], Result<Command, Error>> {
    alt((value(Ok(Command::Quit), tag("quit")),
         map(report, Ok),
         pwm,
         pid,
         steinhart_hart,
         postfilter,
    ))(input)
}

impl Command {
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        match command(input) {
            Ok((b"", result)) =>
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
    fn parse_pwm_manual() {
        let command = Command::parse(b"pwm 1 16383");
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
    fn parse_postfilter_rate() {
        let command = Command::parse(b"postfilter 0 rate 21");
        assert_eq!(command, Ok(Command::PostFilter {
            channel: 0,
            rate: 21.0,
        }));
    }
}
