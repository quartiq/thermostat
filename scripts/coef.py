# SingularitySurfer 2021

import numpy as np
import matplotlib.pyplot as plt
from scipy import signal


def pi(fc, k_db, g_db, fs=1, plot=False):
    """
    Parameters
    ----------
    fc : cutoff frequency for integral gain (in rad/sec)
    k_db : K in db
    g_db : g in db
    fs: sampling freq (default normalized to 1)

    Returns
    -------
    ba : vector of b and a coefficients in format: [b0,b1,b2,a1,a2]
    """
    
    fcw = np.pi * fc * (1/fs)
    k = 10**((k_db)/20)
    g = 10**((g_db)/20)
    
    ba = [0,0,0,0,0]
    ba[0] = k * ((1+fcw)/(1 + (fcw/g)))
    ba[1] = -k * ((1-fcw)/(1 + (fcw/g)))
    ba[3] = (1 - (fcw/g))/(1 + (fcw/g))

    if plot:
        plt.figure(1)
        w, h = signal.freqz(ba[:3], [1] + [-x for x in ba[3:]], 2 ** 20)
        h = 20 * np.log10(np.abs(h))
        plt.plot(w, h)
        plt.xscale('log')
    return ba


def pd(fc, k_db, g_db, fs=1, plot=False):
    """
    Parameters
    ----------
    fc : cutoff frequency for differential gain (in rad/sec)
    k_db : K in db
    g_db : g in db
    fs: sampling freq (default normalized to 1)

    Returns
    -------
    ba : vector of b and a coefficients in format: [b0,b1,b2,a1,a2]
    """

    fcw = np.pi * fc * (1 / fs)
    k = 10 ** ((k_db) / 20)
    g = 10 ** ((g_db) / 20)

    ba = [0, 0, 0, 0, 0]
    ba[0] = k * ((1 + fcw) / ((1 / g) + fcw))
    ba[1] = -k * ((1 - fcw) / ((1 / g) + fcw))
    ba[3] = ((1 / g) - fcw) / ((1 / g) + fcw)

    if plot:
        plt.figure(1)
        w, h = signal.freqz(ba[:3], [1] + [-x for x in ba[3:]], 2 ** 20)
        h = 20 * np.log10(np.abs(h))
        plt.plot(w, h)
        plt.xscale('log')
    return ba


if __name__ == "__main__":
    ba_i=pi(0.0001, 0, 40, 1, True)
    pd(0.01, 0, 20, 1, True)