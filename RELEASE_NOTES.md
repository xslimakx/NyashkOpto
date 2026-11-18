# NyashkOpto v1.0.11

- Нові кнопки: клавіша сидить у заглибленні панелі, опукла матова, при ввімкненні світиться зсередини, а світло м'яко лягає на панель навколо (раніше світіння обрізалось прямокутною рамкою).

# NyashkOpto v1.0.10

- VU: замість круглого ковпачка — вигнута чорна накладка через усю ширину низу шкали, як у класичних VU-метрах; стрілка виходить з-під неї, напис режиму на накладці.

# NyashkOpto v1.0.9

- Стрілка VU тепер виходить з осі під ковпачком унизу шкали, як у справжнього VU-метра (раніше висіла посеред циферблата).

# NyashkOpto v1.0.8

- Шкали, риски, LED і підписи тепер згладжені (малюються в 4 рази більшими й зменшуються), без кривих країв.
- Світіння адитивне, як у справжніх LED: м'який ореол і яскраве ядро.
- Плавні анімації: кнопки загоряються й гаснуть, LED-кільця ковзають за ручкою, обідок навколо ручки плавно проявляється.
- Оптимізація: оновлення екрана 30 разів на секунду (Opto було 60), анімації не додають зайвих перемальовувань.

# NyashkOpto v1.0.7

- Деталі дизайну: стрілка ручки починається від краю ковпачка (як справжня гравіровка), бокові стінки металевих ручок ловлять світло, навколо ручки, яку крутиш або на яку наводиш, світиться тонкий обідок, LED-кільця без зайвих рисок на маленьких ручках, мітки пікового рівня на LED-шкалах, більші підписи.

# NyashkOpto v1.0.6

- Новий дизайн: сучасне аналогове залізо. Корпус, ручки, кнопки й екрани відрендерені як справжні фізичні об'єкти: точені алюмінієві ручки з рифленою стінкою, тіні в три шари, анодований метал з подряпинами, пилом і потертими краями, кнопки з підсвіткою, LED-дуги навколо ручок, скло на екранах.
- Ручки: тягни вгору / вниз (Shift — точно), колесо миші, подвійний клік — значення за замовчуванням, правий клік — ввести число. Значення показується при наведенні.

# NyashkOpto v1.0.5

- **No colouring of hot signals:** the output safety ceiling started at -0.45 dBFS and softly squashed any peak above it, even with Peak Reduction at 0. It now starts at -0.04 dBFS, so anything that already fits passes unchanged.
- New behaviour tests: gain reduction rises steadily with Peak Reduction, Compress ~3:1 / Limit ~12:1, attack ~8 ms, release 50 % in ~120 ms with a slow tail of seconds, HF follow only affects highs, linked stereo, same result at 44.1 and 96 kHz, hum only at the chosen mains frequency.

# NyashkOpto v1.0.4

- **Waveform without breaks:** the live waveform was drawn as one thin line per column, which overlapped unevenly on the pixel grid and showed stripes and gaps. It is now one continuous filled band for the input and one for the output.
- The waveform scrolls smoothly between audio blocks (and the window redraws at 60 fps).

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
