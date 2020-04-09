# Firmware for the Sinara 8451 Thermostat

- [x] [Continuous Integration](https://nixbld.m-labs.hk/job/stm32/stm32/thermostat)
- [x] [Download latest firmware build](https://nixbld.m-labs.hk/job/stm32/stm32/thermostat/latest/download-by-type/file/binary-dist)


## Building

### Debian-based systems (tested on Ubuntu 19.10)

- install git, clone this repository
- install [rustup](https://rustup.rs/)

```shell
rustup toolchain install nightly
rustup update
rustup target add thumbv7em-none-eabihf --toolchain nightly
rustup default nightly
cargo build --release
```

The resulting ELF file will be located under `target/thumbv7em-none-eabihf/release/thermostat`


## Network

### Connecting

Ethernet, IP: 192.168.1.26/24

Use netcat to connect to port 23/tcp (telnet)
```sh
nc -vv 192.168.1.26 23
```

telnet clients send binary data after connect. Enter \n once to
invalidate the first line of input.


### Reading ADC input

Set report mode to `on` for a continuous stream of input data.

The scope of this setting is per TCP session.


### Commands

| Syntax                           | Function                                        |
| ---                              | ---                                             |
| `report`                         | Show current input                              |
| `report mode`                    | Show current report mode                        |
| `report mode <off/on>`           | Set report mode                                 |
| `pwm`                            | Show current PWM settings                       |
| `pwm <0/1> max_i_pos <width>`    | Set PWM duty cycle for **max_i_pos** to *width* |
| `pwm <0/1> max_i_neg <width>`    | Set PWM duty cycle for **max_i_neg** to *width* |
| `pwm <0/1> max_v <width>`        | Set PWM duty cycle for **max_v** to *width*     |
| `pwm <0/1> <width>`              | Disengage PID, set **i_set** DAC to *width*     |
| `pwm <0/1> pid`                  | Set PWM to be controlled by PID                 |
| `pid`                            | Show PID configuration                          |
| `pid <0/1> target <value>`       | Set the PID controller target                   |
| `pid <0/1> kp <value>`           | Set proportional gain                           |
| `pid <0/1> ki <value>`           | Set integral gain                               |
| `pid <0/1> kd <value>`           | Set differential gain                           |
| `pid <0/1> output_min <value>`   | Set mininum output                              |
| `pid <0/1> output_max <value>`   | Set maximum output                              |
| `pid <0/1> integral_min <value>` | Set integral lower bound                        |
| `pid <0/1> integral_max <value>` | Set integral upper bound                        |
| `s-h`                            | Show Steinhart-Hart equation parameters         |
| `s-h <0/1> <t/b/r0> <value>`     | Set Steinhart-Hart parameter for a channel      |
| `postfilter <0/1> rate <rate>`   | Set postfilter output data rate                 |
