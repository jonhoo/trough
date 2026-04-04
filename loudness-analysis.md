# Volume Normalization Analysis

## Background

RustFFT's inverse FFT is unnormalized — the output is scaled by
N=44100. By Parseval's theorem for the unnormalized IFFT, the
time-domain RMS of the output signal is:

    RMS = sqrt(2 × Σ |A(k)|²)    for k = 1..22049

where `A(k)` is the amplitude set for positive-frequency bin k. The
factor of 2 comes from conjugate symmetry (each positive bin's energy
appears twice — once in the positive half and once in the mirrored
negative half). DC and Nyquist bins are both zero.

The current code uses `avg_amplitude = 8.0` as a shared baseline, but
each color applies its own ad-hoc normalization. The existing TODO
comment in the brownian section acknowledges this:

> TODO: normalize this (and everything) by computing area under the
> curve of amplitudes (maybe of power density instead?) and normalizing
> that to a particular number. you know, instead of eyeballing "brownian
> is ~4x quieter than pink".

## Per-color derivations

### White

All 22049 bins set to `avg_amplitude = 8`:

    Σ A² = 22049 × 64 = 1,411,136
    RMS = sqrt(2 × 1,411,136) ≈ 1680

### Pink

`A(k) = sqrt(power / (k+1))` for bins 21–22049, where
`power = (8 × √11025)² = 705,600`:

    Σ A² = 705600 × Σ_{f=21}^{22049} 1/f
         ≈ 705600 × ln(22049/20) ≈ 705600 × 7.005
         ≈ 4,942,728
    RMS ≈ sqrt(2 × 4,942,728) ≈ 3144

### Brownian

`A(k) = (21/(k+1)) × pink_max_amplitude` for bins 21–22049, where
`pink_max_amplitude ≈ 751.3`:

    Σ A² = 751.3² × 21² × Σ_{f=21}^{22049} 1/f²
         ≈ 564,453 × 441 × 0.0488
         ≈ 12,146,000
    RMS ≈ sqrt(2 × 12,146,000) ≈ 4928

### Blue

`A(k) = sqrt(power × (k+1))` for all bins, where
`power = (8/√11025)² ≈ 0.00581`:

    Σ A² = 0.00581 × Σ_{f=1}^{22049} f
         ≈ 0.00581 × 243,090,225
         ≈ 1,411,154
    RMS ≈ sqrt(2 × 1,411,154) ≈ 1680

### Violet

`A(k) = power × (k+1)` (amplitude grows linearly, not as sqrt), where
`power ≈ 0.000363`:

    Σ A² = 0.000363² × Σ_{f=1}^{22049} f²
         ≈ 1.318e-7 × 3.57e12
         ≈ 470,400
    RMS ≈ sqrt(2 × 470,400) ≈ 970

### Grey

Inverse A-weighting produces wildly varying amplitudes. At key
frequencies:

| Frequency | A-weight (dB) | Target amplitude |
|-----------|---------------|------------------|
| 21 Hz     | ≈ -50         | ≈ 2500           |
| 100 Hz    | ≈ -19         | ≈ 72             |
| 1000 Hz   | 0             | 8                |
| 10000 Hz  | ≈ -2.5        | ≈ 11             |

The ~10 bins near 20 Hz dominate the energy sum. Exact integration
requires numerical computation; the measured value is ~7970 RMS (i16
scale).

## Measured results

Generated 5 seconds of each color and measured with `sox`:

| Color    | RMS (sox) | RMS (i16) | Predicted | vs White | Peak (i16) | Peak % |
|----------|-----------|-----------|-----------|----------|------------|--------|
| White    | 0.0505    | 1,655     | 1,680     | 1.0×     | 8,052      | 25%    |
| Blue     | 0.0505    | 1,655     | 1,680     | 1.0×     | 7,525      | 23%    |
| Violet   | 0.0292    | 955       | 970       | 0.58×    | 4,233      | 13%    |
| Pink     | 0.0943    | 3,090     | 3,144     | 1.87×    | 14,123     | 43%    |
| Brownian | 0.1481    | 4,855     | 4,928     | 2.93×    | 16,092     | 49%    |
| Grey     | 0.2432    | 7,970     | —         | 4.82×    | 29,131     | 89%    |

Predictions match within ~2% for all analytically-computed colors.

## Findings

1. **White and blue are well-matched** — the symmetric normalization
   works as intended (both ≈ 1680 RMS).

2. **Violet is 42% quieter** than white. The `/ 4` fudge factor in its
   normalization over-attenuates relative to the other colors.

3. **Pink is 1.87× louder** than white.

4. **Brownian is 2.93× louder** than white. The `* 4` fudge factor in
   the code doesn't fully compensate (acknowledged by the TODO).

5. **Grey is 4.82× louder** than white and peaks at 89% of full scale.
   Not clipping at 5 seconds, but dangerously close — a longer duration
   or different random seed could clip.

6. The overall volume spread is **~5× from quietest (violet) to loudest
   (grey)**, approximately 14 dB.
