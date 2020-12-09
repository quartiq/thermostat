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

## Debugging

Connect SWDIO/SWCLK/RST/GND to a programmer such as ST-Link v2.1. Run OpenOCD:
```shell
openocd -f interface/stlink-v2-1.cfg -f target/stm32f4x.cfg
```

You may need to power up the programmer before powering the device.
Leave OpenOCD running. Run the GNU debugger:
```shell
gdb target/thumbv7em-none-eabihf/release/thermostat

(gdb) source openocd.gdb
```

## Flashing
There are several options for performing device firmware upgrade (DFU) on Thermostat. The firmware is an .ELF file. 

### dfu-util
This can be done from Linux machine using only micro-USB connector. 
([Issue #10](https://git.m-labs.hk/M-Labs/thermostat/issues/10))
* Install the DFU USB tool (dfu-util).
* Convert firmware from ELF to BIN: ```arm-none-eabi-objcopy -O binary thermostat.elf thermostat.bin```
* Connect to the Micro USB connector to Thermostat below the RJ45.
* Add jumper to Thermostat v2.0 across 2-pin jumper adjacent to JTAG connector.
* Cycle board power to put it in DFU update mode
* Push firmware to flash: ```dfu-util -a 0 -s 0x08000000:leave -D thermostat.bin```
* Remove jumper
* Cycle power to leave DFU update mode

### st.com DfuSe tool
On a Windows machine install [st.com](https://st.com) DfuSe USB device firmware upgrade (DFU) software. [link](https://www.st.com/en/development-tools/stsw-stm32080.html).  
- add jumper to Thermostat v2.0 across 2-pin jumper adjacent to JTAG connector
- cycle board power to put it in DFU update mode
- connect micro-USB to PC
- use st.com software to upload firmware
- remove jumper
- cycle power to leave DFU update mode

### openocd
```shell
openocd -f interface/stlink-v2-1.cfg -f target/stm32f4x.cfg -c "program target/thumbv7em-none-eabihf/release/thermostat verify reset;exit"
```

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

| Syntax                           | Function                                            |
| ---                              | ---                                                 |
| `report`                         | Show current input                                  |
| `report mode`                    | Show current report mode                            |
| `report mode <off/on>`           | Set report mode                                     |
| `pwm`                            | Show current PWM settings                           |
| `pwm <0/1> max_i_pos <amp>`      | Set PWM duty cycle for **max_i_pos** to *ampere*    |
| `pwm <0/1> max_i_neg <amp>`      | Set PWM duty cycle for **max_i_neg** to *ampere*    |
| `pwm <0/1> max_v <volts>`        | Set PWM duty cycle for **max_v** to *volt*          |
| `pwm <0/1> i_set <amp>`          | Disengage PID, set **i_set** DAC to *ampere*        |
| `pwm <0/1> pid`                  | Set PWM to be controlled by PID                     |
| `center <0/1> <volts>`           | Set the MAX1968 0A-centerpoint to *volts*           |
| `center <0/1> vref`              | Set the MAX1968 0A-centerpoint to measure from VREF |
| `pid`                            | Show PID configuration                              |
| `pid <0/1> target <deg_celsius>` | Set the PID controller target temperature           |
| `pid <0/1> kp <value>`           | Set proportional gain                               |
| `pid <0/1> ki <value>`           | Set integral gain (unit: 10 Hz)                     |
| `pid <0/1> kd <value>`           | Set differential gain (unit: 0.1 seconds)           |
| `pid <0/1> output_min <amp>`     | Set mininum output                                  |
| `pid <0/1> output_max <amp>`     | Set maximum output                                  |
| `pid <0/1> integral_min <value>` | Set integral lower bound                            |
| `pid <0/1> integral_max <value>` | Set integral upper bound                            |
| `s-h`                            | Show Steinhart-Hart equation parameters             |
| `s-h <0/1> <t0/b/r0> <value>`    | Set Steinhart-Hart parameter for a channel          |
| `postfilter`                     | Show postfilter settings                            |
| `postfilter <0/1> off`           | Disable postfilter                                  |
| `postfilter <0/1> rate <rate>`   | Set postfilter output data rate                     |
| `load`                           | Restore configuration from EEPROM                   |
| `save`                           | Save configuration to EEPROM                        |
| `reset`                          | Reset the device                                    |
| `ipv4 <X.X.X.X>`                 | Configure IPv4 address                              |


## USB

The firmware includes experimental support for acting as a USB-Serial
peripheral. Debug logging will be sent there by default (unless build
with logging via semihosting.)

**Caveat:** This logging does not flush its output. Doing so would
hang indefinitely if the output is not read by the USB host. Therefore
output will be truncated when USB buffers are full.


## Temperature measurement

Connect the thermistor with the SENS pins of the
device. Temperature-depending resistance is measured by the AD7172
ADC. To prepare conversion to a temperature, set the Beta parameters
for the Steinhart-Hart equation.

Set the base temperature in degrees celsius for the channel 0 thermistor:
```
s-h 0 t0 20
```

Set the resistance in Ohms measured at the base temperature t0:
```
s-h 0 r0 10000
```

Set the Beta parameter:
```
s-h 0 b 3800
```


## Thermo-Electric Cooling (TEC)

- Connect Peltier device 0 to TEC0- and TEC0+.
- Connect Peliter device 1 to TEC1- and TEC1+.
- The GND pin is for shielding not for sinking Peltier currents.

### Limits

Each of the MAX1968 TEC driver has analog/PWM inputs for setting
output limits.

Use the `pwm` command to see current settings and maximum values.

| Limit       | Unit    | Description                   |
| ---         | :---:   | ---                           |
| `max_v`     | Volts   | Maximum voltage               |
| `max_i_pos` | Amperes | Maximum positive current      |
| `max_i_neg` | Amperes | Maximum negative current      |
| `i_set`     | Amperes | (Not a limit; Open-loop mode) |

Example: set the maximum voltage of channel 0 to 1.5 V.
```
pwm 0 max_v 1.5
```

### Open-loop mode

To manually control TEC output current, omit the limit parameter of
the `pwm` command. Doing so will disengage the PID control for that
channel.

Example: set output current of channel 0 to 0 A.
```
pwm 0 i_set 0
```

## PID-stabilized temperature control

Set the target temperature of channel 0 to 20 degrees celsius:
```
pid 0 target 20
```

Enter closed-loop mode by switching control of the TEC output current
of channel 0 to the PID algorithm:
```
pwm 0 pid
```

## LED indicators

| Name | Color | Meaning                        |
| ---  | :---: | ---                            |
| L1   | Red   | Firmware initializing          |
| L3   | Green | Closed-loop mode (PID engaged) |
| L4   | Green | Firmware busy                  |

## Reports

Use the bare `report` command to obtain a single report. Enable
continuous reporting with `report mode on`. Reports are JSON objects
with the following keys.

| Key            | Unit            | Description                                          |
| ---            | :---:           | ---                                                  |
| `channel`      | Integer         | Channel `0`, or `1`                                  |
| `time`         | Milliseconds    | Temperature measurement time                         |
| `adc`          | Volts           | AD7172 input                                         |
| `sens`         | Ohms            | Thermistor resistance derived from `adc`             |
| `temperature`  | Degrees Celsius | Steinhart-Hart conversion result derived from `sens` |
| `pid_engaged`  | Boolean         | `true` if in closed-loop mode                        |
| `i_set`        | Amperes         | TEC output current                                   |
| `vref`         | Volts           | MAX1968 VREF (1.5 V)                                 |
| `dac_value`    | Volts           | AD5680 output derived from `i_set`                   |
| `dac_feedback` | Volts           | ADC measurement of the AD5680 output                 |
| `i_tec`        | Volts           | MAX1968 TEC current monitor                          |
| `tec_i`        | Amperes         | TEC output current feedback derived from `i_tec`     |
| `tec_u_meas`   | Volts           | Measurement of the voltage across the TEC            |
| `pid_output`   | Amperes         | PID control output                                   |
