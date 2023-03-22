# Firmware for the Sinara 8451 Thermostat

- [x] [Continuous Integration](https://nixbld.m-labs.hk/job/mcu/thermostat/thermostat)
- [x] Download latest firmware build: [ELF](https://nixbld.m-labs.hk/job/mcu/thermostat/thermostat/latest/download/1) [BIN](https://nixbld.m-labs.hk/job/mcu/thermostat/thermostat/latest/download/2)


## Building

### Reproducible build with Nix

Thermostat firmware is packaged using the [Nix](https://nixos.org) Flakes system. Install Nix 2.4+ and enable flakes by adding ``experimental-features = nix-command flakes`` to ``nix.conf`` (e.g. ``~/.config/nix/nix.conf``). 

Once you have Flakes enabled, you can use ``nix build`` to build the firmware.

### Development environment

Clone this repository and with Nix Flakes enabled, use the following commands:

```shell
nix develop
cargo build --release
```

The resulting ELF file will be located under `target/thumbv7em-none-eabihf/release/thermostat`.

Alternatively, you can install the Rust toolchain without Nix using rustup; see the Rust manifest file pulled in `flake.nix` to determine which Rust version to use.

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
There are several options for flashing Thermostat. DFU requires only a micro-USB connector, whereas OpenOCD needs a JTAG/SWD adapter.

### dfu-util on Linux
* Install the DFU USB tool (dfu-util).
* Convert firmware from ELF to BIN: `arm-none-eabi-objcopy -O binary thermostat thermostat.bin` (you can skip this step if using the BIN from Hydra)
* Connect to the Micro USB connector to Thermostat below the RJ45.
* Add jumper to Thermostat v2.0 across 2-pin jumper adjacent to JTAG connector.
* Cycle board power to put it in DFU update mode
* Push firmware to flash: `dfu-util -a 0 -s 0x08000000:leave -D thermostat.bin`
* Remove jumper
* Cycle power to leave DFU update mode

### st.com DfuSe tool on Windows
On a Windows machine install [st.com](https://st.com) DfuSe USB device firmware upgrade (DFU) software. [link](https://www.st.com/en/development-tools/stsw-stm32080.html).
- add jumper to Thermostat v2.0 across 2-pin jumper adjacent to JTAG connector
- cycle board power to put it in DFU update mode
- connect micro-USB to PC
- use st.com software to upload firmware
- remove jumper
- cycle power to leave DFU update mode

### OpenOCD
```shell
openocd -f interface/stlink-v2-1.cfg -f target/stm32f4x.cfg -c "program target/thumbv7em-none-eabihf/release/thermostat verify reset;exit"
```

## Network

### Connecting

Ethernet, IP: 192.168.1.26/24

Use netcat to connect to port 23/tcp (telnet)
```sh
rlwrap nc -vv 192.168.1.26 23
```

telnet clients send binary data after connect. Enter \n once to
invalidate the first line of input.


### Reading ADC input

Set report mode to `on` for a continuous stream of input data.

The scope of this setting is per TCP session.


### TCP commands

Send commands as simple text string terminated by `\n`. Responses are
formatted as line-delimited JSON.

| Syntax                           | Function                                                                      |
|----------------------------------|-------------------------------------------------------------------------------|
| `report`                         | Show current input                                                            |
| `report mode`                    | Show current report mode                                                      |
| `report mode <off/on>`           | Set report mode                                                               |
| `pwm`                            | Show current PWM settings                                                     |
| `pwm <0/1> max_i_pos <amp>`      | Set maximum positive output current                                           |
| `pwm <0/1> max_i_neg <amp>`      | Set maximum negative output current                                           |
| `pwm <0/1> max_v <volt>`         | Set maximum output voltage                                                    |
| `pwm <0/1> i_set <amp>`          | Disengage PID, set fixed output current                                       |
| `pwm <0/1> pid`                  | Let output current to be controlled by the PID                                |
| `center <0/1> <volt>`            | Set the MAX1968 0A-centerpoint to the specified fixed voltage                 |
| `center <0/1> vref`              | Set the MAX1968 0A-centerpoint to measure from VREF                           |
| `pid`                            | Show PID configuration                                                        |
| `pid <0/1> target <deg_celsius>` | Set the PID controller target temperature                                     |
| `pid <0/1> kp <value>`           | Set proportional gain                                                         |
| `pid <0/1> ki <value>`           | Set integral gain                                                             |
| `pid <0/1> kd <value>`           | Set differential gain                                                         |
| `pid <0/1> output_min <amp>`     | Set mininum output                                                            |
| `pid <0/1> output_max <amp>`     | Set maximum output                                                            |
| `s-h`                            | Show Steinhart-Hart equation parameters                                       |
| `s-h <0/1> <t0/b/r0> <value>`    | Set Steinhart-Hart parameter for a channel                                    |
| `postfilter`                     | Show postfilter settings                                                      |
| `postfilter <0/1> off`           | Disable postfilter                                                            |
| `postfilter <0/1> rate <rate>`   | Set postfilter output data rate                                               |
| `load [0/1]`                     | Restore configuration for channel all/0/1 from flash                          |
| `save [0/1]`                     | Save configuration for channel all/0/1 to flash                               |
| `reset`                          | Reset the device                                                              |
| `dfu`                            | Reset device and enters USB device firmware update (DFU) mode                 |
| `ipv4 <X.X.X.X/L> [Y.Y.Y.Y]`     | Configure IPv4 address, netmask length, and optional default gateway          |
| `fan`                            | Show current fan settings and sensors' measurements                           |
| `fan <value>`                    | Set fan power with values from 1 to 100                                       |
| `fan auto`                       | Enable automatic fan speed control                                            |
| `fcurve <a> <b> <c>`             | Set fan controller curve coefficients (see *Fan control* section)             |
| `fcurve default`                 | Set fan controller curve coefficients to defaults (see *Fan control* section) |
| `hwrev`                          | Show hardware revision, and settings related to it                            |


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

### 50/60 Hz filtering

The AD7172-2 ADC on the SENS inputs supports simultaneous rejection of
50 Hz ± 1 Hz and 60 Hz ± 1 Hz (dB). Affecting sampling rate, the
postfilter rate can be tuned with the `postfilter` command.

| Postfilter rate | Rejection | Effective sampling rate |
| ---             | :---:     | ---                     |
| 16.67 Hz        | 92 dB     | 8.4 Hz                  |
| 20 Hz           | 86 dB     | 9.1 Hz                  |
| 21.25 Hz        | 62 dB     | 10 Hz                   |
| 27 Hz           | 47 dB     | 10.41 Hz                |

## Thermo-Electric Cooling (TEC)

- Connect TEC module device 0 to TEC0- and TEC0+.
- Connect TEC module device 1 to TEC1- and TEC1+.
- The GND pin is for shielding not for sinking TEC module currents.

When using a TEC module with the Thermostat, the Thermostat expects the thermal load (where the thermistor is located) to heat up with a positive software current set point, and cool down with a negative current set point.

Testing heat flow direction with a low set current is recommended before installation of the TEC module.

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

Example: set the maximum negative current of channel 0 to -3 A.
```
pwm 0 max_i_neg 3
```

Example: set the maximum positive current of channel 1 to 3 A.
```
pwm 0 max_i_pos 3
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

## PID Tuning

The thermostat implements a PID control loop for each of the TEC channels, more details on setting up the PID control loop can be found [here](./doc/PID%20tuning.md).

## Fan control

Fan control is available for the thermostat revisions with integrated fan system. For this purpose four commands are available:
1. `fan` - show fan stats: `fan_pwm`, `abs_max_tec_i`, `auto_mode`, `k_a`, `k_b`, `k_c`.
2. `fan auto` - enable auto speed controller mode, which correlates with fan curve `fcurve`.
3. `fan <value>` - set the fan power with the value from `1` to `100` and disable auto mode. There is no way to disable the fan.
Please note that power doesn't correlate with the actual speed linearly.
4. `fcurve <a> <b> <c>` - set coefficients of the controlling curve `a*x^2 + b*x + c`, where `x` is `abs_max_tec_i/MAX_TEC_I`, 
i.e. receives values from 0 to 1 linearly tied to the maximum current. The controlling curve should produce values from 0 to 1,
as below and beyond values would be substituted by 0 and 1 respectively.
5. `fcurve default` - restore fan curve settings to defaults: `a = 1.0, b = 0.0, c = 0.0`.
