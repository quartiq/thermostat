import math
import logging
from collections import deque, namedtuple
from enum import Enum

from pytec.client import Client

# Based on hirshmann pid-autotune libiary
# See https://github.com/hirschmann/pid-autotune
# Which is in turn based on a fork of Arduino PID AutoTune Library
# See https://github.com/t0mpr1c3/Arduino-PID-AutoTune-Library


class PIDAutotuneState(Enum):
    STATE_OFF = 'off'
    STATE_RELAY_STEP_UP = 'relay step up'
    STATE_RELAY_STEP_DOWN = 'relay step down'
    STATE_SUCCEEDED = 'succeeded'
    STATE_FAILED = 'failed'


class PIDAutotune:
    PIDParams = namedtuple('PIDParams', ['Kp', 'Ki', 'Kd'])

    PEAK_AMPLITUDE_TOLERANCE = 0.05

    _tuning_rules = {
        "ziegler-nichols": [0.6, 1.2, 0.075],
        "tyreus-luyben": [0.4545,  0.2066, 0.07214],
        "ciancone-marlin": [0.303, 0.1364, 0.0481],
        "pessen-integral": [0.7, 1.75, 0.105],
        "some-overshoot": [0.333, 0.667,  0.111],
        "no-overshoot": [0.2, 0.4,  0.0667]
    }

    def __init__(self, setpoint, out_step=10, lookback=60,
                 noiseband=0.5, sampletime=1.2):
        if setpoint is None:
            raise ValueError('setpoint must be specified')

        self._inputs = deque(maxlen=round(lookback / sampletime))
        self._setpoint = setpoint
        self._outputstep = out_step
        self._noiseband = noiseband
        self._out_min = -out_step
        self._out_max = out_step
        self._state = PIDAutotuneState.STATE_OFF
        self._peak_timestamps = deque(maxlen=5)
        self._peaks = deque(maxlen=5)
        self._output = 0
        self._last_run_timestamp = 0
        self._peak_type = 0
        self._peak_count = 0
        self._initial_output = 0
        self._induced_amplitude = 0
        self._Ku = 0
        self._Pu = 0

    def state(self):
        """Get the current state."""
        return self._state

    def output(self):
        """Get the last output value."""
        return self._output

    def tuning_rules(self):
        """Get a list of all available tuning rules."""
        return self._tuning_rules.keys()

    def get_pid_parameters(self, tuning_rule='ziegler-nichols'):
        """Get PID parameters.

        Args:
            tuning_rule (str): Sets the rule which should be used to calculate
                the parameters.
        """
        divisors = self._tuning_rules[tuning_rule]
        kp = self._Ku * divisors[0]
        ki = divisors[1] * self._Ku / self._Pu
        kd = divisors[2] * self._Ku * self._Pu
        return PIDAutotune.PIDParams(kp, ki, kd)

    def run(self, input_val, time_input):
        """To autotune a system, this method must be called periodically.

        Args:
            input_val (float): The temperature input value.
            time_input (float): Current time in seconds.

        Returns:
            `true` if tuning is finished, otherwise `false`.
        """
        now = time_input * 1000

        if (self._state == PIDAutotuneState.STATE_OFF
                or self._state == PIDAutotuneState.STATE_SUCCEEDED
                or self._state == PIDAutotuneState.STATE_FAILED):
            self._state = PIDAutotuneState.STATE_RELAY_STEP_UP

        self._last_run_timestamp = now

        # check input and change relay state if necessary
        if (self._state == PIDAutotuneState.STATE_RELAY_STEP_UP
                and input_val > self._setpoint + self._noiseband):
            self._state = PIDAutotuneState.STATE_RELAY_STEP_DOWN
            logging.debug('switched state: {0}'.format(self._state))
            logging.debug('input: {0}'.format(input_val))
        elif (self._state == PIDAutotuneState.STATE_RELAY_STEP_DOWN
                and input_val < self._setpoint - self._noiseband):
            self._state = PIDAutotuneState.STATE_RELAY_STEP_UP
            logging.debug('switched state: {0}'.format(self._state))
            logging.debug('input: {0}'.format(input_val))

        # set output
        if (self._state == PIDAutotuneState.STATE_RELAY_STEP_UP):
            self._output = self._initial_output - self._outputstep
        elif self._state == PIDAutotuneState.STATE_RELAY_STEP_DOWN:
            self._output = self._initial_output + self._outputstep

        # respect output limits
        self._output = min(self._output, self._out_max)
        self._output = max(self._output, self._out_min)

        # identify peaks
        is_max = True
        is_min = True

        for val in self._inputs:
            is_max = is_max and (input_val >= val)
            is_min = is_min and (input_val <= val)

        self._inputs.append(input_val)

        # we don't trust the maxes or mins until the input array is full
        if len(self._inputs) < self._inputs.maxlen:
            return False

        # increment peak count and record peak time for maxima and minima
        inflection = False

        # peak types:
        # -1: minimum
        # +1: maximum
        if is_max:
            if self._peak_type == -1:
                inflection = True
            self._peak_type = 1
        elif is_min:
            if self._peak_type == 1:
                inflection = True
            self._peak_type = -1

        # update peak times and values
        if inflection:
            self._peak_count += 1
            self._peaks.append(input_val)
            self._peak_timestamps.append(now)
            logging.debug('found peak: {0}'.format(input_val))
            logging.debug('peak count: {0}'.format(self._peak_count))

        # check for convergence of induced oscillation
        # convergence of amplitude assessed on last 4 peaks (1.5 cycles)
        self._induced_amplitude = 0

        if inflection and (self._peak_count > 4):
            abs_max = self._peaks[-2]
            abs_min = self._peaks[-2]
            for i in range(0, len(self._peaks) - 2):
                self._induced_amplitude += abs(self._peaks[i]
                                               - self._peaks[i+1])
                abs_max = max(self._peaks[i], abs_max)
                abs_min = min(self._peaks[i], abs_min)

            self._induced_amplitude /= 6.0

            # check convergence criterion for amplitude of induced oscillation
            amplitude_dev = ((0.5 * (abs_max - abs_min)
                              - self._induced_amplitude)
                             / self._induced_amplitude)

            logging.debug('amplitude: {0}'.format(self._induced_amplitude))
            logging.debug('amplitude deviation: {0}'.format(amplitude_dev))

            if amplitude_dev < PIDAutotune.PEAK_AMPLITUDE_TOLERANCE:
                self._state = PIDAutotuneState.STATE_SUCCEEDED

        # if the autotune has not already converged
        # terminate after 10 cycles
        if self._peak_count >= 20:
            self._output = 0
            self._state = PIDAutotuneState.STATE_FAILED
            return True

        if self._state == PIDAutotuneState.STATE_SUCCEEDED:
            self._output = 0
            logging.debug('peak finding successful')

            # calculate ultimate gain
            self._Ku = 4.0 * self._outputstep / \
                (self._induced_amplitude * math.pi)
            print('Ku: {0}'.format(self._Ku))

            # calculate ultimate period in seconds
            period1 = self._peak_timestamps[3] - self._peak_timestamps[1]
            period2 = self._peak_timestamps[4] - self._peak_timestamps[2]
            self._Pu = 0.5 * (period1 + period2) / 1000.0
            print('Pu: {0}'.format(self._Pu))

            for rule in self._tuning_rules:
                params = self.get_pid_parameters(rule)
                print('rule: {0}'.format(rule))
                print('Kp: {0}'.format(params.Kp))
                print('Ki: {0}'.format(params.Ki))
                print('Kd: {0}'.format(params.Kd))

            return True
        return False


def main():
    # Auto tune parameters
    # Thermostat channel
    channel = 0
    # Target temperature of the autotune routine, celcius
    target_temperature = 20
    # Value by which output will be increased/decreased from zero, amps
    output_step = 1
    # Reference period for local minima/maxima, seconds
    lookback = 3
    # Determines by how much the input value must
    # overshoot/undershoot the setpoint, celcius
    noiseband = 1.5

    # logging.basicConfig(level=logging.DEBUG)

    tec = Client()

    data = next(tec.report_mode())
    ch = data[channel]

    tuner = PIDAutotune(target_temperature, output_step,
                        lookback, noiseband, ch['interval'])

    for data in tec.report_mode():

        ch = data[channel]

        temperature = ch['temperature']

        if (tuner.run(temperature, ch['time'])):
            break

        tuner_out = tuner.output()

        tec.set_param("pwm", channel, "i_set", tuner_out)

    tec.set_param("pwm", channel, "i_set", 0)


if __name__ == "__main__":
    main()
