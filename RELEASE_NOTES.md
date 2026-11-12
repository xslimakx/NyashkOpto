# NyashkOpto v1.0.3

- **Alt-tab fix (Linux):** after alt-tab, minimising or covering the window, parts of the picture (backgrounds, knobs, images) could stay blank until something on them moved. The window is now drawn again in full whenever the system throws its contents away, and pictures are no longer lost while the window is hidden or a frame is skipped.
- Smoother images in the CPU renderer (used when Vulkan is not available): pictures drawn smaller than their real size, such as knobs and photos, are now scaled with mipmaps instead of picking single pixels, so they no longer look grainy or jagged.
- **Display fix:** the built-in CPU renderer (used when Vulkan is not available) drew pixels on the shared edges of triangles twice, which showed as brighter seams and specks in see-through fills (spectra, curves, bars). It now uses an exact fill rule like a GPU: every pixel is drawn once, without holes.
- VU meter: the needle's pivot sits under a cover like on a real meter, so the needle no longer crosses the GAIN REDUCTION / INPUT / OUTPUT label.
- Value boxes have a small gap between them, and a value that would not fit its box is drawn smaller instead of running over the frame.
- Clicking a value box and pressing Enter no longer changes a value whose unit matters (for example "1.03 s" was read back as 1.03 ms).
- New automatic test: every parameter's text is typed back over its whole range and must give the same value.

# NyashkOpto v1.0.2

- **Live waveform:** the last 4 seconds of input (grey) and output (white) with the gain reduction line hanging from the top, so you can see the compressor working.

# NyashkOpto v1.0.1

Sound-quality pass on the optical compressor.

- **Transparent at Peak Reduction 0:** the threshold slides up below 20, so the compressor really does nothing at 0, even on full-scale peaks (new test).
- **Cleaner ANALOG mode:** the soft saturation uses an anti-aliased (first-order antiderivative) tanh, so it no longer adds aliasing; DC from the asymmetric stage is removed.
- **Click-free BYPASS:** about 5 ms crossfade instead of a hard switch (new test).
- **Output ceiling:** gain, trim and saturation can never push the signal over 0 dBFS (soft ceiling at about -0.5 dBFS).
- **macOS:** the resize grip uses host units, so the window resizes correctly on Retina displays.
