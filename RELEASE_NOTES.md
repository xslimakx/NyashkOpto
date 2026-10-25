# NyashkOpto v1.0.1

Sound-quality pass on the optical compressor.

- **Transparent at Peak Reduction 0:** the threshold slides up below 20, so the compressor really does nothing at 0, even on full-scale peaks (new test).
- **Cleaner ANALOG mode:** the soft saturation uses an anti-aliased (first-order antiderivative) tanh, so it no longer adds aliasing; DC from the asymmetric stage is removed.
- **Click-free BYPASS:** about 5 ms crossfade instead of a hard switch (new test).
- **Output ceiling:** gain, trim and saturation can never push the signal over 0 dBFS (soft ceiling at about -0.5 dBFS).
- **macOS:** the resize grip uses host units, so the window resizes correctly on Retina displays.
