# trough

A command-line tool that generates colored audio noise as WAV files.

Supports white, pink, brownian, blue, violet, and grey noise. Each color
has a distinct frequency power distribution. For example, white noise
distributes energy evenly across frequencies, pink noise rolls off at 3
dB/octave, brownian at 6 dB/octave, and so on. Grey noise is A-weighted
to match human hearing.

## Usage

```
trough [-d|--duration=SECONDS] COLOR
```

`COLOR` is one of `white`, `pink`, `brownian`, `blue`, `violet`, or
`grey`. Output is written to `audio.wav` in the current directory.
Duration defaults to 20 seconds.

The implementation generates noise in the frequency domain (setting
per-bin amplitudes according to the desired spectral shape with random
phases), then uses an inverse FFT to produce time-domain PCM samples,
which are written directly into the RIFF/WAVE container format.

## Live-stream

This project was built from scratch during a [live-stream][stream] as an
exercise in going back to basics: no LLM assistance, no high-level
abstractions, just reading specs and working through the math by hand.
Topics covered include the RIFF/WAVE file format, PCM audio encoding,
colored noise theory, and inverse FFT-based synthesis.

The stream ended at https://github.com/jonhoo/trough/tree/33623289870346696724769cded6dc2bbb07fc1d.

After the stream, I ran the hand-written code through Claude Code for an
audit and polish pass. Its access to domain-knowledge meant that it
caught both a few correctness issues and identified some good
improvements (including good contextual comments). The analysis,
prompts, and resulting changes are all in #1, and is worth a read even
(or perhaps *especially*) if you're skeptical of the utility of LLMs and
agentic coding!

[stream]: https://youtu.be/zOTE4BN59u4
